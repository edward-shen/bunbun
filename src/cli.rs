use clap::{crate_authors, crate_version, Clap};
use std::path::PathBuf;

#[derive(Clap)]
#[clap(version = crate_version!(), author = crate_authors!())]
pub struct Opts {
  /// Increases the log level to info, debug, and trace, respectively.
  #[clap(short, long, parse(from_occurrences), conflicts_with("quiet"))]
  pub verbose: u8,
  /// Decreases the log level to error or no logging at all, respectively.
  #[clap(short, long, parse(from_occurrences), conflicts_with("verbose"))]
  pub quiet: u8,
  /// Specify the location of the config file to read from. Needs read/write permissions.
  #[clap(short, long)]
  pub config: Option<PathBuf>,
  /// Allow config sizes larger than 100MB.
  #[clap(long)]
  pub large_config: bool,
}
