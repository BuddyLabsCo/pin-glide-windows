use std::io;
use std::process::Command;
use std::ptr::null;
use std::sync::Mutex;
use std::sync::mpsc::channel;
use std::thread;
use std::time::Duration;

use slint::winit_030::winit::event_loop::EventLoop;
use slint::winit_030::winit::platform::windows::EventLoopBuilderExtWindows;
use windows_sys::Win32::Foundation::{HWND, LPARAM, RECT};
use windows_sys::Win32::System::Threading::GetCurrentThreadId;
use windows_sys::Win32::UI::Input::KeyboardAndMouse::{VK_DOWN, VK_RETURN};
use windows_sys::Win32::UI::Shell::{NOTIFYICONIDENTIFIER, Shell_NotifyIconGetRect};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    EnumThreadWindows, FindWindowW, GetWindowRect, GetWindowThreadProcessId, IsWindowVisible,
    PostMessageW, PostThreadMessageW, WM_APP, WM_CANCELMODE, WM_KEYDOWN, WM_QUIT, WM_RBUTTONUP,
};

use super::{platform, run};

#[path = "../tests/support/mod.rs"]
mod support;
use support::wait_for;

const TIMEOUT: Duration = Duration::from_secs(5);
const DUPLICATE: &str = "PIN_GLIDE_TEST_DUPLICATE";
static DESKTOP: Mutex<()> = Mutex::new(());

#[test]
#[ignore = "requires an interactive Windows desktop with Pin Glide stopped"]
fn failed_automatic_startup_keeps_the_icon_and_can_exit_through_its_menu() {
    let (attempted, attempts) = channel();
    exercise_startup(
        move |tray, owner| {
            attempts
                .recv_timeout(TIMEOUT)
                .expect("backend initialization was not attempted");
            wait_for(
                || icon_registered(tray),
                "Explorer did not register the tray icon",
            );
            assert!(
                !has_visible_window(owner),
                "automatic launch must stay hidden"
            );
            duplicate_launch();
            attempts
                .recv_timeout(TIMEOUT)
                .expect("explicit launch did not retry initialization");
        },
        |instance| {
            run(
                instance,
                false,
                || {
                    attempted.send(()).unwrap();
                    Err(slint::PlatformError::Other(
                        "simulated backend failure".into(),
                    ))
                },
                platform::Instance::listen,
            )
        },
    );
}

#[test]
#[ignore = "requires an interactive Windows desktop with Pin Glide stopped"]
fn listener_failure_still_lets_an_explicit_launch_open_the_original_welcome() {
    if std::env::var_os(DUPLICATE).is_some() {
        assert!(platform::Instance::claim(true).unwrap().is_none());
        return;
    }
    let (attempted, attempts) = channel();
    exercise_startup(
        move |_, owner| {
            attempts
                .recv_timeout(TIMEOUT)
                .expect("listener failure was not injected");
            assert!(
                !has_visible_window(owner),
                "automatic launch must stay hidden"
            );
            duplicate_launch();
            wait_for(
                || has_visible_window(owner),
                "the original instance did not open Welcome",
            );
        },
        |instance| {
            run(instance, false, select_test_backend, |_, _| {
                attempted.send(()).unwrap();
                Err(io::Error::other("simulated listener failure"))
            })
        },
    );
}

