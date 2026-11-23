use super::{
    AppProcess, Fixture, assert_label, capture_client, click, key, main_capture, post,
    read_settings, seed_registration, visible, wait_for_window, window_size,
};
use main_capture::{accessible_name, edit_value, live_region_is_polite};
use std::{fs, fs::OpenOptions, os::windows::fs::OpenOptionsExt, path::Path};
use windows_sys::Win32::{
    Foundation::HWND,
    Globalization::GetUserDefaultUILanguage,
    UI::{
        HiDpi::{DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2, SetThreadDpiAwarenessContext},
        Input::KeyboardAndMouse::{VK_DOWN, VK_END, VK_RETURN, VK_TAB},
        WindowsAndMessaging::WM_CLOSE,
    },
};

const LABEL: &str = "弹窗显示时长（秒）";
const INVALID: &str = "请输入大于零的整数秒数。";

#[test]
#[ignore = "requires an unlocked interactive Windows desktop"]
fn popup_duration_saves_on_blur_and_preserves_the_saved_value_on_errors() {
    // SAFETY: set the thread's DPI context before using fixture window coordinates.
    unsafe {
        assert!(!SetThreadDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2).is_null())
    };
    let fixture = Fixture::new();
    seed_registration(&fixture.path().join("registration.json"), false);
    let path = fixture.path().join("settings.json");
    let child = AppProcess(fixture.command().spawn().unwrap());
    let (main, settings) = open_settings(&child);
    // SAFETY: Windows returns the current user's UI language without side effects.
    if unsafe { GetUserDefaultUILanguage() & 0x03ff != 4 } {
        click(settings, 350, 90);
        key(settings, VK_DOWN);
        key(settings, VK_RETURN);
    }
    assert_label(settings, LABEL);
    let handle = settings as usize;
    let edit = |value| {
        click(settings, 330, 270);
        edit_value(handle, LABEL, Some(value));
        super::wait_for(
            || edit_value(handle, LABEL, None) == value,
            "duration draft",
        );
    };
    assert_eq!(edit_value(handle, LABEL, None), "30");
    click(settings, 80, 270);
    capture_client(settings, 420, 392, "settings-duration-focused");
    key(settings, VK_END);
    key(settings, b'1'.into());
    super::wait_for(
        || edit_value(handle, LABEL, None) == "301",
        "label must focus the duration editor for keyboard input",
    );
    edit_value(handle, LABEL, Some("30"));
    click(settings, 210, 350);
    capture_client(settings, 420, 392, "settings-duration-default");

    let previous = fs::read(&path).unwrap();
    for (draft, state) in [
        ("", "settings-duration-empty"),
        ("-1", "settings-duration-negative"),
        ("1.5", "settings-duration-fraction"),
        ("0", "settings-duration-invalid"),
        ("abc", "settings-duration-non-numeric"),
    ] {
        edit(draft);
        key(settings, VK_TAB);
        assert_label(settings, INVALID);
        assert!(live_region_is_polite(handle, INVALID));
        assert_eq!(edit_value(handle, LABEL, None), draft);
        assert_eq!(fs::read(&path).unwrap(), previous);
        click(settings, 210, 350);
        capture_client(settings, 420, 392, state);
        edit("30");
        click(settings, 210, 350);
        super::wait_for(
            || !accessible_name(handle, INVALID).unwrap_or(true),
            "a valid blur must clear the previous error",
        );
    }
    edit("40");
    assert_eq!(fs::read(&path).unwrap(), previous);
    click(settings, 220, 270);
    assert_duration(&path, 40);

    edit(" 45 ");
    assert_eq!(read_settings(&path)["popup_duration_seconds"], 40);
    click(settings, 365, 150);
    assert_duration(&path, 45);
    assert_eq!(edit_value(handle, LABEL, None), "45");
    super::wait_for(
        || fixture.startup().is_none(),
        "switch must turn startup off",
    );
    click(settings, 365, 150);
    super::wait_for(
        || fixture.startup().is_some(),
        "switch must turn startup on",
    );
    click(settings, 210, 350);
    capture_client(settings, 420, 392, "settings-duration-saved");

    let locked = OpenOptions::new()
        .read(true)
        .share_mode(1)
        .open(&path)
        .unwrap();
    edit("60");
    click(settings, 210, 350);
    assert_label(settings, "无法保存弹窗显示时长，请重试。");
    assert_eq!(edit_value(handle, LABEL, None), "45");
    assert_eq!(read_settings(&path)["popup_duration_seconds"], 45);
    capture_client(settings, 420, 392, "settings-duration-save-failed");
    drop(locked);
    edit("4294967296");
    click(settings, 35, 36);
    super::wait_for(|| visible(main), "Back must show Main");
    assert_duration(&path, 4294967296);
    click(main, 360, 40);
    super::wait_for(|| visible(settings), "Main must reopen Settings");
    edit("90");
    post(settings, WM_CLOSE, 0, 0);
    super::wait_for(|| !visible(settings), "native Close must hide Settings");
    assert_duration(&path, 90);
    drop(child);

    let restarted = AppProcess(fixture.command().spawn().unwrap());
    let (_, settings) = open_settings(&restarted);
    assert_label(settings, LABEL);
    assert_eq!(edit_value(settings as usize, LABEL, None), "90");
    drop(restarted);
    fixture.verify_system_settings();
}

fn open_settings(child: &AppProcess) -> (HWND, HWND) {
    let main = wait_for_window(child.0.id(), |window| {
        window_size::client_size(window, 400, 576).is_some()
    });
    click(main, 360, 40);
    let settings = wait_for_window(child.0.id(), |window| {
        window_size::client_size(window, 420, 392).is_some()
    });
    (main, settings)
}

fn assert_duration(path: &Path, seconds: u64) {
    super::wait_for(
        || read_settings(path)["popup_duration_seconds"] == seconds,
        "duration must be saved",
    );
}
