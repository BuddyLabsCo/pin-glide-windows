#![cfg(windows)]

#[path = "support/app_process.rs"]
mod app_process;
#[allow(dead_code)]
#[path = "support/dpapi.rs"]
mod dpapi;
#[allow(dead_code)]
#[path = "support/main_capture.rs"]
mod main_capture;
mod support;
#[path = "support/window_size.rs"]
mod window_size;

use std::{
    fs::{self, OpenOptions},
    os::windows::fs::OpenOptionsExt,
    path::Path,
    ptr::null_mut,
    thread,
    time::Duration,
};

use app_process::{AppProcess, Fixture};
use serde_json::{Value, json};
use support::wait_for;
use windows_sys::Win32::{
    Foundation::{HWND, LPARAM, RECT},
    Globalization::GetUserDefaultUILanguage,
    UI::{
        HiDpi::{
            DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2, GetDpiForWindow,
            SetThreadDpiAwarenessContext,
        },
        Input::KeyboardAndMouse::{VK_DOWN, VK_ESCAPE, VK_RETURN},
        WindowsAndMessaging::{
            EnumWindows, GetClassNameW, GetWindowRect, GetWindowThreadProcessId, IsWindowVisible,
            PostMessageW, WM_APP, WM_KEYDOWN, WM_KEYUP, WM_LBUTTONDOWN, WM_LBUTTONUP, WM_MOUSEMOVE,
            WM_RBUTTONUP,
        },
    },
};

#[test]
#[ignore = "requires an unlocked interactive Windows desktop"]
fn main_and_tray_settings_show_saved_language_and_actual_startup() {
    // SAFETY: use physical coordinates for windows owned by the fixture process.
    unsafe {
        assert!(!SetThreadDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2).is_null())
    };
    let fixture = Fixture::new();
    let registration = fixture.path().join("registration.json");
    seed_registration(&registration, false);
    let child = AppProcess(fixture.command().spawn().unwrap());
    let process = child.0.id();
    let main = wait_for_window(process, |window| {
        window_size::client_size(window, 400, 576).is_some()
    });
    let settings_path = fixture.path().join("settings.json");
    assert_language(&settings_path, "system");
    // SAFETY: Windows returns this user's UI language without side effects.
    let system_chinese = unsafe { GetUserDefaultUILanguage() & 0x03ff == 4 };
    assert_label(main, if system_chinese { "设置" } else { "Settings" });
    capture_client(main, 400, 576, "main-ready");
    click(main, 360, 40);
    let settings = wait_for_window(process, |window| {
        window_size::client_size(window, 420, 392).is_some()
    });
    wait_for(
        || !visible(main),
        "Main must be hidden while Settings is open",
    );
    assert!(fixture.startup().is_some());
    assert_label(
        settings,
        if system_chinese {
            "界面语言"
        } else {
            "Interface language"
        },
    );
    capture_client(settings, 420, 392, "settings-default");

    click(settings, 350, 90);
    capture_client(settings, 420, 392, "settings-language-open");
    key(settings, VK_DOWN);
    key(settings, VK_RETURN);
    assert_language(&settings_path, "chinese");
    assert_label(settings, "开机启动");
    click(settings, 210, 350);
    capture_client(settings, 420, 392, "settings-language-zh");

    click(settings, 350, 90);
    key(settings, VK_DOWN);
    key(settings, VK_RETURN);
    assert_language(&settings_path, "english");
    assert_label(settings, "Launch at login");
    click(settings, 210, 350);
    capture_client(settings, 420, 392, "settings-language-en");
    click(settings, 35, 36);
    wait_for(|| visible(main), "Back must show Main");
    assert_label(main, "我的设备");
    assert_label(main, "Paired devices");

    click(main, 360, 40);
    wait_for(|| visible(settings), "Main must reopen Settings");
    click(settings, 350, 90);
    click(settings, 300, if system_chinese { 94 } else { 134 });
    assert_language(
        &settings_path,
        if system_chinese { "system" } else { "chinese" },
    );
    click(settings, 210, 350);
    click(settings, 365, 150);
    wait_for(
        || fixture.startup().is_none(),
        "turning off must remove the Run entry",
    );
    capture_immediate(settings, "settings-startup-toggle-frame");
    capture_client(settings, 420, 392, "settings-startup-off");
    let locked = OpenOptions::new()
        .read(true)
        .share_mode(1)
        .open(&settings_path)
        .unwrap();
    click(settings, 365, 150);
    assert_label(settings, "无法启用开机启动，可再次打开开关重试。");
    assert!(fixture.startup().is_none());
    capture_client(settings, 420, 392, "settings-startup-failed");
    drop(locked);
    click(settings, 365, 150);
    wait_for(
        || fixture.startup().is_some() && read_settings(&settings_path)["launch_at_login"] == true,
        "retry must enable Windows startup and persist the new setting",
    );
    capture_immediate(settings, "settings-startup-retry-frame");
    capture_client(settings, 420, 392, "settings-startup-retried");

    seed_registration(&registration, true);
    click(settings, 35, 36);
    wait_for(
        || visible(main),
        "Back must show Main after a new Sender arrives",
    );
    assert_label(main, "备用设备");
    let tray = wait_for_window(process, |window| {
        class(window) == "BuddyLabsCo.PinGlide.Tray"
    });
    post(tray, WM_APP + 1, 1, WM_RBUTTONUP as LPARAM);
    let menu = wait_for_window(process, |window| {
        visible(window) && class(window) == "#32768"
    });
    capture_tray(menu);
    key(menu, VK_ESCAPE);
    wait_for(|| !visible(menu), "Escape must close the tray menu");
    post(tray, WM_APP + 1, 1, WM_RBUTTONUP as LPARAM);
    let menu = wait_for_window(process, |window| {
        visible(window) && class(window) == "#32768"
    });
    for code in [VK_DOWN, VK_DOWN, VK_RETURN] {
        key(menu, code);
    }
    wait_for(
        || visible(settings) && !visible(main),
        "paired tray action must show Settings",
    );
    drop(child);
    fixture.verify_system_settings();
}

