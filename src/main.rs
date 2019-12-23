use actix_web::{App, HttpServer};
use handlebars::Handlebars;
use hotwatch::{Event, Hotwatch};
use serde::Deserialize;
use std::collections::HashMap;
use std::fmt;
use std::fs::{read_to_string, OpenOptions};
use std::io::Write;
use std::sync::{Arc, RwLock};
use std::time::Duration;

mod routes;
mod template_args;

static DEFAULT_CONFIG: &[u8] = include_bytes!("../bunbun.default.toml");
static CONFIG_FILE: &str = "bunbun.toml";

#[derive(Debug)]
#[allow(clippy::enum_variant_names)]
enum BunBunError {
  IoError(std::io::Error),
  ParseError(serde_yaml::Error),
  WatchError(hotwatch::Error),
}

impl fmt::Display for BunBunError {
  fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
    match self {
      BunBunError::IoError(e) => e.fmt(f),
      BunBunError::ParseError(e) => e.fmt(f),
      BunBunError::WatchError(e) => e.fmt(f),
    }
  }
}

/// Generates a from implementation from the specified type to the provided
/// bunbun error.
macro_rules! from_error {
  ($from:ty, $to:ident) => {
    impl From<$from> for BunBunError {
      fn from(e: $from) -> Self {
        BunBunError::$to(e)
      }
    }
  };
}

from_error!(std::io::Error, IoError);
from_error!(serde_yaml::Error, ParseError);
from_error!(hotwatch::Error, WatchError);

/// Dynamic variables that either need to be present at runtime, or can be
/// changed during runtime.
pub struct State {
  public_address: String,
  default_route: Option<String>,
  routes: HashMap<String, String>,
  renderer: Handlebars,
}

fn main() -> Result<(), BunBunError> {
  let conf = read_config(CONFIG_FILE)?;
  let renderer = compile_templates();
  let state = Arc::from(RwLock::new(State {
    public_address: conf.public_address,
    default_route: conf.default_route,
    routes: conf.routes,
    renderer,
  }));
  let state_ref = state.clone();

  let mut watch = Hotwatch::new_with_custom_delay(Duration::from_millis(500))?;

  watch.watch(CONFIG_FILE, move |e: Event| {
    if let Event::Write(_) = e {
      let mut state = state.write().unwrap();
      match read_config(CONFIG_FILE) {
        Ok(conf) => {
          state.public_address = conf.public_address;
          state.default_route = conf.default_route;
          state.routes = conf.routes;
        }
        Err(e) => eprintln!("Config is malformed: {}", e),
      }
    }
  })?;

  HttpServer::new(move || {
    App::new()
      .data(state_ref.clone())
      .service(routes::hop)
      .service(routes::list)
      .service(routes::index)
      .service(routes::opensearch)
  })
  .bind(&conf.bind_address)?
  .run()?;

  Ok(())
}

#[derive(Deserialize)]
struct Config {
  bind_address: String,
  public_address: String,
  default_route: Option<String>,
  routes: HashMap<String, String>,
}

/// Attempts to read the config file. If it doesn't exist, generate one a
/// default config file before attempting to parse it.
fn read_config(config_file_path: &str) -> Result<Config, BunBunError> {
  let config_str = match read_to_string(config_file_path) {
    Ok(conf_str) => conf_str,
    Err(_) => {
      eprintln!(
        "Unable to find a {} file. Creating default!",
        config_file_path
      );
      let mut fd = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(config_file_path)
        .expect("Unable to write to directory!");
      fd.write_all(DEFAULT_CONFIG)?;
      String::from_utf8_lossy(DEFAULT_CONFIG).into_owned()
    }
  };

  // Reading from memory is faster than reading directly from a reader for some
  // reason; see https://github.com/serde-rs/json/issues/160
  Ok(serde_yaml::from_str(&config_str)?)
}

/// Returns an instance with all pre-generated templates included into the
/// binary. This allows for users to have a portable binary without needed the
/// templates at runtime.
fn compile_templates() -> Handlebars {
  let mut handlebars = Handlebars::new();
  macro_rules! register_template {
    [ $( $template:expr ),* ] => {
      $(
        handlebars
          .register_template_string(
            $template,
            String::from_utf8_lossy(
              include_bytes!(concat!("templates/", $template, ".hbs")))
          )
          .unwrap();
      )*
    };
  }
  register_template!["index", "list", "opensearch"];
  handlebars
}
