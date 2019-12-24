use actix_web::middleware::Logger;
use actix_web::{App, HttpServer};
use clap::{crate_authors, crate_version, load_yaml, App as ClapApp};
use handlebars::Handlebars;
use hotwatch::{Event, Hotwatch};
use libc::daemon;
use log::{debug, error, info, trace, warn};
use serde::Deserialize;
use std::cmp::min;
use std::collections::HashMap;
use std::fmt;
use std::fs::{read_to_string, OpenOptions};
use std::io::Write;
use std::sync::{Arc, RwLock};
use std::time::Duration;

mod routes;
mod template_args;

static DEFAULT_CONFIG: &[u8] = include_bytes!("../bunbun.default.toml");

#[derive(Debug)]
#[allow(clippy::enum_variant_names)]
enum BunBunError {
  IoError(std::io::Error),
  ParseError(serde_yaml::Error),
  WatchError(hotwatch::Error),
  LoggerInitError(log::SetLoggerError),
}

impl fmt::Display for BunBunError {
  fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
    match self {
      BunBunError::IoError(e) => e.fmt(f),
      BunBunError::ParseError(e) => e.fmt(f),
      BunBunError::WatchError(e) => e.fmt(f),
      BunBunError::LoggerInitError(e) => e.fmt(f),
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
from_error!(log::SetLoggerError, LoggerInitError);

/// Dynamic variables that either need to be present at runtime, or can be
/// changed during runtime.
pub struct State {
  public_address: String,
  default_route: Option<String>,
  routes: HashMap<String, String>,
  renderer: Handlebars,
}

fn main() -> Result<(), BunBunError> {
  let yaml = load_yaml!("cli.yaml");
  let matches = ClapApp::from(yaml)
    .version(crate_version!())
    .author(crate_authors!())
    .get_matches();

  let log_level = match min(matches.occurrences_of("verbose"), 3) as i8
    - min(matches.occurrences_of("quiet"), 2) as i8
  {
    -2 => None,
    -1 => Some(log::Level::Error),
    0 => Some(log::Level::Warn),
    1 => Some(log::Level::Info),
    2 => Some(log::Level::Debug),
    3 => Some(log::Level::Trace),
    _ => unreachable!(),
  };

  if let Some(level) = log_level {
    simple_logger::init_with_level(level)?;
  }

  // config has default location provided
  let conf_file_location = String::from(matches.value_of("config").unwrap());
  let conf = read_config(&conf_file_location)?;
  let renderer = compile_templates();
  let state = Arc::from(RwLock::new(State {
    public_address: conf.public_address,
    default_route: conf.default_route,
    routes: conf.routes,
    renderer,
  }));

  // Daemonize after trying to read from config and before watching; allow user
  // to see a bad config (daemon process sets std{in,out} to /dev/null)
  if matches.is_present("daemon") {
    unsafe {
      debug!("Daemon flag provided. Running as a daemon.");
      daemon(0, 0);
    }
  }

  let mut watch = Hotwatch::new_with_custom_delay(Duration::from_millis(500))?;
  // TODO: keep retry watching in separate thread

  // Closures need their own copy of variables for proper lifecycle management
  let state_ref = state.clone();
  let conf_file_location_clone = conf_file_location.clone();
  let watch_result = watch.watch(&conf_file_location, move |e: Event| {
    if let Event::Write(_) = e {
      trace!("Grabbing writer lock on state...");
      let mut state = state.write().unwrap();
      trace!("Obtained writer lock on state!");
      match read_config(&conf_file_location_clone) {
        Ok(conf) => {
          state.public_address = conf.public_address;
          state.default_route = conf.default_route;
          state.routes = conf.routes;
          info!("Successfully updated active state");
        }
        Err(e) => warn!("Failed to update config file: {}", e),
      }
    } else {
      debug!("Saw event {:#?} but ignored it", e);
    }
  });

  match watch_result {
    Ok(_) => info!("Watcher is now watching {}", &conf_file_location),
    Err(e) => warn!(
      "Couldn't watch {}: {}. Changes to this file won't be seen!",
      &conf_file_location, e
    ),
  }

  HttpServer::new(move || {
    App::new()
      .data(state_ref.clone())
      .wrap(Logger::default())
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
  trace!("Loading config file...");
  let config_str = match read_to_string(config_file_path) {
    Ok(conf_str) => {
      debug!("Successfully loaded config file into memory.");
      conf_str
    }
    Err(_) => {
      info!(
        "Unable to find a {} file. Creating default!",
        config_file_path
      );

      let fd = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(config_file_path);

      match fd {
        Ok(mut fd) => fd.write_all(DEFAULT_CONFIG)?,
        Err(e) => {
          error!("Failed to write to {}: {}. Default config will be loaded but not saved.", config_file_path, e);
        }
      };

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
        debug!("Loaded {} template.", $template);
      )*
    };
  }
  register_template!["index", "list", "opensearch"];
  handlebars
}
