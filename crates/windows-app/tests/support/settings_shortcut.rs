use super::{
    AppProcess, Fixture, assert_label, capture_client, click, main_capture, post, read_settings,
    seed_registration, visible, wait_for, wait_for_window, window_size,
};
use std::{fs, os::windows::fs::OpenOptionsExt};
use windows_sys::Win32::{
    Foundation::HWND,
    UI::{
        HiDpi::{DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2, SetThreadDpiAwarenessContext},
        Input::KeyboardAndMouse::{
            MOD_ALT, MOD_CONTROL, RegisterHotKey, UnregisterHotKey, VK_CONTROL, VK_ESCAPE, VK_MENU,
            VK_RETURN,
        },
        WindowsAndMessaging::{WM_KEYDOWN, WM_KEYUP},
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
    let occupied = Occupied::new(b'Q');
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
    assert_eq!(value(), "Ctrl + Alt + V");
    assert_eq!(fs::read(&path).unwrap(), saved);
    drop(locked);
    begin(settings);
    send_keys(settings, &[VK_CONTROL, VK_MENU, b'Q'.into()]);
    send_keys(settings, &[VK_RETURN]);
    wait_for(
        || value() == "Ctrl + Alt + Q",
        "retry must replace the effective shortcut",
    );
    assert_eq!(read_settings(&path)["shortcut"]["key"], b'Q');
    drop(Occupied::new(b'V'));

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
    drop(restarted);
    let occupied = Occupied::new(b'Q');
    let blocked = AppProcess(
        fixture
            .command()
            .env("PINGLIDE_TEST_REDUCED_MOTION", "1")
            .spawn()
            .unwrap(),
    );
    let main = wait_for_window(blocked.0.id(), |window| {
        visible(window) && window_size::client_size(window, 400, 576).is_some()
    });
    click(main, 360, 40);
    let settings = wait_for_window(blocked.0.id(), |window| {
        visible(window) && window_size::client_size(window, 420, 392).is_some()
    });
    assert_label(settings, "此快捷键已被占用，原有快捷键保持不变。");
    assert_eq!(
        main_capture::edit_value(settings as usize, LABEL, None),
        "未设置"
    );
    click(settings, 354, 210);
    wait_for(
        || read_settings(&path).get("shortcut").is_none(),
        "Clear must remove the saved shortcut even when it could not be restored",
    );
    drop(occupied);

    begin(settings);
    send_keys(settings, &[VK_CONTROL, VK_MENU, b'Q'.into()]);
    send_keys(settings, &[VK_RETURN]);
    let value = || main_capture::edit_value(settings as usize, LABEL, None);
    wait_for(|| value() == "Ctrl + Alt + Q", "saved shortcut");
    click(settings, 354, 210);
    wait_for(
        || value() == "未设置",
        "Clear must remove the effective shortcut",
    );
    assert!(read_settings(&path).get("shortcut").is_none());
    drop(Occupied::new(b'Q'));
}

struct Occupied;

impl Occupied {
    fn new(key: u8) -> Self {
        // SAFETY: the test reserves and releases the chord on the same thread.
        let registered =
            unsafe { RegisterHotKey(std::ptr::null_mut(), 1, MOD_CONTROL | MOD_ALT, key.into()) };
        assert_ne!(registered, 0);
        Self
    }
}

impl Drop for Occupied {
    fn drop(&mut self) {
        // SAFETY: release this test thread's registration.
        unsafe { UnregisterHotKey(std::ptr::null_mut(), 1) };
    }
}

fn begin(settings: HWND) {
    click(settings, 302, 210);
    assert_label(settings, HINT);
}

fn send_keys(settings: HWND, keys: &[u16]) {
    for &key in keys {
        post(settings, WM_KEYDOWN, key.into(), 1);
    }
    for &key in keys.iter().rev() {
        post(settings, WM_KEYUP, key.into(), 0xc001_0001_u32 as isize);
    }
}
