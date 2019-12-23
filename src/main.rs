use actix_web::{
  get,
  http::header,
  web::{Data, Query},
  App, HttpResponse, HttpServer, Responder,
};
use handlebars::Handlebars;
use hotwatch::{Event, Hotwatch};
use serde::Deserialize;
use std::collections::{BTreeMap, HashMap};
use std::fmt;
use std::fs::{File, OpenOptions};
use std::io::Write;
use std::sync::{Arc, RwLock};
use std::time::Duration;

static DEFAULT_CONFIG: &[u8] = include_bytes!("../bunbun.default.toml");
static CONFIG_FILE: &str = "bunbun.toml";

#[derive(Debug)]
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
  let mut raw_args = query.to.split_ascii_whitespace();
  let command = raw_args.next();

  if command.is_none() {
    return HttpResponse::NotFound().body("not found");
  }

  // Reform args into url-safe string (probably want to go thru an actual parser)
  let mut args = String::new();
  if let Some(first_arg) = raw_args.next() {
    args.push_str(first_arg);
    for arg in raw_args {
      args.push_str("+");
      args.push_str(arg);
    }
  }

  let mut template_args = HashMap::new();
  template_args.insert("query", args);

  match data.routes.get(command.unwrap()) {
    Some(template) => HttpResponse::Found()
      .header(
        header::LOCATION,
        data
          .renderer
          .render_template(template, &template_args)
          .unwrap(),
      )
      .finish(),
    None => match &data.default_route {
      Some(route) => {
        template_args.insert(
          "query",
          format!(
            "{}+{}",
            command.unwrap(),
            template_args.get("query").unwrap()
          ),
        );
        HttpResponse::Found()
          .header(
            header::LOCATION,
            data
              .renderer
              .render_template(data.routes.get(route).unwrap(), &template_args)
              .unwrap(),
          )
          .finish()
      }
      None => HttpResponse::NotFound().body("not found"),
    },
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
  routes: BTreeMap<String, String>,
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
  routes: BTreeMap<String, String>,
}

/// Attempts to read the config file. If it doesn't exist, generate one a
/// default config file before attempting to parse it.
fn read_config(config_file_path: &str) -> Result<Config, BunBunError> {
  let config_file = match File::open(config_file_path) {
    Ok(file) => file,
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
      File::open(config_file_path)?
    }
  };
  Ok(serde_yaml::from_reader(config_file)?)
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
