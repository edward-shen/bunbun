use crate::template_args;
use crate::State;
use actix_web::get;
use actix_web::http::header;
use actix_web::web::{Data, Query};
use actix_web::{HttpResponse, Responder};
use itertools::Itertools;
use log::debug;
use percent_encoding::{utf8_percent_encode, AsciiSet, CONTROLS};
use serde::Deserialize;
use std::collections::HashMap;
use std::sync::{Arc, RwLock};

/// https://url.spec.whatwg.org/#fragment-percent-encode-set
const FRAGMENT_ENCODE_SET: &AsciiSet = &CONTROLS
  .add(b' ')
  .add(b'"')
  .add(b'<')
  .add(b'>')
  .add(b'`')
  .add(b'+');

#[get("/ls")]
pub async fn list(data: Data<Arc<RwLock<State>>>) -> impl Responder {
  let data = data.read().unwrap();
  HttpResponse::Ok().body(data.renderer.render("list", &data.groups).unwrap())
}

#[derive(Deserialize)]
pub struct SearchQuery {
  to: String,
}

#[get("/hop")]
pub async fn hop(
  data: Data<Arc<RwLock<State>>>,
  query: Query<SearchQuery>,
) -> impl Responder {
  let data = data.read().unwrap();

  match resolve_hop(&query.to, &data.routes, &data.default_route) {
    (Some(path), args) => HttpResponse::Found()
      .header(
        header::LOCATION,
        data
          .renderer
          .render_template(
            &path,
            &template_args::query(
              utf8_percent_encode(&args, FRAGMENT_ENCODE_SET).to_string(),
            ),
          )
          .unwrap(),
      )
      .finish(),
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
    None => {
      debug!("Found empty query, returning no route.");
      return (None, String::new());
    }
  };

  match (routes.get(*command), default_route) {
    // Found a route
    (Some(resolved), _) => (
      Some(resolved.clone()),
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
      (routes.get(route).cloned(), args)
    }
    // No default route and no match
    (None, None) => {
      debug!("Failed to resolve route!");
      (None, String::new())
    }
  }
}

#[get("/")]
pub async fn index(data: Data<Arc<RwLock<State>>>) -> impl Responder {
  let data = data.read().unwrap();
  HttpResponse::Ok().body(
    data
      .renderer
      .render(
        "index",
        &template_args::hostname(data.public_address.clone()),
      )
      .unwrap(),
  )
}

#[get("/bunbunsearch.xml")]
pub async fn opensearch(data: Data<Arc<RwLock<State>>>) -> impl Responder {
  let data = data.read().unwrap();
  HttpResponse::Ok()
    .header(
      header::CONTENT_TYPE,
      "application/opensearchdescription+xml",
    )
    .body(
      data
        .renderer
        .render(
          "opensearch",
          &template_args::hostname(data.public_address.clone()),
        )
        .unwrap(),
    )
}
