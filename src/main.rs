#![forbid(unsafe_code)]
#![deny(missing_docs)]
#![warn(clippy::nursery, clippy::pedantic)]

//! Bunbun is a pure-Rust implementation of bunny1 that provides a customizable
//! search engine and quick-jump tool in one small binary. For information on
//! usage, please take a look at the readme.

use crate::config::{
  get_config_data, load_custom_file, load_file, FileData, Route, RouteGroup,
};
use anyhow::Result;
use arc_swap::ArcSwap;
use axum::routing::get;
use axum::{Extension, Router};
use clap::Parser;
use error::BunBunError;
use handlebars::Handlebars;
use hotwatch::{Event, Hotwatch};
use log::{debug, info, trace, warn};
use simple_logger::SimpleLogger;
use std::cmp::min;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

mod cli;
mod config;
#[cfg(not(tarpaulin_include))]
mod error;
mod routes;
#[cfg(not(tarpaulin_include))]
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

#[tokio::main]
#[cfg(not(tarpaulin_include))]
async fn main() -> Result<()> {
  let opts = cli::Opts::parse();

  init_logger(opts.verbose, opts.quiet)?;

  let conf_data = match opts.config {
    Some(file_name) => load_custom_file(file_name),
    None => get_config_data(),
  }?;

  let conf = load_file(conf_data.file.try_clone()?, opts.large_config)?;
  let state = Arc::from(ArcSwap::from_pointee(State {
    public_address: conf.public_address,
    default_route: conf.default_route,
    routes: cache_routes(conf.groups.clone()),
    groups: conf.groups,
  }));

  // Cannot be named _ or Rust will immediately drop it.
  let _watch = start_watch(Arc::clone(&state), conf_data, opts.large_config);

  let app = Router::new()
    .route("/", get(routes::index))
    .route("/bunbunsearch.xml", get(routes::opensearch))
    .route("/ls", get(routes::list))
    .route("/hop", get(routes::hop))
    .layer(Extension(compile_templates()?))
    .layer(Extension(state));

  axum::Server::bind(&conf.bind_address.parse()?)
    .serve(app.into_make_service())
    .await?;

  Ok(())
}

/// Initializes the logger based on the number of quiet and verbose flags passed
/// in. Usually, these values are mutually exclusive, that is, if the number of
/// verbose flags is non-zero then the quiet flag is zero, and vice versa.
#[cfg(not(tarpaulin_include))]
fn init_logger(num_verbose_flags: u8, num_quiet_flags: u8) -> Result<()> {
  let log_level =
    match min(num_verbose_flags, 3) as i8 - min(num_quiet_flags, 2) as i8 {
      -2 => None,
      -1 => Some(log::LevelFilter::Error),
      0 => Some(log::LevelFilter::Warn),
      1 => Some(log::LevelFilter::Info),
      2 => Some(log::LevelFilter::Debug),
      3 => Some(log::LevelFilter::Trace),
      _ => unreachable!(), // values are clamped to [0, 3] - [0, 2]
    };

  if let Some(level) = log_level {
    SimpleLogger::new().with_level(level).init()?;
  }

  Ok(())
}

/// Generates a hashmap of routes from the data structure created by the config
/// file. This should improve runtime performance and is a better solution than
/// just iterating over the config object for every hop resolution.
fn cache_routes(groups: Vec<RouteGroup>) -> HashMap<String, Route> {
  let mut mapping = HashMap::new();
  for group in groups {
    for (kw, dest) in group.routes {
      // This function isn't called often enough to not be a performance issue.
      match mapping.insert(kw.clone(), dest.clone()) {
        None => trace!("Inserting {kw} into mapping."),
        Some(old_value) => {
          trace!("Overriding {kw} route from {old_value} to {dest}.");
        }
      }
    }
  }
  mapping
}

