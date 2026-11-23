use windows_sys::Win32::{
    Foundation::HWND,
    System::Threading::{AttachThreadInput, GetCurrentThreadId},
    UI::{
        Input::KeyboardAndMouse::{
            INPUT, INPUT_KEYBOARD, KEYBDINPUT, KEYEVENTF_EXTENDEDKEY, KEYEVENTF_KEYUP, SendInput,
            VK_NEXT,
        },
        WindowsAndMessaging::{GetForegroundWindow, GetWindowThreadProcessId, SetForegroundWindow},
    },
};

pub(super) fn foreground() -> HWND {
    // SAFETY: Windows returns the active HWND without modifying it.
    unsafe { GetForegroundWindow() }
}

pub(super) fn activate(window: HWND) {
    // SAFETY: attach only for the foreground transition, then immediately detach.
    unsafe {
        if foreground() != window && SetForegroundWindow(window) == 0 {
            let current = GetCurrentThreadId();
            let active = GetWindowThreadProcessId(foreground(), std::ptr::null_mut());
            let attached =
                active != 0 && active != current && AttachThreadInput(current, active, 1) != 0;
            SetForegroundWindow(window);
            if attached {
                AttachThreadInput(current, active, 0);
            }
        }
    }
    super::support::wait_for(
        || foreground() == window,
        "fixture did not get foreground focus",
    );
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
