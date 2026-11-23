use std::{fs, os::windows::fs::OpenOptionsExt};

use super::{LaunchAtLogin, Preferences, SettingsStore, StartupError};
use crate::storage::settings::Language;

#[test]
fn launch_at_login_keeps_the_working_registration_when_settings_cannot_be_saved() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("settings.json");
    let name = directory.path().file_name().unwrap().to_str().unwrap();
    let cleanup = RegistryCleanup(LaunchAtLogin::for_test(name));
    let mut preferences = Preferences::load(
        SettingsStore::new(path.clone()),
        Ok(LaunchAtLogin::for_test(name)),
    );
    assert!(preferences.is_startup_enabled().unwrap());
    let locked = fs::OpenOptions::new()
        .read(true)
        .share_mode(1)
        .open(&path)
        .unwrap();
    assert!(matches!(
        preferences.save_startup(false),
        Err(StartupError::Change(false, _))
    ));
    assert!(preferences.is_startup_enabled().unwrap());
    assert!(preferences.save_language(Language::Chinese).is_err());
    assert_eq!(
        preferences.store.read().unwrap().unwrap().language,
        Language::System
    );
    drop(locked);
    fs::write(&path, br#"{"version":2}"#).unwrap();
    assert!(preferences.save_startup(false).is_err());
    assert!(preferences.is_startup_enabled().unwrap());
    assert_eq!(fs::read(&path).unwrap(), br#"{"version":2}"#);
    fs::write(&path, br#"{"version":1}"#).unwrap();
    preferences.save_startup(false).unwrap();
    assert!(!preferences.is_startup_enabled().unwrap());
    preferences.save_language(Language::Chinese).unwrap();
    let restarted = Preferences::load(SettingsStore::new(path), Ok(LaunchAtLogin::for_test(name)));
    assert_eq!(restarted.settings.language, Language::Chinese);
    assert!(!restarted.is_startup_enabled().unwrap());
    cleanup.0.set(true).unwrap();
    assert!(restarted.is_startup_enabled().unwrap());
    assert!(!restarted.settings.launch_at_login);
}

#[test]
fn launch_at_login_respects_the_windows_override_and_recovers_after_it_is_cleared() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("settings.json");
    let name = directory.path().file_name().unwrap().to_str().unwrap();
    let startup = RegistryCleanup(LaunchAtLogin::for_test(name));
    startup.0.set(true).unwrap();
    let store = SettingsStore::new(path.clone());
    store.save(Default::default()).unwrap();
    let committed = fs::read(&path).unwrap();
    let mut preferences = Preferences::load(store, Ok(LaunchAtLogin::for_test(name)));
    startup.0.set_test_approval(false);
    assert!(!startup.0.enabled().unwrap());
    let entry = startup.0.read().unwrap();
    assert!(matches!(
        preferences.save_startup(true),
        Err(StartupError::WindowsOverride)
    ));
    assert_eq!(startup.0.read().unwrap(), entry);
    assert_eq!(fs::read(&path).unwrap(), committed);
    startup.0.set(false).unwrap();
    assert!(matches!(
        preferences.save_startup(true),
        Err(StartupError::WindowsOverride)
    ));
    assert_eq!(startup.0.read().unwrap(), entry);
    assert!(!startup.0.enabled().unwrap());
    assert_eq!(fs::read(&path).unwrap(), committed);
    startup.0.set_test_approval(true);
    preferences.save_startup(true).unwrap();
    assert!(startup.0.enabled().unwrap());
}

struct RegistryCleanup(LaunchAtLogin);

impl Drop for RegistryCleanup {
    fn drop(&mut self) {
        self.0.set_test_approval(true);
        let _ = self.0.set(false);
    }
}
