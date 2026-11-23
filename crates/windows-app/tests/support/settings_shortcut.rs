use super::{
    AppProcess, Fixture, assert_label, capture_client, click, keyboard, main_capture,
    read_settings, seed_registration, visible, wait_for, wait_for_window, window_size,
};
use std::{fs, os::windows::fs::OpenOptionsExt};
use windows_sys::Win32::{
    Foundation::{ERROR_HOTKEY_ALREADY_REGISTERED, GetLastError, HWND},
    UI::{
        HiDpi::{DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2, SetThreadDpiAwarenessContext},
        Input::KeyboardAndMouse::{
            MOD_ALT, MOD_CONTROL, RegisterHotKey, UnregisterHotKey, VK_CONTROL, VK_ESCAPE, VK_MENU,
            VK_NEXT, VK_RETURN,
        },
    },
};

const LABEL: &str = "复制验证码快捷键";
const HINT: &str = "按下快捷键组合，Enter 应用，Esc 取消。";

#[test]
#[ignore = "requires an unlocked interactive Windows desktop"]
fn shortcut_capture_shows_field_feedback_and_preserves_the_effective_registration() {
    // SAFETY: use physical coordinates when addressing the fixture windows.
    unsafe {
        assert!(
            !SetThreadDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2).is_null()
        );
    }
    let fixture = Fixture::new();
    seed_registration(&fixture.path().join("registration.json"), false);
    let path = fixture.path().join("settings.json");
    let child = AppProcess(fixture.command().spawn().unwrap());
    let main = wait_for_window(child.0.id(), |window| {
        visible(window) && window_size::client_size(window, 400, 576).is_some()
    });
    keyboard::activate(main);
    click(main, 360, 40);
    let settings = wait_for_window(child.0.id(), |window| {
        visible(window) && window_size::client_size(window, 420, 392).is_some()
    });
    if !main_capture::accessible_name(settings as usize, LABEL).unwrap_or(false) {
        click(settings, 350, 90);
        super::key(
            settings,
            windows_sys::Win32::UI::Input::KeyboardAndMouse::VK_DOWN,
        );
        super::key(settings, VK_RETURN);
        assert_label(settings, LABEL);
    }
    let value = || main_capture::edit_value(settings as usize, LABEL, None);
    assert_eq!(value(), "未设置");
    capture_client(settings, 420, 392, "settings-shortcut-default");
    begin(settings);
    send_keys(settings, &[VK_CONTROL, VK_MENU, b'V'.into()]);
    send_keys(settings, &[VK_RETURN]);
    wait_for(
        || value() == "Ctrl + Alt + V",
        "effective shortcut must be displayed",
    );
    assert_eq!(read_settings(&path)["shortcut"]["key"], b'V');
    let saved = fs::read(&path).unwrap();
    capture_client(settings, 420, 392, "settings-shortcut-assigned");
    click(settings, 170, 336);
    begin(settings);
    capture_client(settings, 420, 392, "settings-shortcut-capture");
    send_keys(settings, &[VK_ESCAPE]);
    wait_for(
        || value() == "Ctrl + Alt + V",
        "Escape must restore the displayed shortcut",
    );
    begin(settings);
    send_keys(settings, &[b'Q'.into()]);
    send_keys(settings, &[VK_RETURN]);
    assert_label(settings, "请输入完整且受支持的快捷键组合。");
    capture_client(settings, 420, 392, "settings-shortcut-invalid");
    assert_eq!(value(), "Ctrl + Alt + V");
    assert_eq!(fs::read(&path).unwrap(), saved);

    // SAFETY: reserve a test chord on the current thread to simulate another app owning it.
    let occupied = Occupied::new(MOD_CONTROL | MOD_ALT, b'Q'.into());
    begin(settings);
    send_keys(settings, &[VK_CONTROL, VK_MENU, b'Q'.into()]);
    send_keys(settings, &[VK_RETURN]);
    assert_label(settings, "此快捷键已被占用，原有快捷键保持不变。");
    drop(occupied);
    capture_client(settings, 420, 392, "settings-shortcut-conflict");
    assert_eq!(value(), "Ctrl + Alt + V");
    assert_eq!(fs::read(&path).unwrap(), saved);

    let locked = fs::OpenOptions::new()
        .read(true)
        .share_mode(1)
        .open(&path)
        .unwrap();
    begin(settings);
    send_keys(settings, &[VK_CONTROL, VK_MENU, b'Q'.into()]);
    send_keys(settings, &[VK_RETURN]);
    assert_label(settings, "无法保存快捷键，请重试。");
    capture_client(settings, 420, 392, "settings-shortcut-save-failed");
    assert_eq!(value(), "Ctrl + Alt + V");
    assert_eq!(fs::read(&path).unwrap(), saved);
    assert!(
        !Occupied::available(MOD_CONTROL | MOD_ALT, b'V'.into()),
        "the old OS registration must survive a failed save"
    );
    drop(Occupied::new(MOD_CONTROL | MOD_ALT, b'Q'.into()));
    drop(locked);
    begin(settings);
    send_keys(settings, &[VK_CONTROL, VK_MENU, b'Q'.into()]);
    send_keys(settings, &[VK_RETURN]);
    wait_for(
        || value() == "Ctrl + Alt + Q",
        "retry must replace the effective shortcut",
    );
    assert_eq!(read_settings(&path)["shortcut"]["key"], b'Q');
    drop(Occupied::new(MOD_CONTROL | MOD_ALT, b'V'.into()));

    drop(child);
    let restarted = AppProcess(fixture.command().spawn().unwrap());
    let main = wait_for_window(restarted.0.id(), |window| {
        visible(window) && window_size::client_size(window, 400, 576).is_some()
    });
    click(main, 360, 40);
    let settings = wait_for_window(restarted.0.id(), |window| {
        visible(window) && window_size::client_size(window, 420, 392).is_some()
    });
    assert_eq!(
        main_capture::edit_value(settings as usize, LABEL, None),
        "Ctrl + Alt + Q"
    );
    begin(settings);
    send_keys(settings, &[VK_CONTROL, VK_NEXT]);
    send_keys(settings, &[VK_RETURN]);
    let value = || main_capture::edit_value(settings as usize, LABEL, None);
    wait_for(
        || read_settings(&path)["shortcut"]["key"] == VK_NEXT && value().starts_with("Ctrl + "),
        "Ctrl + Page Down must register and display its key name",
    );
    click(settings, 354, 210);
    wait_for(
        || value() == "未设置",
        "Clear must remove the effective shortcut",
    );
    assert!(read_settings(&path).get("shortcut").is_none());
    drop(Occupied::new(MOD_CONTROL, VK_NEXT.into()));
}

