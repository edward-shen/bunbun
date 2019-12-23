use actix_web::{
  get,
  http::header,
  web::{Data, Query},
  App, HttpResponse, HttpServer, Responder,
};
use handlebars::Handlebars;
use hotwatch::{Event, Hotwatch};
use itertools::Itertools;
use percent_encoding::{utf8_percent_encode, AsciiSet, CONTROLS};
use serde::Deserialize;
use std::collections::HashMap;
use std::fmt;
use std::fs::{read_to_string, OpenOptions};
use std::io::Write;
use std::sync::{Arc, RwLock};
use std::time::Duration;

/// https://url.spec.whatwg.org/#fragment-percent-encode-set
static FRAGMENT_ENCODE_SET: &AsciiSet =
  &CONTROLS.add(b' ').add(b'"').add(b'<').add(b'>').add(b'`');
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

#[get("/ls")]
fn list(data: Data<Arc<RwLock<State>>>) -> impl Responder {
  let data = data.read().unwrap();
  HttpResponse::Ok().body(data.renderer.render("list", &data.routes).unwrap())
}

#[derive(Deserialize)]
struct SearchQuery {
  to: String,
}

#[get("/hop")]
fn hop(
  data: Data<Arc<RwLock<State>>>,
  query: Query<SearchQuery>,
) -> impl Responder {
  let data = data.read().unwrap();

  match resolve_hop(&query.to, &data.routes, &data.default_route) {
    (Some(path), args) => {
      let mut template_args = HashMap::new();
      template_args.insert(
        "query",
        utf8_percent_encode(&args, FRAGMENT_ENCODE_SET).to_string(),
      );

      HttpResponse::Found()
        .header(
          header::LOCATION,
          data
            .renderer
            .render_template(&path, &template_args)
            .unwrap(),
        )
        .finish()
    }
    (None, _) => HttpResponse::NotFound().body("not found"),
  }
}

/// Attempts to resolve the provided string into its route and its arguments.
/// If a default route was provided, then this will consider that route before
/// failing to resolve a route.
///
/// The first element in the tuple describes the route, while the second element
/// returns the remaining arguments. If none remain, an empty string is given.
fn resolve_hop(
  query: &str,
  routes: &HashMap<String, String>,
  default_route: &Option<String>,
) -> (Option<String>, String) {
  let mut split_args = query.split_ascii_whitespace().peekable();
  let command = match split_args.peek() {
    Some(command) => command,
    None => return (None, String::new()),
  };

  match (routes.get(*command), default_route) {
    // Found a route
    (Some(resolved), _) => (
      Some(resolved.clone()),
      match split_args.next() {
        // Discard the first result, we found the route using the first arg
        Some(_) => split_args.join(" "),
        None => String::new(),
      },
    ),
    // Unable to find route, but had a default route
    (None, Some(route)) => (routes.get(route).cloned(), split_args.join(" ")),
    // No default route and no match
    (None, None) => (None, String::new()),
  }
}

#[get("/")]
fn index(data: Data<Arc<RwLock<State>>>) -> impl Responder {
  let data = data.read().unwrap();
  let mut template_args = HashMap::new();
  template_args.insert("hostname", &data.public_address);
  HttpResponse::Ok()
    .body(data.renderer.render("index", &template_args).unwrap())
}

#[get("/bunbunsearch.xml")]
fn opensearch(data: Data<Arc<RwLock<State>>>) -> impl Responder {
  let data = data.read().unwrap();
  let mut template_args = HashMap::new();
  template_args.insert("hostname", &data.public_address);
  HttpResponse::Ok()
    .header(
      header::CONTENT_TYPE,
      "application/opensearchdescription+xml",
    )
    .body(data.renderer.render("opensearch", &template_args).unwrap())
}

/// Dynamic variables that either need to be present at runtime, or can be
/// changed during runtime.
struct State {
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
      .service(hop)
      .service(list)
      .service(index)
      .service(opensearch)
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
