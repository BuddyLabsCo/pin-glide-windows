use std::{cell::RefCell, io, ptr::null_mut};

use windows_sys::Win32::{
    Foundation::{ERROR_HOTKEY_ALREADY_REGISTERED, HWND, LPARAM, LRESULT, WPARAM},
    System::LibraryLoader::GetModuleHandleW,
    UI::{
        Input::KeyboardAndMouse::{
            GetAsyncKeyState, MOD_ALT, MOD_CONTROL, MOD_NOREPEAT, MOD_SHIFT, MOD_WIN,
            RegisterHotKey, UnregisterHotKey, VK_CONTROL, VK_ESCAPE, VK_F1, VK_F12, VK_F24,
            VK_LCONTROL, VK_LMENU, VK_LSHIFT, VK_LWIN, VK_MENU, VK_RCONTROL, VK_RETURN, VK_RMENU,
            VK_RSHIFT, VK_RWIN, VK_SHIFT,
        },
        Shell::{DefSubclassProc, RemoveWindowSubclass, SetWindowSubclass},
        WindowsAndMessaging::{
            CallNextHookEx, GetForegroundWindow, HC_ACTION, HHOOK, KBDLLHOOKSTRUCT, PostMessageW,
            SetWindowsHookExW, UnhookWindowsHookEx, WH_KEYBOARD_LL, WM_APP, WM_KEYDOWN, WM_KEYUP,
            WM_SYSKEYDOWN, WM_SYSKEYUP,
        },
    },
};

use crate::storage::settings::Shortcut;

pub(super) const CAPTURE_FINISHED: u32 = WM_APP + 3;
const CAPTURE_KEY: u32 = WM_APP + 4;
const MODIFIERS: u32 = MOD_CONTROL | MOD_ALT | MOD_SHIFT;

#[derive(Clone, Copy)]
pub(crate) enum ShortcutError {
    Invalid,
    Conflict,
    Register,
    Save,
}

impl Shortcut {
    fn valid(self) -> bool {
        self.modifiers & !MODIFIERS == 0
            && self.modifiers & (MOD_CONTROL | MOD_ALT) != 0
            && (matches!(self.key, 0x30..=0x39 | 0x41..=0x5a)
                || ((u32::from(VK_F1)..=u32::from(VK_F24)).contains(&self.key)
                    && self.key != u32::from(VK_F12)))
    }

    pub(crate) fn label(self) -> String {
        let mut label = String::new();
        for (modifier, name) in [
            (MOD_CONTROL, "Ctrl"),
            (MOD_ALT, "Alt"),
            (MOD_SHIFT, "Shift"),
        ] {
            if self.modifiers & modifier != 0 {
                label.push_str(name);
                label.push_str(" + ");
            }
        }
        if self.key >= u32::from(VK_F1) {
            label.push_str(&format!("F{}", self.key - u32::from(VK_F1) + 1));
        } else if let Some(key) = char::from_u32(self.key) {
            label.push(key);
        }
        label
    }
}

struct Registration {
    owner: HWND,
    id: i32,
    shortcut: Shortcut,
}

impl Registration {
    fn new(owner: HWND, id: i32, shortcut: Shortcut) -> Result<Self, ShortcutError> {
        if !shortcut.valid() {
            return Err(ShortcutError::Invalid);
        }
        // SAFETY: the live tray window belongs to this thread and the chord was validated.
        let success =
            unsafe { RegisterHotKey(owner, id, shortcut.modifiers | MOD_NOREPEAT, shortcut.key) };
        if success == 0 {
            return Err(
                if io::Error::last_os_error().raw_os_error()
                    == Some(ERROR_HOTKEY_ALREADY_REGISTERED as i32)
                {
                    ShortcutError::Conflict
                } else {
                    ShortcutError::Register
                },
            );
        }
        Ok(Self {
            owner,
            id,
            shortcut,
        })
    }
}

impl Drop for Registration {
    fn drop(&mut self) {
        // SAFETY: this thread owns the registration and its HWND is still live.
        unsafe { UnregisterHotKey(self.owner, self.id) };
    }
}

struct Capture {
    window: HWND,
    tray: HWND,
    hook: HHOOK,
    modifiers: u32,
    draft: Option<Shortcut>,
    finished: bool,
}

impl Drop for Capture {
    fn drop(&mut self) {
        // SAFETY: this object owns both registrations on the current thread.
        unsafe {
            UnhookWindowsHookEx(self.hook);
            RemoveWindowSubclass(self.window, Some(capture_window_proc), 0);
        }
    }
}

thread_local! {
    static REGISTRATION: RefCell<Option<Registration>> = const { RefCell::new(None) };
    static CAPTURE: RefCell<Option<Capture>> = const { RefCell::new(None) };
}

pub(crate) fn effective() -> Option<Shortcut> {
    REGISTRATION.with_borrow(|current| current.as_ref().map(|entry| entry.shortcut))
}

pub(super) fn replace(
    owner: HWND,
    shortcut: Option<Shortcut>,
    save: impl FnOnce() -> io::Result<()>,
) -> Result<(), ShortcutError> {
    if owner.is_null() {
        return Err(ShortcutError::Register);
    }
    if effective() == shortcut {
        return save().map_err(|_| ShortcutError::Save);
    }
    let id = REGISTRATION.with_borrow(|current| {
        if current.as_ref().is_some_and(|entry| entry.id == 1) {
            2
        } else {
            1
        }
    });
    let next = shortcut
        .map(|chord| Registration::new(owner, id, chord))
        .transpose()?;
    save().map_err(|_| ShortcutError::Save)?;
    drop(REGISTRATION.replace(next));
    Ok(())
}

