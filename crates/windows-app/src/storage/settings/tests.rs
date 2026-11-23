use super::{Language, SettingsStore};
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
    settings.language = Language::Chinese;
    settings.launch_at_login = false;
    store.save(settings).unwrap();

    let reopened = SettingsStore::new(path).read().unwrap().unwrap();
    assert_eq!(reopened, settings);
}

#[test]
fn invalid_or_future_settings_survive_every_save_attempt() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("settings.json");
    let store = SettingsStore::new(path.clone());
    for bytes in [
        b"broken JSON".as_slice(),
        br#"{"version":2,"language":"english"}"#,
        br#"{"version":1,"language":"unknown"}"#,
    ] {
        fs::write(&path, bytes).unwrap();
        assert_eq!(store.read().unwrap_err().kind(), io::ErrorKind::InvalidData);
        assert!(store.save(Default::default()).is_err());
        assert_eq!(fs::read(&path).unwrap(), bytes);
    }
}

#[test]
fn settings_keep_the_previous_commit_when_replacement_is_denied() {
    use std::os::windows::fs::OpenOptionsExt;

    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("settings.json");
    let store = SettingsStore::new(path.clone());
    store.save(Default::default()).unwrap();
    let previous = store.read().unwrap().unwrap();
    let locked = fs::OpenOptions::new()
        .read(true)
        .share_mode(1)
        .open(path)
        .unwrap();
    let mut next = previous;
    next.language = Language::English;
    assert!(store.save(next).is_err());
    assert_eq!(store.read().unwrap(), Some(previous));
    drop(locked);
    store.save(next).unwrap();
    assert_eq!(store.read().unwrap(), Some(next));
}
