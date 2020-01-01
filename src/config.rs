use crate::BunBunError;
use log::{debug, error, info, trace};
use serde::{
  de::{Deserializer, Visitor},
  Deserialize, Serialize, Serializer,
};
use std::collections::HashMap;
use std::fmt;
use std::fs::{read_to_string, OpenOptions};
use std::io::Write;

const DEFAULT_CONFIG: &[u8] = include_bytes!("../bunbun.default.yaml");

#[derive(Deserialize, Debug, PartialEq)]
pub struct Config {
  pub bind_address: String,
  pub public_address: String,
  pub default_route: Option<String>,
  pub groups: Vec<RouteGroup>,
}

#[derive(Deserialize, Serialize, Debug, PartialEq, Clone)]
pub struct RouteGroup {
  pub name: String,
  pub description: Option<String>,
  pub routes: HashMap<String, Route>,
}

#[derive(Debug, PartialEq, Clone)]
pub enum Route {
  External(String),
  Path(String),
}

/// Serialization of the Route enum needs to be transparent, but since the
/// `#[serde(transparent)]` macro isn't available on enums, so we need to
/// implement it manually.
impl Serialize for Route {
  fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
  where
    S: Serializer,
  {
    match self {
      Self::External(s) => serializer.serialize_str(s),
      Self::Path(s) => serializer.serialize_str(s),
    }
  }
}

/// Deserialization of the route string into the enum requires us to figure out
/// whether or not the string is valid to run as an executable or not. To
/// determine this, we simply check if it exists on disk or assume that it's a
/// web path. This incurs a disk check operation, but since users shouldn't be
/// updating the config that frequently, it should be fine.
impl<'de> Deserialize<'de> for Route {
  fn deserialize<D>(deserializer: D) -> Result<Route, D::Error>
  where
    D: Deserializer<'de>,
  {
    struct RouteVisitor;
    impl<'de> Visitor<'de> for RouteVisitor {
      type Value = Route;

      fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
        formatter.write_str("string")
      }

      fn visit_str<E>(self, value: &str) -> Result<Self::Value, E>
      where
        E: serde::de::Error,
      {
        // Return early if it's a path, don't go through URL parsing
        if std::path::Path::new(value).exists() {
          debug!("Parsed {} as a valid local path.", value);
          Ok(Route::Path(value.into()))
        } else {
          debug!("{} does not exist on disk, assuming web path.", value);
          Ok(Route::External(value.into()))
        }
      }
    }

    deserializer.deserialize_str(RouteVisitor)
  }
}

impl std::fmt::Display for Route {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    match self {
      Self::External(s) => write!(f, "raw ({})", s),
      Self::Path(s) => write!(f, "file ({})", s),
    }
  }
}

/// Attempts to read the config file. If it doesn't exist, generate one a
/// default config file before attempting to parse it.
pub fn read_config(config_file_path: &str) -> Result<Config, BunBunError> {
  trace!("Loading config file...");
  let config_str = match read_to_string(config_file_path) {
    Ok(conf_str) => {
      debug!("Successfully loaded config file into memory.");
      conf_str
    }
    Err(_) => {
      info!(
        "Unable to find a {} file. Creating default!",
        config_file_path
      );

      let fd = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(config_file_path);

      match fd {
        Ok(mut fd) => fd.write_all(DEFAULT_CONFIG)?,
        Err(e) => {
          error!("Failed to write to {}: {}. Default config will be loaded but not saved.", config_file_path, e);
        }
      };

      String::from_utf8_lossy(DEFAULT_CONFIG).into_owned()
    }
  };

  // Reading from memory is faster than reading directly from a reader for some
  // reason; see https://github.com/serde-rs/json/issues/160
  Ok(serde_yaml::from_str(&config_str)?)
}

#[cfg(test)]
mod route {
  use super::*;
  use serde_yaml::{from_str, to_string};
  use tempfile::NamedTempFile;

  #[test]
  fn deserialize_relative_path() {
    let tmpfile = NamedTempFile::new_in(".").unwrap();
    let path = format!("{}", tmpfile.path().display());
    let path = path.get(path.rfind(".").unwrap()..).unwrap();
    let path = std::path::Path::new(path);
    assert!(path.is_relative());
    let path = path.to_str().unwrap();
    assert_eq!(from_str::<Route>(path).unwrap(), Route::Path(path.into()));
  }

  #[test]
  fn deserialize_absolute_path() {
    let tmpfile = NamedTempFile::new().unwrap();
    let path = format!("{}", tmpfile.path().display());
    assert!(tmpfile.path().is_absolute());
    assert_eq!(from_str::<Route>(&path).unwrap(), Route::Path(path));
  }

  #[test]
  fn deserialize_http_path() {
    assert_eq!(
      from_str::<Route>("http://google.com").unwrap(),
      Route::External("http://google.com".into())
    );
  }

  #[test]
  fn deserialize_https_path() {
    assert_eq!(
      from_str::<Route>("https://google.com").unwrap(),
      Route::External("https://google.com".into())
    );
  }

  #[test]
  fn serialize() {
    assert_eq!(
      &to_string(&Route::External("hello world".into())).unwrap(),
      "---\nhello world"
    );
    assert_eq!(
      &to_string(&Route::Path("hello world".into())).unwrap(),
      "---\nhello world"
    );
  }
}

#[cfg(test)]
mod read_config {
  use super::*;
  use tempfile::NamedTempFile;

  #[test]
  fn returns_default_config_if_path_does_not_exist() {
    assert_eq!(
      read_config("/this_is_a_non_existent_file").unwrap(),
      serde_yaml::from_slice(DEFAULT_CONFIG).unwrap()
    );
  }

  #[test]
  fn returns_error_if_given_empty_config() {
    assert_eq!(
      read_config("/dev/null").unwrap_err().to_string(),
      "EOF while parsing a value"
    );
  }

  #[test]
  fn returns_error_if_given_invalid_config() -> Result<(), std::io::Error> {
    let mut tmp_file = NamedTempFile::new()?;
    tmp_file.write_all(b"g")?;
    assert_eq!(
      read_config(tmp_file.path().to_str().unwrap())
        .unwrap_err()
        .to_string(),
      r#"invalid type: string "g", expected struct Config at line 1 column 1"#
    );
    Ok(())
  }

  #[test]
  fn returns_error_if_config_missing_field() -> Result<(), std::io::Error> {
    let mut tmp_file = NamedTempFile::new()?;
    tmp_file.write_all(
      br#"
      bind_address: "localhost"
      public_address: "localhost"
      "#,
    )?;
    assert_eq!(
      read_config(tmp_file.path().to_str().unwrap())
        .unwrap_err()
        .to_string(),
      "missing field `groups` at line 2 column 19"
    );
    Ok(())
  }

  #[test]
  fn returns_ok_if_valid_config() -> Result<(), std::io::Error> {
    let mut tmp_file = NamedTempFile::new()?;
    tmp_file.write_all(
      br#"
      bind_address: "a"
      public_address: "b"
      groups: []"#,
    )?;
    assert_eq!(
      read_config(tmp_file.path().to_str().unwrap()).unwrap(),
      Config {
        bind_address: String::from("a"),
        public_address: String::from("b"),
        groups: vec![],
        default_route: None,
      }
    );
    Ok(())
  }
}
