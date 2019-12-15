use actix_web::{
    get,
    http::header,
    web::{Data, Query},
    App, HttpResponse, HttpServer, Responder,
};
use handlebars::Handlebars;
use serde::Deserialize;
use std::collections::BTreeMap;
use std::collections::HashMap;
use std::fs::{File, OpenOptions};
use std::io::{Error, Write};
use std::sync::{Arc, RwLock};

static DEFAULT_CONFIG: &[u8] = br#"
bind_address: "127.0.0.1:8080"
public_address: "localhost"

routes:
    g: "https://google.com/search?q={{query}}"
"#;

#[derive(Deserialize)]
struct SearchQuery {
    to: String,
}

#[get("/ls")]
fn list(data: Data<Arc<State>>) -> impl Responder {
    let mut resp = String::new();
    for (k, v) in data.routes.iter() {
        resp.push_str(&format!("{}: {}\n", k, v));
    }
    HttpResponse::Ok().body(resp)
}

#[get("/hop")]
fn hop(data: Data<Arc<State>>, query: Query<SearchQuery>) -> impl Responder {
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

    let mut template_args = HashMap::new();
    template_args.insert("query", args);

    match data.routes.get(command.unwrap()) {
        Some(template) => HttpResponse::Found()
            .header(
                header::LOCATION,
                reg.render_template(template, &template_args).unwrap(),
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
                        reg.render_template(data.routes.get(route).unwrap(), &template_args)
                            .unwrap(),
                    )
                    .finish()
            }
            None => HttpResponse::NotFound().body("not found"),
        },
    }
}

#[get("/")]
fn index(data: Data<Arc<State>>) -> impl Responder {
    HttpResponse::Ok().body(data.renderer.read().unwrap().render("index", &()).unwrap())
}

#[get("/bunbunsearch.xml")]
fn opensearch(data: Data<Arc<State>>) -> impl Responder {
    let mut template_args = HashMap::new();
    template_args.insert("hostname", &data.public_address);
    HttpResponse::Ok()
        .header(
            header::CONTENT_TYPE,
            "application/opensearchdescription+xml",
        )
        .body(
            data.renderer
                .read()
                .unwrap()
                .render("opensearch", &template_args)
                .unwrap(),
        )
}

#[derive(Deserialize)]
struct Config {
    bind_address: String,
    public_address: String,
    default_route: Option<String>,
    routes: BTreeMap<String, String>,
}

struct State {
    public_address: String,
    default_route: Option<String>,
    routes: BTreeMap<String, String>,
    renderer: RwLock<Handlebars>,
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

    let renderer = compile_templates();
    let conf: Config = serde_yaml::from_reader(config_file).unwrap();
    let state = Arc::from(State {
        public_address: conf.public_address,
        default_route: conf.default_route,
        routes: conf.routes,
        renderer: RwLock::new(renderer),
    });

    HttpServer::new(move || {
        App::new()
            .data(state.clone())
            .service(hop)
            .service(list)
            .service(index)
            .service(opensearch)
    })
    .bind(&conf.bind_address)?
    .run()
}

fn compile_templates() -> Handlebars {
    let mut handlebars = Handlebars::new();
    handlebars
        .register_template_string(
            "index",
            String::from_utf8_lossy(include_bytes!("templates/index.hbs")),
        )
        .unwrap();
    handlebars
        .register_template_string(
            "opensearch",
            String::from_utf8_lossy(include_bytes!("templates/bunbunsearch.xml")),
        )
        .unwrap();
    handlebars
}