fn exercise_startup(
    verify: impl FnOnce(HWND, u32) + Send + 'static,
    start: impl FnOnce(platform::Instance) -> Result<(), Box<dyn std::error::Error>>,
) {
    let _desktop = DESKTOP.lock().unwrap();
    let instance = platform::Instance::claim(false).unwrap().unwrap();
    let window_name = instance.window_name().to_vec();
    // SAFETY: this query returns the ID of the thread that will pump the tray queue.
    let owner = unsafe { GetCurrentThreadId() };
    let (stopped, shutdown) = channel();
    let observer = thread::spawn(move || {
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let tray = find_tray(&window_name);
            verify(tray, owner);
            exit_from_menu(tray, owner);
            shutdown
                .recv_timeout(TIMEOUT)
                .expect("tray Exit did not stop the application");
        }));
        if let Err(error) = result {
            let _ = slint::quit_event_loop();
            // SAFETY: the name is terminated and the owner thread pumps these messages.
            unsafe {
                let tray = FindWindowW(null(), window_name.as_ptr());
                if !tray.is_null() {
                    PostMessageW(tray, WM_CANCELMODE, 0, 0);
                }
                PostThreadMessageW(owner, WM_QUIT, 0, 0);
            }
            std::panic::resume_unwind(error);
        }
    });
    let result = start(instance);
    let _ = stopped.send(());
    observer.join().unwrap();
    result.unwrap();
}

pub(crate) fn select_test_backend() -> Result<(), slint::PlatformError> {
    let mut builder = EventLoop::with_user_event();
    builder.with_any_thread(true);
    slint::BackendSelector::new()
        .backend_name("winit".into())
        .renderer_name("software".into())
        .with_winit_event_loop_builder(builder)
        .select()
}

fn duplicate_launch() {
    let status = Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "startup_tests::listener_failure_still_lets_an_explicit_launch_open_the_original_welcome",
            "--ignored",
        ])
        .env(DUPLICATE, "1")
        .status()
        .unwrap();
    assert!(
        status.success(),
        "the explicit duplicate must succeed: {status}"
    );
}

fn find_tray(name: &[u16]) -> HWND {
    let mut tray = std::ptr::null_mut();
    wait_for(
        || {
            // SAFETY: the instance name is NUL-terminated.
            tray = unsafe { FindWindowW(null(), name.as_ptr()) };
            !tray.is_null()
        },
        "the native tray window must exist",
    );
    tray
}

fn icon_registered(tray: HWND) -> bool {
    let icon = NOTIFYICONIDENTIFIER {
        cbSize: size_of::<NOTIFYICONIDENTIFIER>() as u32,
        hWnd: tray,
        uID: 1,
        ..Default::default()
    };
    let mut bounds = RECT::default();
    // SAFETY: the identifier and writable rectangle live for the duration of the call.
    unsafe { Shell_NotifyIconGetRect(&icon, &raw mut bounds) == 0 }
}

fn has_visible_window(owner: u32) -> bool {
    unsafe extern "system" fn visit(window: HWND, found: LPARAM) -> i32 {
        let mut bounds = RECT::default();
        // SAFETY: the callback receives a live HWND and a pointer to the caller's flag.
        unsafe {
            if IsWindowVisible(window) != 0
                && GetWindowRect(window, &raw mut bounds) != 0
                && bounds.right - bounds.left > 100
            {
                *(found as *mut bool) = true;
                return 0;
            }
        }
        1
    }
    let mut found = false;
    // SAFETY: enumeration is synchronous and found stays live during the call.
    unsafe { EnumThreadWindows(owner, Some(visit), &raw mut found as LPARAM) };
    found
}

fn exit_from_menu(tray: HWND, owner: u32) {
    // SAFETY: tray is the live notification callback window.
    unsafe {
        assert_ne!(PostMessageW(tray, WM_APP + 1, 1, WM_RBUTTONUP as LPARAM), 0);
    }
    let menu_class = platform::wide("#32768");
    let mut popup = std::ptr::null_mut();
    wait_for(
        // SAFETY: the class name is terminated and the handle is checked before use.
        || unsafe {
            let menu = FindWindowW(menu_class.as_ptr(), null());
            if menu.is_null() || GetWindowThreadProcessId(menu, std::ptr::null_mut()) != owner {
                return false;
            }
            popup = menu;
            true
        },
        "the native tray context menu did not open",
    );
    // SAFETY: the live popup processes these keyboard messages to select Exit.
    unsafe {
        for key in [VK_DOWN, VK_DOWN, VK_DOWN, VK_RETURN] {
            assert_ne!(PostMessageW(popup, WM_KEYDOWN, key.into(), 0), 0);
        }
    }
}