/// Returns an instance with all pre-generated templates included into the
/// binary. This allows for users to have a portable binary without needed the
/// templates at runtime.
fn compile_templates() -> Result<Handlebars<'static>> {
  let mut handlebars = Handlebars::new();
  handlebars.set_strict_mode(true);
  handlebars.register_partial("bunbun_version", env!("CARGO_PKG_VERSION"))?;
  handlebars.register_partial("bunbun_src", env!("CARGO_PKG_REPOSITORY"))?;
  macro_rules! register_template {
    [ $( $template:expr ),* ] => {
      $(
        handlebars
          .register_template_string(
            $template,
            String::from_utf8_lossy(
              include_bytes!(concat!("templates/", $template, ".hbs")))
          )?;
        debug!("Loaded {} template.", $template);
      )*
    };
  }
  register_template!["index", "list", "opensearch"];
  Ok(handlebars)
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
#[cfg(not(tarpaulin_include))]
fn start_watch(
  state: Arc<ArcSwap<State>>,
  config_data: FileData,
  large_config: bool,
) -> Result<Hotwatch> {
  let mut watch = Hotwatch::new_with_custom_delay(Duration::from_millis(500))?;
  let FileData { path, mut file } = config_data;
  let watch_result = watch.watch(&path, move |e: Event| {
    if let Event::Create(ref path) = e {
      file = load_custom_file(path).expect("file to exist at path").file;
      trace!("Getting new file handler as file was recreated.");
    }

    match e {
      Event::Write(_) | Event::Create(_) => {
        trace!("Grabbing writer lock on state...");
        trace!("Obtained writer lock on state!");
        match load_file(
          file.try_clone().expect("Failed to clone file handle"),
          large_config,
        ) {
          Ok(conf) => {
            state.store(Arc::new(State {
              public_address: conf.public_address,
              default_route: conf.default_route,
              routes: cache_routes(conf.groups.clone()),
              groups: conf.groups,
            }));
            info!("Successfully updated active state");
          }
          Err(e) => warn!("Failed to update config file: {e}"),
        }
      }
      _ => debug!("Saw event {e:#?} but ignored it"),
    }
  });

  match watch_result {
    Ok(_) => info!("Watcher is now watching {path:?}"),
    Err(e) => {
      warn!(
        "Couldn't watch {path:?}: {e}. Changes to this file won't be seen!"
      );
    }
  }

  Ok(watch)
}

#[cfg(test)]
mod init_logger {
  use super::*;
  use anyhow::Result;

  #[test]
  fn defaults_to_warn() -> Result<()> {
    init_logger(0, 0)?;
    assert_eq!(log::max_level(), log::Level::Warn);
    Ok(())
  }

  // The following tests work but because the log crate is global, initializing
  // the logger more than once (read: testing it more than once) leads to a
  // panic. These ignored tests must be manually tested.

  #[test]
  #[ignore]
  fn caps_to_2_when_log_level_is_lt_2() -> Result<()> {
    init_logger(0, 3)?;
    assert_eq!(log::max_level(), log::LevelFilter::Off);
    Ok(())
  }

  #[test]
  #[ignore]
  fn caps_to_3_when_log_level_is_gt_3() -> Result<()> {
    init_logger(4, 0)?;
    assert_eq!(log::max_level(), log::Level::Trace);
    Ok(())
  }
}

#[cfg(test)]
mod cache_routes {
  use super::*;
  use std::iter::FromIterator;

  fn generate_external_routes(
    routes: &[(&'static str, &'static str)],
  ) -> HashMap<String, Route> {
    HashMap::from_iter(
      routes
        .into_iter()
        .map(|(key, value)| ((*key).to_owned(), Route::from(*value))),
    )
  }

  #[test]
  fn empty_groups_yield_empty_routes() {
    assert_eq!(cache_routes(Vec::new()), HashMap::new());
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
      cache_routes(vec![group1, group2]),
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
      cache_routes(vec![group1.clone(), group2]),
      generate_external_routes(&[("a", "1"), ("c", "2")])
    );

    let group3 = RouteGroup {
      name: String::from("5"),
      description: Some(String::from("6")),
      routes: generate_external_routes(&[("a", "1"), ("b", "2")]),
      hidden: false,
    };

    assert_eq!(
      cache_routes(vec![group1, group3]),
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
