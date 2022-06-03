use clap::{crate_authors, crate_version, Parser};
use std::path::PathBuf;
use tracing_subscriber::filter::Directive;

#[derive(Parser)]
#[clap(version = crate_version!(), author = crate_authors!())]
pub struct Opts {
    /// Set the logging directives
    #[clap(long, default_value = "info")]
    pub log: Vec<Directive>,
    /// Specify the location of the config file to read from. Needs read/write permissions.
    #[clap(short, long)]
    pub config: Option<PathBuf>,
    /// Allow config sizes larger than 100MB.
    #[clap(long)]
    pub large_config: bool,
}