#[test]
#[ignore = "requires an unlocked interactive Windows desktop"]
fn startup_switch_settles_immediately_with_reduced_motion() {
    // SAFETY: use physical coordinates for windows owned by the fixture process.
    unsafe {
        assert!(!SetThreadDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2).is_null())
    };
    let fixture = Fixture::new();
    seed_registration(&fixture.path().join("registration.json"), false);
    // SAFETY: the UI language query has no side effects.
    let system_chinese = unsafe { GetUserDefaultUILanguage() & 0x03ff == 4 };
    fs::write(
        fixture.path().join("settings.json"),
        json!({
            "version": 1, "language": if system_chinese { "system" } else { "chinese" },
        })
        .to_string(),
    )
    .unwrap();
    let mut command = fixture.command();
    command.env("PINGLIDE_TEST_REDUCED_MOTION", "1");
    let child = AppProcess(command.spawn().unwrap());
    let process = child.0.id();
    let main = wait_for_window(process, |window| {
        window_size::client_size(window, 400, 576).is_some()
    });
    click(main, 360, 40);
    let settings = wait_for_window(process, |window| {
        window_size::client_size(window, 420, 392).is_some()
    });
    click(settings, 365, 150);
    wait_for(
        || fixture.startup().is_some(),
        "reduced-motion switch must enable startup",
    );
    thread::sleep(Duration::from_millis(160));
    let enabled = capture_pixels(settings);
    click(settings, 365, 150);
    wait_for(
        || fixture.startup().is_none(),
        "reduced-motion switch must disable startup",
    );
    capture_immediate(settings, "settings-startup-reduced-motion");
    let first = capture_pixels(settings);
    thread::sleep(Duration::from_millis(220));
    let settled = capture_pixels(settings);
    assert_ne!(
        enabled, settled,
        "on and off must draw different thumb positions"
    );
    assert_eq!(first, settled, "switch pixels must already be settled");
    drop(child);
    fixture.verify_system_settings();
}

fn seed_registration(path: &Path, second_sender: bool) {
    let sender =
        |id, name| json!({"sender_id":id,"protected_key":dpapi::protect(&[7;32]),"name":name});
    let mut pairings = json!({"sample": sender("one", "我的设备")});
    if second_sender {
        pairings["second"] = sender("two", "备用设备");
    }
    fs::write(
        path,
        json!({
            "version":4, "receiver_id":"sample", "active_pairings":pairings
        })
        .to_string(),
    )
    .unwrap();
}

fn read_settings(path: &Path) -> Value {
    serde_json::from_slice(&fs::read(path).unwrap()).unwrap()
}

fn assert_language(path: &Path, expected: &str) {
    wait_for(
        || read_settings(path)["language"] == expected,
        "language selection was not saved",
    );
}

fn assert_label(window: HWND, label: &str) {
    wait_for(
        || main_capture::accessible_name(window as usize, label).unwrap_or(false),
        label,
    );
}

