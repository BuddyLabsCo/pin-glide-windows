#![cfg(windows)]

#[path = "support/app_process.rs"]
mod app_process;
mod support;
#[path = "support/window_size.rs"]
mod window_size;

use app_process::{AppProcess, Fixture};
use std::io;
use std::thread;
use std::time::{Duration, Instant};

use windows_sys::Win32::Foundation::{HWND, LPARAM, RECT};
use windows_sys::Win32::UI::HiDpi::{
    DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2, SetThreadDpiAwarenessContext,
};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    EnumWindows, GWL_STYLE, GetClientRect, GetWindowLongPtrW, GetWindowThreadProcessId, IsIconic,
    IsWindowVisible, PostMessageW, SW_MINIMIZE, ShowWindowAsync, WM_CLOSE, WS_MAXIMIZEBOX,
    WS_MINIMIZEBOX, WS_THICKFRAME,
};

fn launch_duplicate(fixture: &Fixture, arguments: &[&str], expectation: &str) {
    let mut child = AppProcess(fixture.command().args(arguments).spawn().unwrap());
    wait_for(
        || {
            child.0.try_wait().unwrap().is_some_and(|status| {
                assert!(status.success(), "{expectation}: {status}");
                true
            })
        },
        expectation,
    );
}

fn visible_window(process: u32) -> HWND {
    struct Search {
        process: u32,
        window: HWND,
    }

    unsafe extern "system" fn visit(window: HWND, param: LPARAM) -> i32 {
        // SAFETY: EnumWindows calls synchronously with the live Search pointer provided below.
        let search = unsafe { &mut *(param as *mut Search) };
        let mut owner = 0;
        let mut bounds = RECT::default();
        // SAFETY: window is the current enumeration entry and both output pointers are writable.
        unsafe {
            GetWindowThreadProcessId(window, &mut owner);
            if owner == search.process
                && IsWindowVisible(window) != 0
                && GetClientRect(window, &raw mut bounds) != 0
                && bounds.right > 0
                && bounds.bottom > 0
            {
                search.window = window;
                return 0;
            }
        }
        1
    }

    let mut search = Search {
        process,
        window: std::ptr::null_mut(),
    };
    // SAFETY: search stays alive for all synchronous callbacks.
    unsafe { EnumWindows(Some(visit), &raw mut search as LPARAM) };
    search.window
}

use support::wait_for;

#[test]
#[ignore = "requires an interactive Windows desktop"]
fn startup_stays_hidden_and_explicit_launch_reopens_the_same_process() -> io::Result<()> {
    // SAFETY: use physical coordinates when querying Welcome from this test thread.
    let previous =
        unsafe { SetThreadDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2) };
    assert!(!previous.is_null());
    let fixture = Fixture::new();
    let mut primary = AppProcess(fixture.command().arg("--startup").spawn()?);
    let process = primary.0.id();
    let observe_until = Instant::now() + Duration::from_secs(1);
    while Instant::now() < observe_until {
        assert!(
            visible_window(process).is_null(),
            "startup must not open a window"
        );
        assert!(
            primary.0.try_wait()?.is_none(),
            "the tray process must remain alive"
        );
        thread::sleep(Duration::from_millis(25));
    }

    assert!(fixture.path().join("settings.json").is_file());
    assert!(fixture.startup().is_some());
    launch_duplicate(
        &fixture,
        &["--startup"],
        "duplicate startup must exit successfully",
    );
    assert!(
        visible_window(process).is_null(),
        "duplicate startup must stay quiet"
    );

    launch_duplicate(
        &fixture,
        &[],
        "explicit second launch must exit successfully",
    );
    wait_for(
        || !visible_window(process).is_null(),
        "the tray process must open Welcome",
    );
    let window = visible_window(process);
    assert!(window_size::client_size(window, 440, 560).is_some());
    // SAFETY: window was obtained from EnumWindows.
    unsafe {
        let resize_buttons = (WS_THICKFRAME | WS_MINIMIZEBOX | WS_MAXIMIZEBOX) as isize;
        assert_eq!(GetWindowLongPtrW(window, GWL_STYLE) & resize_buttons, 0);
        assert_ne!(ShowWindowAsync(window, SW_MINIMIZE), 0);
    }
    wait_for(
        // SAFETY: the primary process owns this HWND for the duration of the test.
        || unsafe { IsIconic(window) != 0 },
        "Windows must minimize Welcome",
    );
    launch_duplicate(
        &fixture,
        &[],
        "launch while minimized must exit successfully",
    );
    wait_for(
        // SAFETY: the primary process still owns the same Welcome HWND.
        || unsafe { IsIconic(window) == 0 && IsWindowVisible(window) != 0 },
        "explicit launch must restore the same minimized Welcome window",
    );
    // SAFETY: Windows delivers the close request to the primary process's window.
    unsafe {
        assert_ne!(PostMessageW(window, WM_CLOSE, 0, 0), 0);
    }
    wait_for(
        || visible_window(process).is_null(),
        "close must hide the window",
    );
    assert!(
        primary.0.try_wait()?.is_none(),
        "close must leave the tray process alive"
    );

    launch_duplicate(&fixture, &[], "relaunch must exit successfully");
    wait_for(
        || !visible_window(process).is_null(),
        "relaunch must reopen Welcome",
    );
    drop(primary);
    fixture.verify_system_settings();
    Ok(())
}
