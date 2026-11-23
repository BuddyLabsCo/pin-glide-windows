use std::{fs, os::windows::fs::OpenOptionsExt};

use super::{LaunchAtLogin, Preferences, SettingsStore};
use crate::storage::RegistrationStore;
use crate::storage::settings::Language;

thread_local! {
    static REGISTRATION: std::cell::RefCell<Option<RegistrationStore>> = const { std::cell::RefCell::new(None) };
}

pub(in crate::ui) fn test_registration() -> Option<RegistrationStore> {
    REGISTRATION.with_borrow(Clone::clone)
}

#[allow(dead_code)]
#[path = "../../../tests/support/main_capture.rs"]
mod main_capture;
#[allow(dead_code)]
#[path = "../../../tests/support/window_size.rs"]
mod window_size;

#[test]
fn launch_at_login_keeps_the_working_registration_when_settings_cannot_be_saved() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("settings.json");
    let name = format!(
        "Pin Glide test {}",
        directory.path().file_name().unwrap().display()
    );
    let cleanup = RegistryCleanup(LaunchAtLogin::for_test(&name));
    let startup = LaunchAtLogin::for_test(&name);
    let mut preferences = Preferences::load(SettingsStore::new(path.clone()), Ok(startup));
    assert!(preferences.is_startup_enabled().unwrap());

    let locked = fs::OpenOptions::new()
        .read(true)
        .share_mode(1)
        .open(&path)
        .unwrap();
    assert!(preferences.save_startup(false).is_err());
    assert!(preferences.is_startup_enabled().unwrap());
    assert!(preferences.save_language(Language::Chinese).is_err());
    assert_eq!(
        preferences.store.read().unwrap().unwrap().language,
        Language::System
    );
    drop(locked);

    preferences.save_startup(false).unwrap();
    assert!(!preferences.is_startup_enabled().unwrap());
    preferences.save_language(Language::Chinese).unwrap();
    let restarted = Preferences::load(SettingsStore::new(path), Ok(LaunchAtLogin::for_test(&name)));
    assert_eq!(restarted.settings.language, Language::Chinese);
    assert!(!restarted.is_startup_enabled().unwrap());
    cleanup.0.set(true).unwrap();
    assert!(restarted.is_startup_enabled().unwrap());
    assert!(!restarted.settings.launch_at_login);
}

#[test]
fn launch_at_login_reflects_a_windows_startup_apps_override() {
    use windows_sys::Win32::System::Registry::{
        HKEY_CURRENT_USER, REG_BINARY, RegDeleteKeyValueW, RegSetKeyValueW,
    };

    let directory = tempfile::tempdir().unwrap();
    let name = format!(
        "Pin Glide test {}",
        directory.path().file_name().unwrap().display()
    );
    let startup = RegistryCleanup(LaunchAtLogin::for_test(&name));
    startup.0.set(true).unwrap();
    let key = crate::platform::wide(
        r"Software\Microsoft\Windows\CurrentVersion\Explorer\StartupApproved\Run",
    );
    let name = crate::platform::wide(&name);
    let approval = [3u8, 0, 0, 0, 1, 2, 3, 4, 5, 6, 7, 8];
    #[expect(
        clippy::undocumented_unsafe_blocks,
        reason = "writes and removes one uniquely named test value"
    )]
    unsafe {
        assert_eq!(
            RegSetKeyValueW(
                HKEY_CURRENT_USER,
                key.as_ptr(),
                name.as_ptr(),
                REG_BINARY,
                approval.as_ptr().cast(),
                approval.len() as u32,
            ),
            0
        );
        let enabled = startup.0.enabled();
        assert_eq!(
            RegDeleteKeyValueW(HKEY_CURRENT_USER, key.as_ptr(), name.as_ptr()),
            0
        );
        assert!(!enabled.unwrap());
        assert!(startup.0.enabled().unwrap());
    }
}

struct RegistryCleanup(LaunchAtLogin);

impl Drop for RegistryCleanup {
    fn drop(&mut self) {
        let _ = self.0.set(false);
    }
}

