use crate::{routes::Route, BunBunError};
use log::{debug, error, info, trace};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
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
