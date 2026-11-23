use windows_sys::Win32::{
    Foundation::{HWND, LPARAM, LRESULT, POINT, RECT, WPARAM},
    Graphics::Gdi::ClientToScreen,
    System::Threading::{AttachThreadInput, GetCurrentThreadId},
    UI::{
        HiDpi::GetDpiForWindow,
        Input::KeyboardAndMouse::{
            INPUT, INPUT_KEYBOARD, INPUT_MOUSE, KEYBDINPUT, KEYEVENTF_EXTENDEDKEY, KEYEVENTF_KEYUP,
            MOUSEEVENTF_LEFTDOWN, MOUSEEVENTF_LEFTUP, MOUSEINPUT, SendInput, SetActiveWindow,
            VK_NEXT,
        },
        WindowsAndMessaging::{
            CallNextHookEx, GA_ROOT, GetAncestor, GetClientRect, GetCursorPos, GetForegroundWindow,
            GetWindowThreadProcessId, HC_ACTION, HHOOK, HWND_NOTOPMOST, HWND_TOPMOST,
            KBDLLHOOKSTRUCT, MSG, PM_NOREMOVE, PeekMessageW, SW_RESTORE, SWP_NOACTIVATE,
            SWP_NOMOVE, SWP_NOSIZE, SetCursorPos, SetForegroundWindow, SetWindowPos,
            SetWindowsHookExW, ShowWindow, UnhookWindowsHookEx, WH_KEYBOARD_LL, WindowFromPoint,
        },
    },
};

const FIXTURE_INPUT: usize = 0x5047_5445;

pub(super) fn foreground() -> HWND {
    // SAFETY: Windows returns the active HWND without modifying it.
    unsafe { GetForegroundWindow() }
}

pub(super) fn activate(window: HWND) {
    // SAFETY: temporarily join the fixture's input queue to request activation of its own window.
    unsafe {
        let current = GetCurrentThreadId();
        let target = GetWindowThreadProcessId(window, std::ptr::null_mut());
        if foreground() != window
            && SetForegroundWindow(window) == 0
            && AttachThreadInput(current, target, 1) != 0
        {
            ShowWindow(window, SW_RESTORE);
            SetActiveWindow(window);
            SetForegroundWindow(window);
            AttachThreadInput(current, target, 0);
        }
    }
    if foreground() != window {
        click_window(window);
    }
    super::support::wait_for(
        || foreground() == window,
        "fixture did not get foreground focus",
    );
}

fn click_window(window: HWND) {
    const FLAGS: u32 = SWP_NOACTIVATE | SWP_NOMOVE | SWP_NOSIZE;
    // SAFETY: raise only the fixture window and inject a click only if it owns the hit point.
    let sent = unsafe {
        let mut previous = POINT::default();
        let mut bounds = RECT::default();
        assert_ne!(GetCursorPos(&raw mut previous), 0);
        assert_ne!(GetClientRect(window, &raw mut bounds), 0);
        let mut point = POINT {
            x: bounds.right / 2,
            y: 12 * GetDpiForWindow(window) as i32 / 96,
        };
        assert_ne!(ClientToScreen(window, &raw mut point), 0);
        assert_ne!(SetWindowPos(window, HWND_TOPMOST, 0, 0, 0, 0, FLAGS), 0);
        let sent = if GetAncestor(WindowFromPoint(point), GA_ROOT) == window
            && SetCursorPos(point.x, point.y) != 0
        {
            let input = [MOUSEEVENTF_LEFTDOWN, MOUSEEVENTF_LEFTUP].map(|flags| {
                let mut input = INPUT {
                    r#type: INPUT_MOUSE,
                    ..Default::default()
                };
                input.Anonymous.mi = MOUSEINPUT {
                    dwFlags: flags,
                    ..Default::default()
                };
                input
            });
            let sent = SendInput(
                input.len() as u32,
                input.as_ptr(),
                size_of::<INPUT>() as i32,
            );
            std::thread::sleep(std::time::Duration::from_millis(100));
            SetCursorPos(previous.x, previous.y);
            sent
        } else {
            0
        };
        SetWindowPos(window, HWND_NOTOPMOST, 0, 0, 0, 0, FLAGS);
        sent
    };
    assert_eq!(sent, 2, "could not click the fixture window");
}

pub(super) fn press(window: HWND, keys: &[u16]) {
    assert_eq!(
        foreground(),
        window,
        "send input only to the foreground fixture"
    );
    let input: Vec<_> = keys
        .iter()
        .map(|&key| (key, 0))
        .chain(keys.iter().rev().map(|&key| (key, KEYEVENTF_KEYUP)))
        .map(|(key, flags)| {
            let mut input = INPUT {
                r#type: INPUT_KEYBOARD,
                ..Default::default()
            };
            input.Anonymous.ki = KEYBDINPUT {
                wVk: key,
                dwExtraInfo: FIXTURE_INPUT,
                dwFlags: flags
                    | if key == VK_NEXT {
                        KEYEVENTF_EXTENDEDKEY
                    } else {
                        0
                    },
                ..Default::default()
            };
            input
        })
        .collect();
    // SAFETY: the array is initialized and kept live throughout SendInput.
    let sent = unsafe {
        SendInput(
            input.len() as u32,
            input.as_ptr(),
            size_of::<INPUT>() as i32,
        )
    };
    assert_eq!(
        sent as usize,
        input.len(),
        "could not insert Windows keyboard input"
    );
}

pub(super) fn press_captured(window: HWND, keys: &[u16]) {
    use std::cell::Cell;
    thread_local! { static COUNT: Cell<usize> = const { Cell::new(0) }; }

    struct InputObserver(HHOOK);

    impl Drop for InputObserver {
        fn drop(&mut self) {
            // SAFETY: this scope owns the observer hook on the current thread.
            unsafe { UnhookWindowsHookEx(self.0) };
        }
    }

    unsafe extern "system" fn key(code: i32, message: WPARAM, data: LPARAM) -> LRESULT {
        // SAFETY: forward the unchanged event to the application's capture hook.
        let result = unsafe { CallNextHookEx(std::ptr::null_mut(), code, message, data) };
        // SAFETY: HC_ACTION supplies a live keyboard event.
        if unsafe {
            code == HC_ACTION as i32
                && (*(data as *const KBDLLHOOKSTRUCT)).dwExtraInfo == FIXTURE_INPUT
        } {
            COUNT.set(COUNT.get() + 1);
        }
        result
    }

    COUNT.set(0);
    // SAFETY: the callback stays live until the scoped observer is unhooked.
    let hook = unsafe { SetWindowsHookExW(WH_KEYBOARD_LL, Some(key), std::ptr::null_mut(), 0) };
    assert!(!hook.is_null());
    let _observer = InputObserver(hook);
    press(window, keys);
    super::support::wait_for(
        || {
            let mut message = MSG::default();
            // SAFETY: pump sent hook callbacks without dispatching unrelated posted messages.
            unsafe { PeekMessageW(&raw mut message, std::ptr::null_mut(), 0, 0, PM_NOREMOVE) };
            COUNT.get() >= keys.len() * 2
        },
        "the keyboard hook must receive the injected key presses and releases",
    );
}
