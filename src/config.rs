use crate::BunBunError;
use dirs::{config_dir, home_dir};
use serde::{
    de::{self, Deserializer, MapAccess, Unexpected, Visitor},
    Deserialize, Serialize,
};
use std::collections::HashMap;
use std::fmt;
use std::fs::{File, OpenOptions};
use std::io::{Read, Write};
use std::path::PathBuf;
use tracing::{debug, info, trace};

const CONFIG_FILENAME: &str = "bunbun.yaml";
const DEFAULT_CONFIG: &[u8] = include_bytes!("../bunbun.default.yaml");
#[cfg(not(test))]
const LARGE_FILE_SIZE_THRESHOLD: u64 = 100_000_000;
#[cfg(test)]
const LARGE_FILE_SIZE_THRESHOLD: u64 = 1_000_000;

#[derive(Deserialize, Debug, PartialEq, Eq)]
pub struct Config {
    pub bind_address: String,
    pub public_address: String,
    pub default_route: Option<String>,
    pub groups: Vec<RouteGroup>,
}

#[derive(Deserialize, Serialize, Debug, Eq, PartialEq, Clone)]
pub struct RouteGroup {
    pub name: String,
    pub description: Option<String>,
    #[serde(default)]
    pub hidden: bool,
    pub routes: HashMap<String, Route>,
}

#[derive(Debug, PartialEq, Eq, Clone, Serialize)]
pub struct Route {
    pub route_type: RouteType,
    pub path: String,
    pub hidden: bool,
    pub description: Option<String>,
    pub min_args: Option<usize>,
    pub max_args: Option<usize>,
}

impl From<String> for Route {
    fn from(s: String) -> Self {
        Self {
            route_type: get_route_type(&s),
            path: s,
            hidden: false,
            description: None,
            min_args: None,
            max_args: None,
        }
    }
}

impl From<&'static str> for Route {
    fn from(s: &'static str) -> Self {
        Self {
            route_type: get_route_type(s),
            path: s.to_string(),
            hidden: false,
            description: None,
            min_args: None,
            max_args: None,
        }
    }
}

/// Deserialization of the route string into the enum requires us to figure out
/// whether or not the string is valid to run as an executable or not. To
/// determine this, we simply check if it exists on disk or assume that it's a
/// web path. This incurs a disk check operation, but since users shouldn't be
/// updating the config that frequently, it should be fine.
impl<'de> Deserialize<'de> for Route {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        #[derive(Deserialize)]
        #[serde(field_identifier, rename_all = "snake_case")]
        enum Field {
            Path,
            Hidden,
            Description,
            MinArgs,
            MaxArgs,
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
                Ok(Self::Value::from(path.to_owned()))
            }

            fn visit_map<M>(self, mut map: M) -> Result<Self::Value, M::Error>
            where
                M: MapAccess<'de>,
            {
                let mut path = None;
                let mut hidden = None;
                let mut description = None;
                let mut min_args = None;
                let mut max_args = None;

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
                        Field::MinArgs => {
                            if min_args.is_some() {
                                return Err(de::Error::duplicate_field("min_args"));
                            }
                            min_args = Some(map.next_value()?);
                        }
                        Field::MaxArgs => {
                            if max_args.is_some() {
                                return Err(de::Error::duplicate_field("max_args"));
                            }
                            max_args = Some(map.next_value()?);
                        }
                    }
                }

                if let (Some(min_args), Some(max_args)) = (min_args, max_args) {
                    if min_args > max_args {
                        {
                            return Err(de::Error::invalid_value(
                                Unexpected::Other(&format!(
                                    "argument count range {min_args} to {max_args}",
                                )),
                                &"a valid argument count range",
                            ));
                        }
                    }
                }

                let path = path.ok_or_else(|| de::Error::missing_field("path"))?;
                Ok(Route {
                    route_type: get_route_type(&path),
                    path,
                    hidden: hidden.unwrap_or_default(),
                    description,
                    min_args,
                    max_args,
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
            } => write!(f, "raw ({path})"),
            Self {
                route_type: RouteType::Internal,
                path,
                ..
            } => write!(f, "file ({path})"),
        }
    }
}

/// Classifies the path depending on if the there exists a local file.
fn get_route_type(path: &str) -> RouteType {
    if std::path::Path::new(path).exists() {
        debug!("Parsed {path} as a valid local path.");
        RouteType::Internal
    } else {
        debug!("{path} does not exist on disk, assuming web path.");
        RouteType::External
    }
}

/// There exists two route types: an external path (e.g. a URL) or an internal
/// path (to a file).
#[derive(Debug, PartialEq, Eq, Clone, Serialize)]
pub enum RouteType {
    External,
    Internal,
}

pub struct FileData {
    pub path: PathBuf,
    pub file: File,
}

