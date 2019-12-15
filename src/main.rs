use actix_web::{
    get,
    http::header,
    web::{Data, Query},
    App, HttpResponse, HttpServer, Responder,
};
use handlebars::Handlebars;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fs::{File, OpenOptions};
use std::io::{Error, Write};
use std::sync::Arc;

static DEFAULT_CONFIG: &'static [u8] = br#"
bind_address: "127.0.0.1:8080"
routes:
    g: "https://google.com/search?q={{query}}"
"#;

#[derive(Deserialize)]
struct SearchQuery {
    to: String,
}

#[get("/ls")]
fn list(data: Data<Arc<BTreeMap<String, String>>>) -> impl Responder {
    let mut resp = String::new();
    for (k, v) in data.iter() {
        resp.push_str(&format!("{}: {}\n", k, v));
    }
    HttpResponse::Ok().body(resp)
}

#[get("/hop")]
fn hop(data: Data<Arc<BTreeMap<String, String>>>, query: Query<SearchQuery>) -> impl Responder {
    let reg = Handlebars::new();
    let mut raw_args = query.to.split_ascii_whitespace();
    let command = raw_args.next();

    // Reform args into url-safe string (probably want to go thru an actual parser)
    let mut args = String::new();
    if let Some(first_arg) = raw_args.next() {
        args.push_str(first_arg);
        for arg in raw_args {
            args.push_str("+");
            args.push_str(arg);
        }
    }

    if command.is_none() {
        return HttpResponse::NotFound().body("not found");
    }

    // This struct is used until anonymous structs can be made
    #[derive(Serialize)]
    struct Filler {
        query: String,
    }

    match data.get(command.unwrap()) {
        Some(template) => HttpResponse::Found()
            .header(
                header::LOCATION,
                reg.render_template(template, &Filler { query: args })
                    .unwrap(),
            )
            .finish(),
        None => HttpResponse::NotFound().body("not found"),
    }
}

#[derive(Deserialize)]
struct Config {
    bind_address: String,
    routes: BTreeMap<String, String>,
}

fn main() -> Result<(), Error> {
    let config_file = match File::open("bunbun.toml") {
        Ok(file) => file,
        Err(_) => {
            eprintln!("Unable to find a bunbun.toml file. Creating default!");
            let mut fd = OpenOptions::new()
                .write(true)
                .create_new(true)
                .open("bunbun.toml")
                .expect("Unable to write to directory!");
            fd.write_all(DEFAULT_CONFIG)?;
            File::open("bunbun.toml")?
        }
    };
    let conf: Config = serde_yaml::from_reader(config_file).unwrap();
    let routes = Arc::from(conf.routes);

    HttpServer::new(move || App::new().data(routes.clone()).service(hop).service(list))
        .bind(&conf.bind_address)?
        .run()
}
