use std::{fs, os::windows::fs::OpenOptionsExt};

use super::{LaunchAtLogin, Preferences, SettingsStore, StartupError};
use crate::storage::settings::Language;

#[test]
#[ignore = "requires an interactive Windows desktop"]
fn settings_keyboard_navigation_skips_the_page_focus_target() {
    use i_slint_core::{
        accessibility::AccessibleStringProperty, item_tree::ParentItemTraversalMode,
        window::WindowInner,
    };
    use slint::{
        ComponentHandle,
        platform::{Key, WindowEvent},
    };

    crate::startup_tests::select_test_backend().unwrap();
    let settings = crate::SettingsWindow::new().unwrap();
    settings.show().unwrap();
    let window = settings.window();
    settings.invoke_focus_page();
    for (key, label) in [
        (Key::Tab, "Back to Main"),
        (Key::Tab, "Interface language"),
        (Key::Tab, "Launch at login"),
        (Key::Tab, "Copy code shortcut"),
        (Key::Tab, "Change copy code shortcut"),
        (Key::Backtab, "Copy code shortcut"),
        (Key::Backtab, "Launch at login"),
        (Key::Backtab, "Interface language"),
        (Key::Backtab, "Back to Main"),
        (Key::Backtab, "Popup duration (seconds)"),
        (Key::Tab, "Back to Main"),
    ] {
        window.dispatch_event(WindowEvent::KeyPressed { text: key.into() });
        window.dispatch_event(WindowEvent::KeyReleased { text: key.into() });
        let focused = WindowInner::from_pub(window).focus_item.borrow().upgrade();
        let actual = std::iter::successors(focused, |item| {
            item.parent_item(ParentItemTraversalMode::StopAtPopups)
        })
        .find_map(|item| item.accessible_string_property(AccessibleStringProperty::Label));
        assert_eq!(actual.as_deref(), Some(label));
    }
}

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
    fs::write(&path, br#"{"version":4}"#).unwrap();
    assert!(preferences.save_startup(false).is_err());
    assert!(preferences.is_startup_enabled().unwrap());
    assert_eq!(fs::read(&path).unwrap(), br#"{"version":4}"#);
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
    store.save(&Default::default()).unwrap();
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
    let other_installation: Vec<_> =
        r#""C:\Other Pin Glide\Pin Glide.exe" --startup"#.encode_utf16().chain([0]).collect();
    for prior in [None, Some(other_installation.as_slice())] {
        startup.0.replace(prior).unwrap();
        assert!(matches!(
            preferences.save_startup(true),
            Err(StartupError::WindowsOverride)
        ));
        assert_eq!(startup.0.read().unwrap().as_deref(), prior);
        assert!(!startup.0.enabled().unwrap());
        assert_eq!(fs::read(&path).unwrap(), committed);
    }
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