pub(super) fn triggered(id: WPARAM, chord: LPARAM) -> bool {
    REGISTRATION.with_borrow(|current| {
        current.as_ref().is_some_and(|entry| {
            id == entry.id as usize
                && (chord as u32 & 0xffff & !MOD_NOREPEAT) == entry.shortcut.modifiers
                && (chord as u32 >> 16) == entry.shortcut.key
        })
    })
}

pub(super) fn start_capture(window: HWND, tray: HWND) -> Result<(), ShortcutError> {
    finish_capture();
    if tray.is_null() {
        return Err(ShortcutError::Register);
    }
    // SAFETY: the capture callback lives in this module and runs on the tray thread.
    let hook = unsafe {
        SetWindowsHookExW(
            WH_KEYBOARD_LL,
            Some(capture_key),
            GetModuleHandleW(null_mut()),
            0,
        )
    };
    if hook.is_null() {
        return Err(ShortcutError::Register);
    }
    let capture = Capture {
        window,
        tray,
        hook,
        modifiers: modifiers(),
        draft: None,
        finished: false,
    };
    // SAFETY: the Settings HWND is on the tray thread and the subclass borrows no data.
    if unsafe { SetWindowSubclass(window, Some(capture_window_proc), 0, 0) } == 0 {
        return Err(ShortcutError::Register);
    }
    CAPTURE.replace(Some(capture));
    Ok(())
}

pub(crate) fn finish_capture() -> Option<Shortcut> {
    CAPTURE.take().and_then(|capture| capture.draft)
}

pub(super) fn close() {
    finish_capture();
    drop(REGISTRATION.take());
}

unsafe extern "system" fn capture_key(code: i32, message: WPARAM, data: LPARAM) -> LRESULT {
    if code == HC_ACTION as i32 {
        let window = CAPTURE.with_borrow(|capture| capture.as_ref().map(|entry| entry.window));
        // SAFETY: HC_ACTION supplies a live keyboard event. Only the active Settings consumes it.
        unsafe {
            if let Some(window) = window.filter(|&window| GetForegroundWindow() == window) {
                let key = (*(data as *const KBDLLHOOKSTRUCT)).vkCode as u16;
                if modifier_for(key) == 0
                    && (matches!(message as u32, WM_KEYUP | WM_SYSKEYUP)
                        || PostMessageW(window, CAPTURE_KEY, key.into(), modifiers() as LPARAM)
                            != 0)
                {
                    return 1;
                }
            }
        }
    }
    // SAFETY: forward the original hook arguments to the next handler.
    unsafe { CallNextHookEx(null_mut(), code, message, data) }
}

unsafe extern "system" fn capture_window_proc(
    window: HWND,
    message: u32,
    wparam: WPARAM,
    lparam: LPARAM,
    _: usize,
    _: usize,
) -> LRESULT {
    if matches!(
        message,
        CAPTURE_KEY | WM_KEYDOWN | WM_KEYUP | WM_SYSKEYDOWN | WM_SYSKEYUP
    ) {
        let finish = CAPTURE.with_borrow_mut(|capture| {
            let capture = capture.as_mut()?;
            if message == CAPTURE_KEY {
                capture.modifiers = lparam as u32;
            }
            capture.key(
                wparam as u16,
                matches!(message, CAPTURE_KEY | WM_KEYDOWN | WM_SYSKEYDOWN),
            )
        });
        if let Some((tray, apply)) = finish {
            // SAFETY: the capture state names the live tray window on this thread.
            unsafe { PostMessageW(tray, CAPTURE_FINISHED, usize::from(apply), 0) };
        }
        return 0;
    }
    // SAFETY: pass the unchanged message to the next window procedure.
    unsafe { DefSubclassProc(window, message, wparam, lparam) }
}

impl Capture {
    fn key(&mut self, key: u16, down: bool) -> Option<(HWND, bool)> {
        if self.finished {
            return None;
        }
        let modifier = modifier_for(key);
        if modifier != 0 {
            self.modifiers = (self.modifiers & !modifier) | if down { modifier } else { 0 };
        } else if down && matches!(key, VK_RETURN | VK_ESCAPE) {
            self.finished = true;
            return Some((self.tray, key == VK_RETURN));
        } else if down {
            self.draft = Some(Shortcut {
                key: key.into(),
                modifiers: self.modifiers,
            });
        }
        None
    }
}

fn modifier_for(key: u16) -> u32 {
    match key {
        VK_CONTROL | VK_LCONTROL | VK_RCONTROL => MOD_CONTROL,
        VK_MENU | VK_LMENU | VK_RMENU => MOD_ALT,
        VK_SHIFT | VK_LSHIFT | VK_RSHIFT => MOD_SHIFT,
        VK_LWIN | VK_RWIN => MOD_WIN,
        _ => 0,
    }
}

fn modifiers() -> u32 {
    let down = |key: u16| {
        // SAFETY: the key state query has no side effects. Only modifier keys are queried.
        unsafe { GetAsyncKeyState(i32::from(key)) < 0 }
    };
    (u32::from(down(VK_CONTROL)) * MOD_CONTROL)
        | (u32::from(down(VK_MENU)) * MOD_ALT)
        | (u32::from(down(VK_SHIFT)) * MOD_SHIFT)
        | (u32::from(down(VK_LWIN) || down(VK_RWIN)) * MOD_WIN)
}
