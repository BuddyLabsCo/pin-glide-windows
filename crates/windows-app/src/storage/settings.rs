use std::{
    fs::File,
    io,
    io::BufReader,
    num::NonZeroU32,
    path::{Path, PathBuf},
    sync::MutexGuard,
};

use serde::{Deserialize, Serialize};

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

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Settings {
    version: u32,
    #[serde(default)]
    pub(crate) language: Language,
    #[serde(default = "default_launch_at_login")]
    pub(crate) launch_at_login: bool,
    #[serde(default = "default_popup_duration_seconds")]
    pub(crate) popup_duration_seconds: NonZeroU32,
}

fn default_launch_at_login() -> bool {
    true
}

pub(crate) const fn default_popup_duration_seconds() -> NonZeroU32 {
    NonZeroU32::new(30).unwrap()
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            version: VERSION,
            language: Language::System,
            launch_at_login: default_launch_at_login(),
            popup_duration_seconds: default_popup_duration_seconds(),
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
        let file = match File::open(&self.path) {
            Ok(file) => file,
            Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
            Err(error) => return Err(error),
        };
        let mut settings: Settings = serde_json::from_reader(BufReader::new(file))
            .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?;
        if !matches!(settings.version, 1 | VERSION) {
            return Err(io::ErrorKind::InvalidData.into());
        }
        settings.version = VERSION;
        Ok(Some(settings))
    }

    pub(crate) fn save(&self, settings: Settings) -> io::Result<()> {
        self.stage(settings)?.commit()
    }

    pub(crate) fn stage(&self, settings: Settings) -> io::Result<StagedSettings<'_>> {
        let writer = WRITER
            .lock()
            .map_err(|_| io::Error::other("settings writer is unavailable"))?;
        self.read()?;
        Ok(StagedSettings {
            _writer: writer,
            file: stage_json(&self.path, &settings)?,
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
