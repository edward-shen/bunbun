#![forbid(unsafe_code)]
#![deny(missing_docs)]

//! Bunbun is a pure-Rust implementation of bunny1 that provides a customizable
//! search engine and quick-jump tool in one small binary. For information on
//! usage, please take a look at the readme.

use crate::config::{
  get_config_data, load_custom_path_config, read_config, ConfigData, Route,
  RouteGroup,
};
use actix_web::{middleware::Logger, App, HttpServer};
use clap::Clap;
use error::BunBunError;
use handlebars::Handlebars;
use hotwatch::{Event, Hotwatch};
use log::{debug, error, info, trace, warn};
use std::cmp::min;
use std::collections::HashMap;
use std::sync::{Arc, RwLock};
use std::time::Duration;

mod cli;
mod config;
mod error;
mod routes;
mod template_args;

/// Dynamic variables that either need to be present at runtime, or can be
/// changed during runtime.
pub struct State {
  public_address: String,
  default_route: Option<String>,
  groups: Vec<RouteGroup>,
  /// Cached, flattened mapping of all routes and their destinations.
  routes: HashMap<String, Route>,
}

#[actix_rt::main]
async fn main() {
  std::process::exit(match run().await {
    Ok(_) => 0,
    Err(e) => {
      error!("{}", e);
      1
    }
  })
}

async fn run() -> Result<(), BunBunError> {
  let opts = cli::Opts::parse();

  init_logger(opts.verbose, opts.quiet)?;

  let conf_data = match opts.config {
    Some(file_name) => load_custom_path_config(file_name),
    None => get_config_data(),
  }?;

  let conf = read_config(conf_data.file.try_clone()?)?;
  let state = Arc::from(RwLock::new(State {
    public_address: conf.public_address,
    default_route: conf.default_route,
    routes: cache_routes(&conf.groups),
    groups: conf.groups,
  }));

  let _watch = start_watch(state.clone(), conf_data)?;

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
  num_verbose_flags: u8,
  num_quiet_flags: u8,
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

/// Generates a hashmap of routes from the data structure created by the config
/// file. This should improve runtime performance and is a better solution than
/// just iterating over the config object for every hop resolution.
fn cache_routes(groups: &[RouteGroup]) -> HashMap<String, Route> {
  let mut mapping = HashMap::new();
  for group in groups {
    for (kw, dest) in &group.routes {
      match mapping.insert(kw.clone(), dest.clone()) {
        None => trace!("Inserting {} into mapping.", kw),
        Some(old_value) => {
          trace!("Overriding {} route from {} to {}.", kw, old_value, dest)
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
  handlebars.set_strict_mode(true);
  handlebars
    .register_partial("bunbun_version", env!("CARGO_PKG_VERSION"))
    .unwrap();
  handlebars
    .register_partial("bunbun_src", env!("CARGO_PKG_REPOSITORY"))
    .unwrap();
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
  config_data: ConfigData,
) -> Result<Hotwatch, BunBunError> {
  let mut watch = Hotwatch::new_with_custom_delay(Duration::from_millis(500))?;

  // Closures need their own copy of variables for proper life cycle management
  let config_data = Arc::new(config_data);
  let config_data_ref = Arc::clone(&config_data);

  let watch_result = watch.watch(&config_data.path, move |e: Event| {
    if let Event::Write(_) = e {
      trace!("Grabbing writer lock on state...");
      let mut state = state.write().expect("Failed to get write lock on state");
      trace!("Obtained writer lock on state!");
      match read_config(
        config_data_ref
          .file
          .try_clone()
          .expect("Failed to clone file handle"),
      ) {
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
    Ok(_) => info!("Watcher is now watching {:?}", &config_data.path),
    Err(e) => warn!(
      "Couldn't watch {:?}: {}. Changes to this file won't be seen!",
      &config_data.path, e
    ),
  }

  Ok(watch)
}

#[cfg(test)]
mod init_logger {
  use super::*;

  #[test]
  fn defaults_to_warn() -> Result<(), BunBunError> {
    init_logger(0, 0)?;
    assert_eq!(log::max_level(), log::Level::Warn);
    Ok(())
  }

  // The following tests work but because the log crate is global, initializing
  // the logger more than once (read: testing it more than once) leads to a
  // panic. These ignored tests must be manually tested.

  #[test]
  #[ignore]
  fn caps_to_2_when_log_level_is_lt_2() -> Result<(), BunBunError> {
    init_logger(0, 3)?;
    assert_eq!(log::max_level(), log::LevelFilter::Off);
    Ok(())
  }

  #[test]
  #[ignore]
  fn caps_to_3_when_log_level_is_gt_3() -> Result<(), BunBunError> {
    init_logger(4, 0)?;
    assert_eq!(log::max_level(), log::Level::Trace);
    Ok(())
  }
}

#[cfg(test)]
mod cache_routes {
  use super::*;
  use std::iter::FromIterator;
  use std::str::FromStr;

  fn generate_external_routes(
    routes: &[(&str, &str)],
  ) -> HashMap<String, Route> {
    HashMap::from_iter(
      routes
        .into_iter()
        .map(|kv| (kv.0.into(), Route::from_str(kv.1).unwrap())),
    )
  }

  #[test]
  fn empty_groups_yield_empty_routes() {
    assert_eq!(cache_routes(&[]), HashMap::new());
  }

  #[test]
  fn disjoint_groups_yield_summed_routes() {
    let group1 = RouteGroup {
      name: String::from("x"),
      description: Some(String::from("y")),
      routes: generate_external_routes(&[("a", "b"), ("c", "d")]),
      hidden: false,
    };

    let group2 = RouteGroup {
      name: String::from("5"),
      description: Some(String::from("6")),
      routes: generate_external_routes(&[("1", "2"), ("3", "4")]),
      hidden: false,
    };

    assert_eq!(
      cache_routes(&[group1, group2]),
      generate_external_routes(&[
        ("a", "b"),
        ("c", "d"),
        ("1", "2"),
        ("3", "4")
      ])
    );
  }

  #[test]
  fn overlapping_groups_use_latter_routes() {
    let group1 = RouteGroup {
      name: String::from("x"),
      description: Some(String::from("y")),
      routes: generate_external_routes(&[("a", "b"), ("c", "d")]),
      hidden: false,
    };

    let group2 = RouteGroup {
      name: String::from("5"),
      description: Some(String::from("6")),
      routes: generate_external_routes(&[("a", "1"), ("c", "2")]),
      hidden: false,
    };

    assert_eq!(
      cache_routes(&[group1.clone(), group2]),
      generate_external_routes(&[("a", "1"), ("c", "2")])
    );

    let group3 = RouteGroup {
      name: String::from("5"),
      description: Some(String::from("6")),
      routes: generate_external_routes(&[("a", "1"), ("b", "2")]),
      hidden: false,
    };

    assert_eq!(
      cache_routes(&[group1, group3]),
      generate_external_routes(&[("a", "1"), ("b", "2"), ("c", "d")])
    );
  }
}

#[cfg(test)]
mod compile_templates {
  use super::compile_templates;

  /// Successful compilation of the binary guarantees that the templates will be
  /// present to be registered to. Thus, we only really need to see that
  /// compilation of the templates don't panic, which is just making sure that
  /// the function can be successfully called.
  #[test]
  fn templates_compile() {
    let _ = compile_templates();
  }
}
