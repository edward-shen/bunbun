use std::error::Error;
use std::fmt;

#[derive(Debug)]
#[allow(clippy::enum_variant_names)]
pub enum BunBunError {
  IoError(std::io::Error),
  ParseError(serde_yaml::Error),
  WatchError(hotwatch::Error),
  LoggerInitError(log::SetLoggerError),
}

impl Error for BunBunError {}

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
