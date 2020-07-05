use crate::{template_args, BunBunError, Route, State};
use actix_web::web::{Data, Query};
use actix_web::{get, http::header};
use actix_web::{HttpRequest, HttpResponse, Responder};
use handlebars::Handlebars;
use itertools::Itertools;
use log::{debug, error};
use percent_encoding::{utf8_percent_encode, AsciiSet, CONTROLS};
use serde::Deserialize;
use std::collections::HashMap;
use std::path::PathBuf;
use std::process::Command;
use std::sync::{Arc, RwLock};

/// https://url.spec.whatwg.org/#fragment-percent-encode-set
const FRAGMENT_ENCODE_SET: &AsciiSet = &CONTROLS
  .add(b' ')
  .add(b'"')
  .add(b'<')
  .add(b'>')
  .add(b'`')
  .add(b'+');

type StateData = Data<Arc<RwLock<State>>>;

#[get("/")]
pub async fn index(data: StateData, req: HttpRequest) -> impl Responder {
  let data = data.read().unwrap();
  HttpResponse::Ok()
    .set_header(header::CONTENT_TYPE, "text/html; charset=utf-8")
    .body(
      req
        .app_data::<Handlebars>()
        .unwrap()
        .render(
          "index",
          &template_args::hostname(data.public_address.clone()),
        )
        .unwrap(),
    )
}

#[get("/bunbunsearch.xml")]
pub async fn opensearch(data: StateData, req: HttpRequest) -> impl Responder {
  let data = data.read().unwrap();
  HttpResponse::Ok()
    .header(
      header::CONTENT_TYPE,
      "application/opensearchdescription+xml",
    )
    .body(
      req
        .app_data::<Handlebars>()
        .unwrap()
        .render(
          "opensearch",
          &template_args::hostname(data.public_address.clone()),
        )
        .unwrap(),
    )
}

#[get("/ls")]
pub async fn list(data: StateData, req: HttpRequest) -> impl Responder {
  let data = data.read().unwrap();
  HttpResponse::Ok()
    .set_header(header::CONTENT_TYPE, "text/html; charset=utf-8")
    .body(
      req
        .app_data::<Handlebars>()
        .unwrap()
        .render("list", &data.groups)
        .unwrap(),
    )
}

#[derive(Deserialize)]
pub struct SearchQuery {
  to: String,
}

