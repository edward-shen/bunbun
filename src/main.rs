use actix_web::middleware::Logger;
use actix_web::{App, HttpServer};
use clap::{crate_authors, crate_version, load_yaml, App as ClapApp};
use handlebars::Handlebars;
use hotwatch::{Event, Hotwatch};
use libc::daemon;
use log::{debug, error, info, trace, warn};
use serde::{Deserialize, Serialize};
use std::cmp::min;
use std::collections::HashMap;
use std::fmt;
use std::fs::{read_to_string, OpenOptions};
use std::io::Write;
use std::sync::{Arc, RwLock};
use std::time::Duration;

mod routes;
mod template_args;

const DEFAULT_CONFIG: &[u8] = include_bytes!("../bunbun.default.yaml");

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
  groups: Vec<RouteGroup>,
  /// Cached, flattened mapping of all routes and their destinations.
  routes: HashMap<String, String>,
}

#[actix_rt::main]
async fn main() -> Result<(), BunBunError> {
  let yaml = load_yaml!("cli.yaml");
  let matches = ClapApp::from(yaml)
    .version(crate_version!())
    .author(crate_authors!())
    .get_matches();

  init_logger(
    matches.occurrences_of("verbose"),
    matches.occurrences_of("quiet"),
  )?;

  // config has default location provided, unwrapping is fine.
  let conf_file_location = String::from(matches.value_of("config").unwrap());
  let conf = read_config(&conf_file_location)?;
  let state = Arc::from(RwLock::new(State {
    public_address: conf.public_address,
    default_route: conf.default_route,
    routes: cache_routes(&conf.groups),
    groups: conf.groups,
  }));

  // Daemonize after trying to read from config and before watching; allow user
  // to see a bad config (daemon process sets std{in,out} to /dev/null)
  if matches.is_present("daemon") {
    unsafe {
      debug!("Daemon flag provided. Running as a daemon.");
      daemon(0, 0);
    }
  }

  let _watch = start_watch(state.clone(), conf_file_location)?;

  HttpServer::new(move || {
    App::new()
      .data(state.clone())
      .app_data(compile_templates())
      .wrap(Logger::default())
      .service(routes::hop)
      .service(routes::list)
      .service(routes::index)
      .service(routes::opensearch)
  })
  .bind(&conf.bind_address)?
  .run()
  .await?;

  Ok(())
}

/// Initializes the logger based on the number of quiet and verbose flags passed
/// in. Usually, these values are mutually exclusive, that is, if the number of
/// verbose flags is non-zero then the quiet flag is zero, and vice versa.
fn init_logger(
  num_verbose_flags: u64,
  num_quiet_flags: u64,
) -> Result<(), BunBunError> {
  let log_level =
    match min(num_verbose_flags, 3) as i8 - min(num_quiet_flags, 2) as i8 {
      -2 => None,
      -1 => Some(log::Level::Error),
      0 => Some(log::Level::Warn),
      1 => Some(log::Level::Info),
      2 => Some(log::Level::Debug),
      3 => Some(log::Level::Trace),
      _ => unreachable!(), // values are clamped to [0, 3] - [0, 2]
    };

  if let Some(level) = log_level {
    simple_logger::init_with_level(level)?;
  }

  Ok(())
}

#[derive(Deserialize)]
struct Config {
  bind_address: String,
  public_address: String,
  default_route: Option<String>,
  groups: Vec<RouteGroup>,
}

#[derive(Deserialize, Serialize)]
struct RouteGroup {
  name: String,
  description: Option<String>,
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

/// Generates a hashmap of routes from the data structure created by the config
/// file. This should improve runtime performance and is a better solution than
/// just iterating over the config object for every hop resolution.
fn cache_routes(groups: &[RouteGroup]) -> HashMap<String, String> {
  let mut mapping = HashMap::new();
  for group in groups {
    for (kw, dest) in &group.routes {
      match mapping.insert(kw.clone(), dest.clone()) {
        None => trace!("Inserting {} into mapping.", kw),
        Some(old_value) => {
          debug!("Overriding {} route from {} to {}.", kw, old_value, dest)
        }
      }
    }
  }
  mapping
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

/// Starts the watch on a file, if possible. This will only return an Error if
/// the notify library (used by Hotwatch) fails to initialize, which is
/// considered to be a more serve error as it may be indicative of a low-level
/// problem. If a watch was unsuccessfully obtained (the most common is due to
/// the file not existing), then this will simply warn before returning a watch
/// object.
///
/// This watch object should be kept in scope as dropping it releases all
/// watches.
fn start_watch(
  state: Arc<RwLock<State>>,
  config_file_path: String,
) -> Result<Hotwatch, BunBunError> {
  let mut watch = Hotwatch::new_with_custom_delay(Duration::from_millis(500))?;
  // TODO: keep retry watching in separate thread
  // Closures need their own copy of variables for proper lifecycle management
  let config_file_path_clone = config_file_path.clone();
  let watch_result = watch.watch(&config_file_path, move |e: Event| {
    if let Event::Write(_) = e {
      trace!("Grabbing writer lock on state...");
      let mut state = state.write().unwrap();
      trace!("Obtained writer lock on state!");
      match read_config(&config_file_path_clone) {
        Ok(conf) => {
          state.public_address = conf.public_address;
          state.default_route = conf.default_route;
          state.routes = cache_routes(&conf.groups);
          state.groups = conf.groups;
          info!("Successfully updated active state");
        }
        Err(e) => warn!("Failed to update config file: {}", e),
      }
    } else {
      debug!("Saw event {:#?} but ignored it", e);
    }
  });

  match watch_result {
    Ok(_) => info!("Watcher is now watching {}", &config_file_path),
    Err(e) => warn!(
      "Couldn't watch {}: {}. Changes to this file won't be seen!",
      &config_file_path, e
    ),
  }

  Ok(watch)
}
