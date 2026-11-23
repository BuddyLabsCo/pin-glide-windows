use std::{
    fs, io,
    path::{Path, PathBuf},
    sync::MutexGuard,
};

use serde::{Deserialize, Serialize};
use serde_json::Number;

use super::{WRITER, stage_json, windows};

#[cfg(test)]
mod tests;

const VERSION: u32 = 2;

#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum Language {
    #[default]
    System,
    Chinese,
    English,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Settings {
    version: u32,
    #[serde(default)]
    pub(crate) language: Language,
    #[serde(default = "default_launch_at_login")]
    pub(crate) launch_at_login: bool,
    #[serde(default)]
    pub(crate) popup_duration_seconds: PopupDuration,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct LegacySettings {
    #[serde(rename = "version")]
    _version: u32,
    #[serde(default)]
    language: Language,
    #[serde(default = "default_launch_at_login")]
    launch_at_login: bool,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(try_from = "Number")]
pub(crate) struct PopupDuration(Number);

impl PopupDuration {
    pub(crate) fn parse(text: &str) -> Result<Self, &'static str> {
        let digits = text.trim_start_matches('0');
        if digits.is_empty() || !digits.bytes().all(|byte| byte.is_ascii_digit()) {
            return Err("expected positive whole seconds");
        }
        Ok(Self(
            digits
                .parse()
                .map_err(|_| "expected positive whole seconds")?,
        ))
    }

    pub(crate) fn seconds(&self) -> Option<u64> {
        self.0.as_u64()
    }
}

impl TryFrom<Number> for PopupDuration {
    type Error = &'static str;

    fn try_from(number: Number) -> Result<Self, Self::Error> {
        Self::parse(number.as_str())
    }
}

impl std::fmt::Display for PopupDuration {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(formatter)
    }
}

impl Default for PopupDuration {
    fn default() -> Self {
        Self(30_u64.into())
    }
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
            popup_duration_seconds: PopupDuration::default(),
        }
    }
}

pub(crate) struct SettingsStore {
    path: PathBuf,
}

impl SettingsStore {
    pub(crate) fn for_current_user() -> io::Result<Self> {
        Ok(Self::new(windows::app_data()?.join("settings.json")))
    }

    pub(crate) fn new(path: PathBuf) -> Self {
        Self { path }
    }

    pub(crate) fn read(&self) -> io::Result<Option<Settings>> {
        let bytes = match fs::read(&self.path) {
            Ok(bytes) => bytes,
            Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
            Err(error) => return Err(error),
        };
        #[derive(Deserialize)]
        struct Version {
            version: u32,
        }
        let invalid = |error| io::Error::new(io::ErrorKind::InvalidData, error);
        let version: Version = serde_json::from_slice(&bytes).map_err(invalid)?;
        let settings = match version.version {
            1 => {
                let legacy: LegacySettings = serde_json::from_slice(&bytes).map_err(invalid)?;
                Settings {
                    language: legacy.language,
                    launch_at_login: legacy.launch_at_login,
                    ..Settings::default()
                }
            }
            VERSION => serde_json::from_slice(&bytes).map_err(invalid)?,
            _ => return Err(io::ErrorKind::InvalidData.into()),
        };
        Ok(Some(settings))
    }

    pub(crate) fn save(&self, settings: &Settings) -> io::Result<()> {
        self.stage(settings)?.commit()
    }

    pub(crate) fn stage(&self, settings: &Settings) -> io::Result<StagedSettings<'_>> {
        let writer = WRITER
            .lock()
            .map_err(|_| io::Error::other("settings writer is unavailable"))?;
        self.read()?;
        Ok(StagedSettings {
            _writer: writer,
            file: stage_json(&self.path, settings)?,
            path: &self.path,
        })
    }
}

pub(crate) struct StagedSettings<'a> {
    file: tempfile::NamedTempFile,
    path: &'a Path,
    _writer: MutexGuard<'a, ()>,
}

impl StagedSettings<'_> {
    pub(crate) fn commit(self) -> io::Result<()> {
        self.file.persist(self.path).map_err(|error| error.error)?;
        Ok(())
    }
}