#[get("/hop")]
pub async fn hop(
  data: StateData,
  req: HttpRequest,
  query: Query<SearchQuery>,
) -> impl Responder {
  let data = data.read().unwrap();

  match resolve_hop(&query.to, &data.routes, &data.default_route) {
    (Some(path), args) => {
      let resolved_template = match path {
        Route::Path(path) => resolve_path(PathBuf::from(path), &args),
        Route::External(path) => Ok(path.to_owned().into_bytes()),
      };

      match resolved_template {
        Ok(path) => HttpResponse::Found()
          .header(
            header::LOCATION,
            req
              .app_data::<Handlebars>()
              .unwrap()
              .render_template(
                std::str::from_utf8(&path).unwrap(),
                &template_args::query(
                  utf8_percent_encode(&args, FRAGMENT_ENCODE_SET).to_string(),
                ),
              )
              .unwrap(),
          )
          .finish(),
        Err(e) => {
          error!("Failed to redirect user for {}: {}", path, e);
          HttpResponse::InternalServerError().body("Something went wrong :(\n")
        }
      }
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
fn resolve_hop<'a>(
  query: &str,
  routes: &'a HashMap<String, Route>,
  default_route: &Option<String>,
) -> (Option<&'a Route>, String) {
  let mut split_args = query.split_ascii_whitespace().peekable();
  let command = match split_args.peek() {
    Some(command) => command,
    None => {
      debug!("Found empty query, returning no route.");
      return (None, String::new());
    }
  };

  match (routes.get(*command), default_route) {
    // Found a route
    (Some(resolved), _) => (
      Some(resolved),
      match split_args.next() {
        // Discard the first result, we found the route using the first arg
        Some(_) => {
          let args = split_args.join(" ");
          debug!("Resolved {} with args {}", resolved, args);
          args
        }
        None => {
          debug!("Resolved {} with no args", resolved);
          String::new()
        }
      },
    ),
    // Unable to find route, but had a default route
    (None, Some(route)) => {
      let args = split_args.join(" ");
      debug!("Using default route {} with args {}", route, args);
      match routes.get(route) {
        Some(v) => (Some(v), args),
        None => (None, String::new()),
      }
    }
    // No default route and no match
    (None, None) => {
      debug!("Failed to resolve route!");
      (None, String::new())
    }
  }
}

/// Runs the executable with the user's input as a single argument. Returns Ok
/// so long as the executable was successfully executed. Returns an Error if the
/// file doesn't exist or bunbun did not have permission to read and execute the
/// file.
fn resolve_path(path: PathBuf, args: &str) -> Result<Vec<u8>, BunBunError> {
  let output = Command::new(path.canonicalize()?).arg(args).output()?;

  if output.status.success() {
    Ok(output.stdout)
  } else {
    error!(
      "Program exit code for {} was not 0! Dumping standard error!",
      path.display(),
    );
    let error = String::from_utf8_lossy(&output.stderr);
    Err(BunBunError::CustomProgram(error.to_string()))
  }
}

#[cfg(test)]
mod resolve_hop {
  use super::*;

  fn generate_route_result<'a>(
    keyword: &'a Route,
    args: &str,
  ) -> (Option<&'a Route>, String) {
    (Some(keyword), String::from(args))
  }

  #[test]
  fn empty_routes_no_default_yields_failed_hop() {
    assert_eq!(
      resolve_hop("hello world", &HashMap::new(), &None),
      (None, String::new())
    );
  }

  #[test]
  fn empty_routes_some_default_yields_failed_hop() {
    assert_eq!(
      resolve_hop(
        "hello world",
        &HashMap::new(),
        &Some(String::from("google"))
      ),
      (None, String::new())
    );
  }

  #[test]
  fn only_default_routes_some_default_yields_default_hop() {
    let mut map: HashMap<String, Route> = HashMap::new();
    map.insert(
      "google".into(),
      Route::External("https://example.com".into()),
    );
    assert_eq!(
      resolve_hop("hello world", &map, &Some(String::from("google"))),
      generate_route_result(
        &Route::External("https://example.com".into()),
        "hello world"
      ),
    );
  }

  #[test]
  fn non_default_routes_some_default_yields_non_default_hop() {
    let mut map: HashMap<String, Route> = HashMap::new();
    map.insert(
      "google".into(),
      Route::External("https://example.com".into()),
    );
    assert_eq!(
      resolve_hop("google hello world", &map, &Some(String::from("a"))),
      generate_route_result(
        &Route::External("https://example.com".into()),
        "hello world"
      ),
    );
  }

  #[test]
  fn non_default_routes_no_default_yields_non_default_hop() {
    let mut map: HashMap<String, Route> = HashMap::new();
    map.insert(
      "google".into(),
      Route::External("https://example.com".into()),
    );
    assert_eq!(
      resolve_hop("google hello world", &map, &None),
      generate_route_result(
        &Route::External("https://example.com".into()),
        "hello world"
      ),
    );
  }
}

#[cfg(test)]
mod resolve_path {
  use super::resolve_path;
  use std::env::current_dir;
  use std::path::PathBuf;

  #[test]
  fn invalid_path_returns_err() {
    assert!(resolve_path(PathBuf::from("/bin/aaaa"), "aaaa").is_err());
  }

  #[test]
  fn valid_path_returns_ok() {
    assert!(resolve_path(PathBuf::from("/bin/echo"), "hello").is_ok());
  }

  #[test]
  fn relative_path_returns_ok() {
    // How many ".." needed to get to /
    let nest_level = current_dir().unwrap().ancestors().count() - 1;
    let mut rel_path = PathBuf::from("../".repeat(nest_level));
    rel_path.push("./bin/echo");
    assert!(resolve_path(rel_path, "hello").is_ok());
  }

  #[test]
  fn no_permissions_returns_err() {
    assert!(
      // Trying to run a command without permission
      format!(
        "{}",
        resolve_path(PathBuf::from("/root/some_exec"), "").unwrap_err()
      )
      .contains("Permission denied")
    );
  }

  #[test]
  fn non_success_exit_code_yields_err() {
    // cat-ing a folder always returns exit code 1
    assert!(resolve_path(PathBuf::from("/bin/cat"), "/").is_err());
  }
}
