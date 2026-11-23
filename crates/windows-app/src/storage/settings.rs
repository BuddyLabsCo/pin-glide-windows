use std::{fs::File, io, io::BufReader, path::PathBuf};

use serde::{Deserialize, Serialize};

use super::{WRITER, replace_json, windows};

#[cfg(test)]
mod tests;

const VERSION: u32 = 1;

#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum Language {
    #[default]
    System,
    Chinese,
    English,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Settings {
    version: u32,
    #[serde(default)]
    pub(crate) language: Language,
    #[serde(default = "default_launch_at_login")]
    pub(crate) launch_at_login: bool,
}

fn default_launch_at_login() -> bool {
    true
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            version: VERSION,
            language: Language::System,
            launch_at_login: default_launch_at_login(),
        }
    }
}

pub(crate) struct SettingsStore {
    path: PathBuf,
}

impl SettingsStore {
    pub(crate) fn for_current_user() -> io::Result<Self> {
        Ok(Self::new(
            windows::local_app_data()?
                .join("Pin Glide")
                .join("settings.json"),
        ))
    }

    pub(crate) fn new(path: PathBuf) -> Self {
        Self { path }
    }

    pub(crate) fn read(&self) -> io::Result<Option<Settings>> {
        let file = match File::open(&self.path) {
            Ok(file) => file,
            Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
            Err(error) => return Err(error),
        };
        let settings: Settings = serde_json::from_reader(BufReader::new(file))
            .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?;
        if settings.version != VERSION {
            return Err(io::ErrorKind::InvalidData.into());
        }
        Ok(Some(settings))
    }

    pub(crate) fn save(&self, settings: Settings) -> io::Result<()> {
        let _writer = WRITER
            .lock()
            .map_err(|_| io::Error::other("settings writer is unavailable"))?;
        self.read()?;
        replace_json(&self.path, &settings)
    }
}