fn wait_for_window(process: u32, predicate: impl Fn(HWND) -> bool) -> HWND {
    struct Search<'a> {
        process: u32,
        predicate: &'a dyn Fn(HWND) -> bool,
        window: HWND,
    }
    unsafe extern "system" fn visit(window: HWND, param: LPARAM) -> i32 {
        // SAFETY: enumeration calls synchronously with the live search record.
        let search = unsafe { &mut *(param as *mut Search<'_>) };
        let mut process = 0;
        // SAFETY: Windows supplies a live window and the process output is writable.
        unsafe { GetWindowThreadProcessId(window, &mut process) };
        if process == search.process && (search.predicate)(window) {
            search.window = window;
            return 0;
        }
        1
    }
    let mut search = Search {
        process,
        predicate: &predicate,
        window: null_mut(),
    };
    wait_for(
        || {
            // SAFETY: search stays live throughout the synchronous enumeration.
            unsafe { EnumWindows(Some(visit), &raw mut search as LPARAM) };
            !search.window.is_null()
        },
        "fixture window did not open",
    );
    search.window
}

fn visible(window: HWND) -> bool {
    // SAFETY: the handle was obtained from Windows and is only queried.
    unsafe { IsWindowVisible(window) != 0 }
}

fn class(window: HWND) -> String {
    let mut name = [0u16; 128];
    // SAFETY: the buffer has the UTF-16 capacity supplied to Windows.
    let len = unsafe { GetClassNameW(window, name.as_mut_ptr(), name.len() as i32) };
    String::from_utf16_lossy(&name[..len as usize])
}

fn post(window: HWND, message: u32, wparam: usize, lparam: LPARAM) {
    // SAFETY: these input messages have no pointer payload and target the fixture window.
    unsafe { assert_ne!(PostMessageW(window, message, wparam, lparam), 0) };
}

fn click(window: HWND, x: isize, y: isize) {
    // SAFETY: Windows supplies the handle and GetDpiForWindow has no side effects.
    let dpi = unsafe { GetDpiForWindow(window) } as isize;
    let point = (x * dpi / 96) | ((y * dpi / 96) << 16);
    post(window, WM_MOUSEMOVE, 0, point);
    post(window, WM_LBUTTONDOWN, 1, point);
    post(window, WM_LBUTTONUP, 0, point);
}

fn key(window: HWND, code: u16) {
    post(window, WM_KEYDOWN, code.into(), 1);
    post(window, WM_KEYUP, code.into(), 0xc001_0001_u32 as LPARAM);
}

fn capture_client(window: HWND, width: i32, height: i32, state: &str) {
    if std::env::var_os("PIN_GLIDE_SETTINGS_EVIDENCE").is_some() {
        thread::sleep(Duration::from_millis(250));
        assert!(window_size::client_size(window, width, height).is_some());
        capture_immediate(window, state);
    }
}

fn capture_immediate(window: HWND, state: &str) {
    if let Some(directory) = std::env::var_os("PIN_GLIDE_SETTINGS_EVIDENCE") {
        let mut rect = RECT::default();
        // SAFETY: Settings is live and the client bounds output is writable.
        unsafe {
            assert_ne!(
                windows_sys::Win32::UI::WindowsAndMessaging::GetClientRect(window, &raw mut rect),
                0
            );
        }
        main_capture::capture_client(
            window,
            (rect.right, rect.bottom),
            &Path::new(&directory).join(format!("{state}.ppm")),
        );
    }
}

fn capture_pixels(window: HWND) -> Vec<u8> {
    use windows_sys::Win32::Graphics::Gdi::{GetDC, GetPixel, ReleaseDC};
    // SAFETY: the live Settings window owns the device context until it is released.
    unsafe {
        let dpi = GetDpiForWindow(window) as i32;
        let dc = GetDC(window);
        let pixels = (347..385)
            .map(|x| GetPixel(dc, x * dpi / 96, 150 * dpi / 96))
            .flat_map(u32::to_ne_bytes)
            .collect();
        ReleaseDC(window, dc);
        pixels
    }
}

fn capture_tray(menu: HWND) {
    if let Some(directory) = std::env::var_os("PIN_GLIDE_SETTINGS_EVIDENCE") {
        thread::sleep(Duration::from_millis(250));
        let mut bounds = RECT::default();
        // SAFETY: Windows enumerated the menu HWND and the bounds record is writable.
        unsafe { assert_ne!(GetWindowRect(menu, &raw mut bounds), 0) };
        main_capture::capture(
            null_mut(),
            (bounds.left, bounds.top),
            (bounds.right - bounds.left, bounds.bottom - bounds.top),
            &Path::new(&directory).join("tray-context-menu.ppm"),
        );
    }
}
