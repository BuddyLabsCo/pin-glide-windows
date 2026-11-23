use super::{Language, PopupDuration, SettingsStore};
use std::{fs, io};

#[test]
fn settings_read_defaults_and_migrate_supported_partial_files_on_save() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("settings.json");
    let store = SettingsStore::new(path.clone());
    assert!(store.read().unwrap().is_none());
    fs::write(&path, r#"{"version":1}"#).unwrap();

    let mut settings = store.read().unwrap().unwrap();
    assert_eq!(settings.language, Language::System);
    assert!(settings.launch_at_login);
    assert_eq!(settings.popup_duration_seconds.seconds(), Some(30));
    settings.language = Language::Chinese;
    settings.launch_at_login = false;
    settings.popup_duration_seconds = PopupDuration::parse("4294967296").unwrap();
    store.save(&settings).unwrap();

    let saved: serde_json::Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
    assert_eq!(saved["version"], 2);
    let reopened = SettingsStore::new(path).read().unwrap().unwrap();
    assert_eq!(reopened, settings);
}

#[test]
fn arbitrarily_large_popup_durations_round_trip_without_rounding() {
    let directory = tempfile::tempdir().unwrap();
    let store = SettingsStore::new(directory.path().join("settings.json"));
    let digits = "1844674407370955161600000000001";
    let settings = super::Settings {
        popup_duration_seconds: PopupDuration::parse(digits).unwrap(),
        ..Default::default()
    };
    store.save(&settings).unwrap();
    let duration = store.read().unwrap().unwrap().popup_duration_seconds;
    assert_eq!(duration.to_string(), digits);
    assert_eq!(duration.seconds(), None);
}

#[test]
fn invalid_or_future_settings_survive_every_save_attempt() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("settings.json");
    let store = SettingsStore::new(path.clone());
    for bytes in [
        b"broken JSON".as_slice(),
        br#"{"version":3,"language":"english"}"#,
        br#"{"version":1,"language":"unknown"}"#,
        br#"{"version":1,"popup_duration_seconds":45}"#,
        br#"{"version":1,"popup_duration_seconds":null}"#,
        br#"{"version":2,"popup_duration_seconds":0}"#,
    ] {
        fs::write(&path, bytes).unwrap();
        assert_eq!(store.read().unwrap_err().kind(), io::ErrorKind::InvalidData);
        assert!(store.save(&Default::default()).is_err());
        assert_eq!(fs::read(&path).unwrap(), bytes);
    }
}

#[test]
fn settings_keep_the_previous_commit_when_replacement_is_denied() {
    use std::os::windows::fs::OpenOptionsExt;

    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("settings.json");
    let store = SettingsStore::new(path.clone());
    store.save(&Default::default()).unwrap();
    let previous = store.read().unwrap().unwrap();
    let locked = fs::OpenOptions::new()
        .read(true)
        .share_mode(1)
        .open(path)
        .unwrap();
    let mut next = previous.clone();
    next.language = Language::English;
    assert!(store.save(&next).is_err());
    assert_eq!(store.read().unwrap(), Some(previous));
    drop(locked);
    store.save(&next).unwrap();
    assert_eq!(store.read().unwrap(), Some(next));
}