#[test]
#[ignore = "requires an interactive Windows desktop"]
fn unavailable_saved_shortcut_can_be_cleared() {
    // SAFETY: use physical coordinates when addressing the fixture windows.
    unsafe {
        assert!(
            !SetThreadDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2).is_null()
        );
    }
    let fixture = Fixture::new();
    seed_registration(&fixture.path().join("registration.json"), false);
    let path = fixture.path().join("settings.json");
    let saved = serde_json::json!({
        "version": 3,
        "language": "chinese",
        "launch_at_login": false,
        "popup_duration_seconds": 30,
        "shortcut": {"key": b'V', "modifiers": MOD_CONTROL | MOD_ALT},
    });
    fs::write(&path, saved.to_string()).unwrap();

    let child = AppProcess(fixture.command().spawn().unwrap());
    let settings = open_settings(&child);
    assert_eq!(
        main_capture::edit_value(settings as usize, LABEL, None),
        "Ctrl + Alt + V"
    );
    capture_client(settings, 420, 392, "settings-shortcut-restored");
    let locked = fs::OpenOptions::new()
        .read(true)
        .share_mode(1)
        .open(&path)
        .unwrap();
    click(settings, 354, 210);
    assert_label(settings, "无法保存快捷键，请重试。");
    capture_client(settings, 420, 392, "settings-shortcut-clear-save-failed");
    assert_eq!(read_settings(&path), saved);
    assert!(!Occupied::available(MOD_CONTROL | MOD_ALT, b'V'.into()));
    drop(locked);
    drop(child);

    let occupied = Occupied::new(MOD_CONTROL | MOD_ALT, b'V'.into());
    let child = AppProcess(fixture.command().spawn().unwrap());
    let settings = open_settings(&child);
    let feedback = "已保存的快捷键被占用，当前没有生效的快捷键。";
    assert_label(settings, feedback);
    capture_client(settings, 420, 392, "settings-shortcut-restore-conflict");
    assert_eq!(
        main_capture::edit_value(settings as usize, LABEL, None),
        "未设置"
    );
    assert_eq!(read_settings(&path), saved);
    click(settings, 354, 210);
    wait_for(
        || {
            read_settings(&path).get("shortcut").is_none()
                && !main_capture::accessible_name(settings as usize, feedback).unwrap()
        },
        "Clear must remove the unavailable shortcut and its feedback",
    );
    assert!(!Occupied::available(MOD_CONTROL | MOD_ALT, b'V'.into()));
    drop(occupied);
}

fn open_settings(child: &AppProcess) -> HWND {
    let main = wait_for_window(child.0.id(), |window| {
        visible(window) && window_size::client_size(window, 400, 576).is_some()
    });
    click(main, 360, 40);
    wait_for_window(child.0.id(), |window| {
        visible(window) && window_size::client_size(window, 420, 392).is_some()
    })
}

struct Occupied;

impl Occupied {
    fn new(modifiers: u32, key: u32) -> Self {
        // SAFETY: the test reserves and releases the chord on the same thread.
        let registered = unsafe { RegisterHotKey(std::ptr::null_mut(), 1, modifiers, key) };
        assert_ne!(registered, 0);
        Self
    }

    fn available(modifiers: u32, key: u32) -> bool {
        // SAFETY: use a separate id on the test thread and release any successful probe.
        unsafe {
            if RegisterHotKey(std::ptr::null_mut(), 2, modifiers, key) == 0 {
                assert_eq!(GetLastError(), ERROR_HOTKEY_ALREADY_REGISTERED);
                return false;
            }
            assert_ne!(UnregisterHotKey(std::ptr::null_mut(), 2), 0);
        }
        true
    }
}

impl Drop for Occupied {
    fn drop(&mut self) {
        // SAFETY: release this test thread's registration.
        unsafe { UnregisterHotKey(std::ptr::null_mut(), 1) };
    }
}

fn begin(settings: HWND) {
    keyboard::activate(settings);
    click(settings, 302, 210);
    assert_label(settings, HINT);
}

fn send_keys(settings: HWND, keys: &[u16]) {
    keyboard::press_captured(settings, keys);
}
