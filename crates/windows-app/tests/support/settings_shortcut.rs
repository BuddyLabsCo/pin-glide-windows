use super::{
    AppProcess, Fixture, assert_label, capture_client, click, keyboard, main_capture,
    read_settings, seed_registration, visible, wait_for, wait_for_window, window_size,
};
use std::{fs, os::windows::fs::OpenOptionsExt};
use windows_sys::Win32::{
    Foundation::{ERROR_HOTKEY_ALREADY_REGISTERED, GetLastError, HWND, POINT},
    Graphics::Gdi::ClientToScreen,
    UI::{
        HiDpi::{
            DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2, GetDpiForWindow,
            SetThreadDpiAwarenessContext,
        },
        Input::KeyboardAndMouse::{
            GetKeyNameTextW, KEYEVENTF_KEYUP, MOD_ALT, MOD_CONTROL, RegisterHotKey,
            UnregisterHotKey, VK_CONTROL, VK_ESCAPE, VK_MENU, VK_NEXT, VK_PACKET, VK_RETURN,
        },
        WindowsAndMessaging::{GetCursorPos, SetCursorPos},
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
    let settings = open_settings(&child, true);
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
    let occupied = Occupied::new(MOD_CONTROL | MOD_ALT, b'Q'.into());
    begin(settings);
    send_keys(settings, &[VK_CONTROL, VK_MENU, b'Q'.into()]);
    send_keys(settings, &[VK_RETURN]);
    assert_label(settings, "此快捷键已被占用。");
    assert_eq!(value(), "未设置");
    assert!(read_settings(&path).get("shortcut").is_none());
    capture_client(settings, 420, 392, "settings-shortcut-first-conflict");
    drop(occupied);
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
    assert_hover_above_feedback(settings, "settings-shortcut-assigned-hover");
    click(settings, 170, 336);
    begin(settings);
    capture_client(settings, 420, 392, "settings-shortcut-capture");
    repeat_command_after_releasing_ctrl(settings, VK_ESCAPE);
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
    assert_hover_above_feedback(settings, "settings-shortcut-invalid-hover");

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
    assert_hover_above_feedback(settings, "settings-shortcut-conflict-hover");

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
    assert_hover_above_feedback(settings, "settings-shortcut-save-failed-hover");
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
    let settings = open_settings(&restarted, false);
    assert_eq!(
        main_capture::edit_value(settings as usize, LABEL, None),
        "Ctrl + Alt + Q"
    );
    begin(settings);
    repeat_command_after_releasing_ctrl(settings, VK_RETURN);
    send_keys(settings, &[VK_RETURN]);
    wait_for(
        || read_settings(&path)["shortcut"]["key"] == VK_RETURN,
        "a fresh Enter press must apply the captured Ctrl + Enter",
    );
    assert_eq!(read_settings(&path)["shortcut"]["modifiers"], MOD_CONTROL);
    begin(settings);
    send_keys(settings, &[VK_CONTROL, VK_NEXT]);
    send_keys(settings, &[VK_RETURN]);
    let value = || main_capture::edit_value(settings as usize, LABEL, None);
    let expected = format!("Ctrl + {}", page_down_name());
    wait_for(
        || value() == expected,
        "the exact Windows key name must be shown",
    );
    let saved = read_settings(&path);
    assert_eq!(saved["shortcut"]["key"], VK_NEXT);
    assert_eq!(saved["shortcut"]["modifiers"], MOD_CONTROL);
    assert!(!Occupied::available(MOD_CONTROL, VK_NEXT.into()));
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
    let settings = open_settings(&child, false);
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
    let settings = open_settings(&child, false);
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

#[test]
#[ignore = "requires an interactive Windows desktop"]
fn windows_registration_failure_is_shown_when_restoring_a_shortcut() {
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
        "version": 3, "language": "chinese", "launch_at_login": false,
        "shortcut": {"key": VK_PACKET, "modifiers": MOD_CONTROL},
    });
    fs::write(&path, saved.to_string()).unwrap();
    let child = AppProcess(fixture.command().spawn().unwrap());
    let settings = open_settings(&child, false);
    assert_label(settings, "无法在 Windows 中注册快捷键，请重试。");
    assert_eq!(
        main_capture::edit_value(settings as usize, LABEL, None),
        "未设置"
    );
    assert_eq!(read_settings(&path), saved);
    capture_client(settings, 420, 392, "settings-shortcut-register-failed");
}

fn assert_hover_above_feedback(window: HWND, state: &str) {
    // SAFETY: Windows supplies the DPI of the live Settings window.
    let scale = |logical| logical * unsafe { GetDpiForWindow(window) } as i32 / 96;
    let pixels =
        |top| main_capture::capture_rgb(window, (scale(155), scale(top)), (scale(115), scale(24)));
    let mut original_cursor = POINT::default();
    // SAFETY: the cursor output is writable.
    unsafe { assert_ne!(GetCursorPos(&raw mut original_cursor), 0) };
    let move_pointer = |x, y| {
        let mut point = POINT {
            x: scale(x),
            y: scale(y),
        };
        // SAFETY: map the live fixture's coordinates and move the pointer without clicking.
        unsafe {
            assert_ne!(ClientToScreen(window, &raw mut point), 0);
            assert_ne!(SetCursorPos(point.x, point.y), 0);
        }
    };
    move_pointer(140, 210);
    let above = pixels(168);
    let feedback = pixels(232);
    move_pointer(228, 210);
    assert_label(window, "Ctrl + Alt + V");
    capture_client(window, 420, 392, state);
    wait_for(
        || pixels(168) != above,
        "the tooltip must appear above the shortcut field",
    );
    assert_eq!(
        pixels(232),
        feedback,
        "the tooltip must leave the feedback visible"
    );
    move_pointer(140, 210);
    wait_for(
        || pixels(168) == above,
        "the tooltip must dismiss on pointer exit",
    );
    // SAFETY: restore the pointer position saved before the fixture hover.
    unsafe { SetCursorPos(original_cursor.x, original_cursor.y) };
}

fn open_settings(child: &AppProcess, activate: bool) -> HWND {
    let main = wait_for_window(child.0.id(), |window| {
        visible(window) && window_size::client_size(window, 400, 576).is_some()
    });
    if activate {
        keyboard::activate(main);
    }
    click(main, 360, 40);
    wait_for_window(child.0.id(), |window| {
        visible(window) && window_size::client_size(window, 420, 392).is_some()
    })
}

fn page_down_name() -> String {
    const EXTENDED_PAGE_DOWN_SCAN_CODE: i32 = 0x0151_0000;
    let mut name = [0; 64];
    // SAFETY: the extended Windows scan code is scalar and the output buffer is writable.
    let length = unsafe {
        GetKeyNameTextW(
            EXTENDED_PAGE_DOWN_SCAN_CODE,
            name.as_mut_ptr(),
            name.len() as i32,
        )
    };
    assert!(length > 0);
    String::from_utf16(&name[..length as usize]).unwrap()
}

fn repeat_command_after_releasing_ctrl(settings: HWND, key: u16) {
    keyboard::send_captured(
        settings,
        &[
            (VK_CONTROL, 0),
            (key, 0),
            (VK_CONTROL, KEYEVENTF_KEYUP),
            (key, 0),
            (key, KEYEVENTF_KEYUP),
        ],
    );
    assert_eq!(
        main_capture::edit_value(settings as usize, LABEL, None),
        "等待输入…",
        "a repeated command key must keep capture active",
    );
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
