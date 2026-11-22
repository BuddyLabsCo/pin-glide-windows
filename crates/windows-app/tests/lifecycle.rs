#![cfg(windows)]

use std::io;
use std::process::{Child, Command};
use std::thread;
use std::time::{Duration, Instant};

use windows_sys::Win32::Foundation::{HWND, LPARAM, RECT};
use windows_sys::Win32::UI::HiDpi::{
    DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2, GetDpiForWindow, SetThreadDpiAwarenessContext,
};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    EnumWindows, GWL_STYLE, GetClientRect, GetWindowLongPtrW, GetWindowThreadProcessId,
    IsWindowVisible, PostMessageW, WM_CLOSE, WS_MAXIMIZEBOX, WS_MINIMIZEBOX, WS_THICKFRAME,
};

struct AppProcess(Child);

impl AppProcess {
    fn launch(arguments: &[&str]) -> Self {
        Self(
            Command::new(env!("CARGO_BIN_EXE_windows-app"))
                .args(arguments)
                .spawn()
                .unwrap(),
        )
    }

    fn id(&self) -> u32 {
        self.0.id()
    }
}

impl Drop for AppProcess {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
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

fn wait_for(mut condition: impl FnMut() -> bool, expectation: &str) {
    let deadline = Instant::now() + Duration::from_secs(10);
    while !condition() {
        assert!(Instant::now() < deadline, "{expectation}");
        thread::sleep(Duration::from_millis(25));
    }
}

#[test]
#[ignore = "requires an interactive Windows desktop with Pin Glide stopped"]
fn startup_stays_hidden_and_explicit_launch_reopens_the_same_process() -> io::Result<()> {
    // SAFETY: use physical coordinates when querying Welcome from this test thread.
    let previous =
        unsafe { SetThreadDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2) };
    assert!(!previous.is_null());
    let mut primary = AppProcess::launch(&["--startup"]);
    let observe_until = Instant::now() + Duration::from_secs(1);
    while Instant::now() < observe_until {
        assert!(
            visible_window(primary.id()).is_null(),
            "startup must not open a window"
        );
        assert!(
            primary.0.try_wait()?.is_none(),
            "the tray process must remain alive"
        );
        thread::sleep(Duration::from_millis(25));
    }

    let mut automatic = AppProcess::launch(&["--startup"]);
    wait_for(
        || automatic.0.try_wait().unwrap().is_some(),
        "duplicate startup must exit",
    );
    assert!(
        visible_window(primary.id()).is_null(),
        "duplicate startup must stay quiet"
    );

    let mut explicit = AppProcess::launch(&[]);
    wait_for(
        || explicit.0.try_wait().unwrap().is_some(),
        "the explicit second launch must exit",
    );
    wait_for(
        || !visible_window(primary.id()).is_null(),
        "the tray process must open Welcome",
    );
    let window = visible_window(primary.id());
    // SAFETY: window was obtained from EnumWindows and bounds is a writable output.
    unsafe {
        let mut bounds = RECT::default();
        assert_ne!(GetClientRect(window, &raw mut bounds), 0);
        let dpi = GetDpiForWindow(window) as i32;
        assert_eq!(
            (bounds.right, bounds.bottom),
            (440 * dpi / 96, 560 * dpi / 96)
        );
        let resize_buttons = (WS_THICKFRAME | WS_MINIMIZEBOX | WS_MAXIMIZEBOX) as isize;
        assert_eq!(GetWindowLongPtrW(window, GWL_STYLE) & resize_buttons, 0);
        assert_ne!(PostMessageW(window, WM_CLOSE, 0, 0), 0);
    }
    wait_for(
        || visible_window(primary.id()).is_null(),
        "close must hide the window",
    );
    assert!(
        primary.0.try_wait()?.is_none(),
        "close must leave the tray process alive"
    );

    let mut reopening = AppProcess::launch(&[]);
    wait_for(
        || reopening.0.try_wait().unwrap().is_some(),
        "the relaunch must exit",
    );
    wait_for(
        || !visible_window(primary.id()).is_null(),
        "relaunch must reopen Welcome",
    );
    Ok(())
}