/// If a provided config path isn't found, this function checks known good
/// locations for a place to write a config file to. In order, it checks the
/// system-wide config location (`/etc/`, in Linux), followed by the config
/// folder, followed by the user's home folder.
pub fn get_config_data() -> Result<FileData, BunBunError> {
    // Locations to check, with highest priority first
    let locations: Vec<_> = {
        let mut folders = vec![PathBuf::from("/etc/")];

        // Config folder
        if let Some(folder) = config_dir() {
            folders.push(folder);
        }

        // Home folder
        if let Some(folder) = home_dir() {
            folders.push(folder);
        }

        folders
            .iter_mut()
            .for_each(|folder| folder.push(CONFIG_FILENAME));

        folders
    };

    debug!("Checking locations for config file: {:?}", &locations);

    for location in &locations {
        let file = OpenOptions::new().read(true).open(location);
        match file {
            Ok(file) => {
                debug!("Found file at {location:?}.");
                return Ok(FileData {
                    path: location.clone(),
                    file,
                });
            }
            Err(e) => {
                debug!("Tried to read '{location:?}' but failed due to error: {e}");
            }
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
                info!("Creating new config file at {location:?}.");
                file.write_all(DEFAULT_CONFIG)?;

                let file = OpenOptions::new().read(true).open(location.clone())?;
                return Ok(FileData {
                    path: location,
                    file,
                });
            }
            Err(e) => {
                debug!("Tried to open a new file at '{location:?}' but failed due to error: {e}");
            }
        }
    }

    Err(BunBunError::NoValidConfigPath)
}

/// Assumes that the user knows what they're talking about and will only try
/// to load the config at the given path.
pub fn load_custom_file(path: impl Into<PathBuf>) -> Result<FileData, BunBunError> {
    let path = path.into();
    let file = OpenOptions::new()
        .read(true)
        .open(&path)
        .map_err(|e| BunBunError::InvalidConfigPath(path.clone(), e))?;

    Ok(FileData { path, file })
}

pub fn load_file(mut config_file: File, large_config: bool) -> Result<Config, BunBunError> {
    trace!("Loading config file.");
    let file_size = config_file.metadata()?.len();

    // 100 MB
    if file_size > LARGE_FILE_SIZE_THRESHOLD && !large_config {
        return Err(BunBunError::ConfigTooLarge(file_size));
    }

    if file_size == 0 {
        return Err(BunBunError::ZeroByteConfig);
    }

    let mut config_data = String::new();
    config_file.read_to_string(&mut config_data)?;
    // Reading from memory is faster than reading directly from a reader for some
    // reason; see https://github.com/serde-rs/json/issues/160
    Ok(serde_yaml::from_str(&config_data)?)
}

#[cfg(test)]
mod route {
    use super::*;
    use anyhow::{Context, Result};
    use serde_yaml::{from_str, to_string};
    use std::path::Path;
    use tempfile::NamedTempFile;

    #[test]
    fn deserialize_relative_path() -> Result<()> {
        let tmpfile = NamedTempFile::new_in(".")?;
        let path = tmpfile.path().display().to_string();
        let path = path
            .get(path.rfind(".").context("While finding .")?..)
            .context("While getting the path")?;
        let path = Path::new(path);
        assert!(path.is_relative());
        let path = path.to_str().context("While stringifying path")?;
        assert_eq!(from_str::<Route>(path)?, Route::from(path.to_owned()));
        Ok(())
    }

    #[test]
    fn deserialize_absolute_path() -> Result<()> {
        let tmpfile = NamedTempFile::new()?;
        let path = format!("{}", tmpfile.path().display());
        assert!(tmpfile.path().is_absolute());
        assert_eq!(from_str::<Route>(&path)?, Route::from(path));

        Ok(())
    }

    #[test]
    fn deserialize_http_path() -> Result<()> {
        assert_eq!(
            from_str::<Route>("http://google.com")?,
            Route::from("http://google.com")
        );
        Ok(())
    }

    #[test]
    fn deserialize_https_path() -> Result<()> {
        assert_eq!(
            from_str::<Route>("https://google.com")?,
            Route::from("https://google.com")
        );
        Ok(())
    }

    #[test]
    fn serialize() -> Result<()> {
        assert_eq!(
            &to_string(&Route::from("hello world"))?,
            "---\nroute_type: External\npath: hello world\nhidden: false\ndescription: ~\nmin_args: ~\nmax_args: ~\n"
        );
        Ok(())
    }
}

#[cfg(test)]
mod read_config {
    use super::*;
    use anyhow::Result;

    #[test]
    fn empty_file() -> Result<()> {
        let config_file = tempfile::tempfile()?;
        assert!(matches!(
            load_file(config_file, false),
            Err(BunBunError::ZeroByteConfig)
        ));
        Ok(())
    }

    #[test]
    fn config_too_large() -> Result<()> {
        let mut config_file = tempfile::tempfile()?;
        let size_to_write = (LARGE_FILE_SIZE_THRESHOLD + 1) as usize;
        config_file.write(&[0].repeat(size_to_write))?;
        match load_file(config_file, false) {
            Err(BunBunError::ConfigTooLarge(size)) if size as usize == size_to_write => {}
            Err(BunBunError::ConfigTooLarge(size)) => {
                panic!("Mismatched size: {size} != {size_to_write}")
            }
            res => panic!("Wrong result, got {res:#?}"),
        }
        Ok(())
    }

    #[test]
    fn valid_config() -> Result<()> {
        assert!(load_file(File::open("bunbun.default.yaml")?, false).is_ok());
        Ok(())
    }
}