#[test]
#[ignore = "requires an unlocked interactive Windows desktop"]
fn main_and_tray_settings_show_saved_language_and_actual_startup() {
    use std::{
        io::{BufWriter, Write},
        path::Path,
        time::Duration,
    };

    use slint::{
        ComponentHandle, LogicalPosition, Model, Timer, Window,
        platform::{Key, PointerEventButton, WindowEvent},
    };

    use crate::{
        platform,
        startup_tests::{open_tray_menu, select_tray_menu},
        ui,
    };
    use windows_sys::Win32::{
        Foundation::{HWND, RECT},
        System::Threading::GetCurrentThreadId,
        UI::WindowsAndMessaging::GetWindowRect,
    };

    fn click(window: &Window, x: f32, y: f32) {
        let position = LogicalPosition::new(x, y);
        let button = PointerEventButton::Left;
        window.dispatch_event(WindowEvent::PointerMoved { position });
        window.dispatch_event(WindowEvent::PointerPressed { position, button });
        window.dispatch_event(WindowEvent::PointerReleased { position, button });
        window.dispatch_event(WindowEvent::PointerExited);
    }

    fn capture(window: &Window, name: &str) {
        if let Some(directory) = std::env::var_os("PIN_GLIDE_SETTINGS_EVIDENCE") {
            let pixels = window.take_snapshot().unwrap();
            let mut file = BufWriter::new(
                fs::File::create(Path::new(&directory).join(format!("{name}.ppm"))).unwrap(),
            );
            write!(file, "P6\n{} {}\n255\n", pixels.width(), pixels.height()).unwrap();
            for pixel in pixels.as_slice() {
                file.write_all(&[pixel.r, pixel.g, pixel.b]).unwrap();
            }
        }
    }

    platform::enable_per_monitor_dpi().unwrap();
    crate::startup_tests::select_test_backend().unwrap();
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("settings.json");
    let name = format!("Pin Glide desktop {}", std::process::id());
    let setup = RegistryCleanup(LaunchAtLogin::for_test(&name));
    let store = SettingsStore::new(path.clone());
    let mut settings = crate::storage::settings::Settings::default();
    settings.language = Language::Chinese;
    if platform::chinese() {
        settings.language = Language::System;
    }
    let follow_system = settings.language == Language::System;
    store.save(settings).unwrap();
    setup.0.set(true).unwrap();
    super::install(Preferences::load(store, Ok(LaunchAtLogin::for_test(&name))));

    let registration = directory.path().join("registration.json");
    fs::write(
        &registration,
        r#"{"version":4,"receiver_id":"sample","active_pairings":{}}"#,
    )
    .unwrap();
    let registration = RegistrationStore::new(registration);
    REGISTRATION.set(Some(registration.clone()));
    let tray_name = platform::wide(&format!(
        "BuddyLabsCo.PinGlide.SettingsTest.{}",
        std::process::id()
    ));
    let tray = platform::Tray::new(&tray_name).unwrap();
    #[expect(
        clippy::undocumented_unsafe_blocks,
        reason = "reads the current thread ID"
    )]
    let owner = unsafe { GetCurrentThreadId() };
    let tray_handle = tray.window() as usize;
    let main = ui::show_registered_with(&registration, true, || None)
        .unwrap()
        .unwrap();
    main.set_sender_devices(slint::VecModel::from_slice(&[crate::SenderRowData {
        pairing_id: "sample".into(),
        name: "我的设备".into(),
        needs_repair: false,
    }]));
    Timer::single_shot(Duration::from_millis(250), move || {
        capture(main.window(), "main-ready");
        click(main.window(), 360., 40.);
        let window = super::WINDOW.with_borrow(|window| window.as_ref().unwrap().clone_strong());
        assert!(window.window().is_visible());
        assert!(!main.window().is_visible());
        assert_eq!(
            window
                .window()
                .size()
                .to_logical(window.window().scale_factor()),
            slint::LogicalSize::new(420., 392.)
        );
        assert!(window.get_startup());
        capture(window.window(), "settings-default");

        click(window.window(), 350., 90.);
        capture(window.window(), "settings-language-open");
        window.window().dispatch_event(WindowEvent::KeyPressed {
            text: Key::DownArrow.into(),
        });
        window.window().dispatch_event(WindowEvent::KeyPressed {
            text: Key::Return.into(),
        });
        assert!(window.get_chinese());
        assert_eq!(window.get_language_index(), 1);
        capture(window.window(), "settings-language-zh");

        window.invoke_choose_language(2);
        assert!(!main.get_chinese());
        assert_eq!(
            main.get_sender_devices().row_data(0).unwrap().name,
            "我的设备"
        );
        window.invoke_choose_language(if follow_system { 0 } else { 1 });
        click(window.window(), 35., 36.);
        click(main.window(), 360., 40.);
        click(window.window(), 365., 150.);
        assert!(!window.get_startup());
        assert!(!setup.0.enabled().unwrap());
        capture(window.window(), "settings-startup-off");
        let locked = fs::OpenOptions::new()
            .read(true)
            .share_mode(1)
            .open(&path)
            .unwrap();
        click(window.window(), 365., 150.);
        assert!(!window.get_startup());
        assert_eq!(
            window.get_startup_error(),
            "无法启用开机启动，可再次打开开关重试。"
        );
        Timer::single_shot(Duration::from_millis(200), move || {
            capture(window.window(), "settings-startup-failed");
            drop(locked);
            click(window.window(), 365., 150.);
            assert!(window.get_startup());
            assert!(window.get_startup_error().is_empty());
            click(window.window(), 35., 36.);
            assert!(main.window().is_visible());
            assert!(!window.window().is_visible());
            let evidence = std::env::var_os("PIN_GLIDE_SETTINGS_EVIDENCE");
            let observer = std::thread::spawn(move || {
                let popup = open_tray_menu(tray_handle as HWND, owner);
                if let Some(directory) = evidence {
                    std::thread::sleep(Duration::from_millis(250));
                    let mut bounds = RECT::default();
                    #[expect(
                        clippy::undocumented_unsafe_blocks,
                        reason = "popup is live and bounds is writable"
                    )]
                    unsafe {
                        assert_ne!(GetWindowRect(popup, &raw mut bounds), 0)
                    };
                    main_capture::capture(
                        std::ptr::null_mut(),
                        (bounds.left, bounds.top),
                        (bounds.right - bounds.left, bounds.bottom - bounds.top),
                        &Path::new(&directory).join("tray-context-menu.ppm"),
                    );
                }
                select_tray_menu(popup, 2);
            });
            Timer::single_shot(Duration::from_millis(500), move || {
                observer.join().unwrap();
                assert!(window.window().is_visible());
                assert!(!main.window().is_visible());
                ui::close();
                slint::quit_event_loop().unwrap();
            });
        });
    });
    slint::run_event_loop_until_quit().unwrap();
}
