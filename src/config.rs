use crate::BunBunError;
use dirs::{config_dir, home_dir};
use log::{debug, info, trace};
use serde::{
  de::{self, Deserializer, MapAccess, Visitor},
  Deserialize, Serialize,
};
use std::collections::HashMap;
use std::fmt;
use std::fs::{File, OpenOptions};
use std::io::{Read, Write};
use std::path::PathBuf;
use std::str::FromStr;

const CONFIG_FILENAME: &str = "bunbun.yaml";
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
  #[serde(default)]
  pub hidden: bool,
  pub routes: HashMap<String, Route>,
}

#[derive(Debug, PartialEq, Clone, Serialize)]
pub struct Route {
  pub route_type: RouteType,
  pub path: String,
  pub hidden: bool,
  pub description: Option<String>,
}

impl FromStr for Route {
  type Err = std::convert::Infallible;
  fn from_str(s: &str) -> Result<Self, Self::Err> {
    Ok(Self {
      route_type: get_route_type(s),
      path: s.to_string(),
      hidden: false,
      description: None,
    })
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
    #[derive(Deserialize)]
    #[serde(field_identifier, rename_all = "lowercase")]
    enum Field {
      Path,
      Hidden,
      Description,
    }

    struct RouteVisitor;

    impl<'de> Visitor<'de> for RouteVisitor {
      type Value = Route;

      fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
        formatter.write_str("string")
      }

      fn visit_str<E>(self, path: &str) -> Result<Self::Value, E>
      where
        E: serde::de::Error,
      {
        // This is infalliable
        Ok(Self::Value::from_str(path).unwrap())
      }

      fn visit_map<M>(self, mut map: M) -> Result<Self::Value, M::Error>
      where
        M: MapAccess<'de>,
      {
        let mut path = None;
        let mut hidden = None;
        let mut description = None;

        while let Some(key) = map.next_key()? {
          match key {
            Field::Path => {
              if path.is_some() {
                return Err(de::Error::duplicate_field("path"));
              }
              path = Some(map.next_value::<String>()?);
            }
            Field::Hidden => {
              if hidden.is_some() {
                return Err(de::Error::duplicate_field("hidden"));
              }
              hidden = map.next_value()?;
            }
            Field::Description => {
              if description.is_some() {
                return Err(de::Error::duplicate_field("description"));
              }
              description = Some(map.next_value()?);
            }
          }
        }

        let path = path.ok_or_else(|| de::Error::missing_field("path"))?;
        Ok(Route {
          route_type: get_route_type(&path),
          path,
          hidden: hidden.unwrap_or_default(),
          description,
        })
      }
    }

    deserializer.deserialize_any(RouteVisitor)
  }
}

impl std::fmt::Display for Route {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    match self {
      Self {
        route_type: RouteType::External,
        path,
        ..
      } => write!(f, "raw ({})", path),
      Self {
        route_type: RouteType::Internal,
        path,
        ..
      } => write!(f, "file ({})", path),
    }
  }
}

/// Classifies the path depending on if the there exists a local file.
fn get_route_type(path: &str) -> RouteType {
  if std::path::Path::new(path).exists() {
    debug!("Parsed {} as a valid local path.", path);
    RouteType::Internal
  } else {
    debug!("{} does not exist on disk, assuming web path.", path);
    RouteType::External
  }
}

/// There exists two route types: an external path (e.g. a URL) or an internal
/// path (to a file).
#[derive(Debug, PartialEq, Clone, Serialize)]
pub enum RouteType {
  External,
  Internal,
}

pub struct ConfigData {
  pub path: PathBuf,
  pub file: File,
}

/// If a provided config path isn't found, this function checks known good
/// locations for a place to write a config file to. In order, it checks the
/// system-wide config location (`/etc/`, in Linux), followed by the config
/// folder, followed by the user's home folder.
pub fn get_config_data() -> Result<ConfigData, BunBunError> {
  // Locations to check, with highest priority first
  let locations: Vec<_> = {
    let mut folders = vec![PathBuf::from("/etc/")];

    // Config folder
    if let Some(folder) = config_dir() {
      folders.push(folder)
    }

    // Home folder
    if let Some(folder) = home_dir() {
      folders.push(folder)
    }

    folders
      .iter_mut()
      .for_each(|folder| folder.push(CONFIG_FILENAME));

    folders
  };

  debug!("Checking locations for config file: {:?}", &locations);

  for location in &locations {
    let file = OpenOptions::new().read(true).open(location.clone());
    match file {
      Ok(file) => {
        debug!("Found file at {:?}.", location);
        return Ok(ConfigData {
          path: location.clone(),
          file,
        });
      }
      Err(e) => debug!(
        "Tried to read '{:?}' but failed due to error: {}",
        location, e
      ),
    }
  }

  debug!("Failed to find any config. Now trying to find first writable path");

  // If we got here, we failed to read any file paths, meaning no config exists
  // yet. In that case, try to return the first location that we can write to,
  // after writing the default config
  for location in locations {
    let file = OpenOptions::new()
      .write(true)
      .create_new(true)
      .open(location.clone());
    match file {
      Ok(mut file) => {
        info!("Creating new config file at {:?}.", location);
        file.write_all(DEFAULT_CONFIG)?;

        let file = OpenOptions::new().read(true).open(location.clone())?;
        return Ok(ConfigData {
          path: location,
          file,
        });
      }
      Err(e) => debug!(
        "Tried to open a new file at '{:?}' but failed due to error: {}",
        location, e
      ),
    }
  }

  Err(BunBunError::NoValidConfigPath)
}

/// Assumes that the user knows what they're talking about and will only try
/// to load the config at the given path.
pub fn load_custom_path_config(
  path: impl Into<PathBuf>,
) -> Result<ConfigData, BunBunError> {
  let path = path.into();
  let file = OpenOptions::new()
    .read(true)
    .open(&path)
    .map_err(|e| BunBunError::InvalidConfigPath(path.clone(), e))?;

  Ok(ConfigData { file, path })
}

pub fn read_config(mut config_file: File) -> Result<Config, BunBunError> {
  trace!("Loading config file.");
  let mut config_data = String::new();
  config_file.read_to_string(&mut config_data)?;
  // Reading from memory is faster than reading directly from a reader for some
  // reason; see https://github.com/serde-rs/json/issues/160
  Ok(serde_yaml::from_str(&config_data)?)
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
