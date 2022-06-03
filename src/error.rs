use std::error::Error;
use std::fmt;

#[derive(Debug)]
#[allow(clippy::module_name_repetitions)]
pub enum BunBunError {
    Io(std::io::Error),
    Parse(serde_yaml::Error),
    Watch(hotwatch::Error),
    CustomProgram(String),
    NoValidConfigPath,
    InvalidConfigPath(std::path::PathBuf, std::io::Error),
    ConfigTooLarge(u64),
    ZeroByteConfig,
    JsonParse(serde_json::Error),
}

impl Error for BunBunError {}

impl fmt::Display for BunBunError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            Self::Io(e) => e.fmt(f),
            Self::Parse(e) => e.fmt(f),
            Self::Watch(e) => e.fmt(f),
            Self::CustomProgram(msg) => msg.fmt(f),
            Self::NoValidConfigPath => write!(f, "No valid config path was found!"),
            Self::InvalidConfigPath(path, reason) => {
                write!(f, "Failed to access {path:?}: {reason}")
            }
            Self::ConfigTooLarge(size) => write!(f, "The config file was too large ({size} bytes)! Pass in --large-config to bypass this check."),
            Self::ZeroByteConfig => write!(f, "The config provided reported a size of 0 bytes. Please check your config path!"),
            Self::JsonParse(e) => e.fmt(f),
        }
    }
}

/// Generates a from implementation from the specified type to the provided
/// bunbun error.
macro_rules! from_error {
    ($from:ty, $to:ident) => {
        impl From<$from> for BunBunError {
            fn from(e: $from) -> Self {
                Self::$to(e)
            }
        }
    };
}

from_error!(std::io::Error, Io);
from_error!(serde_yaml::Error, Parse);
from_error!(hotwatch::Error, Watch);
from_error!(serde_json::Error, JsonParse);
