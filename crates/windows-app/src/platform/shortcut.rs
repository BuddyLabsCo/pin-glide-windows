use std::{cell::RefCell, io, ptr::null_mut};

use windows_sys::Win32::{
    Foundation::{ERROR_HOTKEY_ALREADY_REGISTERED, HWND, LPARAM, LRESULT, WPARAM},
    System::LibraryLoader::GetModuleHandleW,
    UI::{
        Input::KeyboardAndMouse::{
            GetAsyncKeyState, GetKeyNameTextW, MAPVK_VK_TO_VSC_EX, MOD_ALT, MOD_CONTROL,
            MOD_NOREPEAT, MOD_SHIFT, MOD_WIN, MapVirtualKeyW, RegisterHotKey, UnregisterHotKey,
            VK_CONTROL, VK_ESCAPE, VK_F1, VK_F12, VK_F24, VK_LCONTROL, VK_LMENU, VK_LSHIFT,
            VK_LWIN, VK_MENU, VK_RCONTROL, VK_RETURN, VK_RMENU, VK_RSHIFT, VK_RWIN, VK_SHIFT,
        },
        WindowsAndMessaging::{
            CallNextHookEx, GetForegroundWindow, HC_ACTION, HHOOK, KBDLLHOOKSTRUCT, PostMessageW,
            SetWindowsHookExW, UnhookWindowsHookEx, WH_KEYBOARD_LL, WM_APP, WM_KEYUP, WM_SYSKEYUP,
        },
    },
};

use crate::storage::settings::Shortcut;

pub(super) const CAPTURE_FINISHED: u32 = WM_APP + 3;
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
            && self.modifiers != 0
            && (0x08..=0xfe).contains(&self.key)
            && modifier_for(self.key as u16) == 0
            && self.key != u32::from(VK_F12)
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
        if (u32::from(VK_F1)..=u32::from(VK_F24)).contains(&self.key) {
            label.push_str(&format!("F{}", self.key - u32::from(VK_F1) + 1));
        } else if matches!(self.key, 0x30..=0x39 | 0x41..=0x5a) {
            label.push(char::from(self.key as u8));
        } else {
            let mut name = [0_u16; 64];
            // SAFETY: the key is scalar and the name buffer has the stated capacity.
            let len = unsafe {
                let scan = MapVirtualKeyW(self.key, MAPVK_VK_TO_VSC_EX);
                let extended = u32::from(scan > 0xff);
                GetKeyNameTextW(
                    ((scan & 0xff) << 16 | extended << 24) as i32,
                    name.as_mut_ptr(),
                    name.len() as i32,
                )
            };
            if len > 0 {
                label.push_str(&String::from_utf16_lossy(&name[..len as usize]));
            } else {
                label.push_str(&format!("VK {:02X}", self.key));
            }
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
    draft: Option<Shortcut>,
    finished: bool,
}

impl Drop for Capture {
    fn drop(&mut self) {
        // SAFETY: this object owns the live hook on the current thread.
        unsafe { UnhookWindowsHookEx(self.hook) };
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
    CAPTURE.replace(Some(Capture {
        window,
        tray,
        hook,
        draft: None,
        finished: false,
    }));
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
            if window.is_some_and(|window| GetForegroundWindow() == window) {
                let key = (*(data as *const KBDLLHOOKSTRUCT)).vkCode as u16;
                if modifier_for(key) == 0 {
                    if !matches!(message as u32, WM_KEYUP | WM_SYSKEYUP) {
                        let held = modifiers();
                        let finish =
                            CAPTURE.with_borrow_mut(|capture| capture.as_mut()?.key(key, held));
                        if let Some((tray, apply)) = finish {
                            PostMessageW(tray, CAPTURE_FINISHED, usize::from(apply), 0);
                        }
                    }
                    return 1;
                }
            }
        }
    }
    // SAFETY: forward the original hook arguments to the next handler.
    unsafe { CallNextHookEx(null_mut(), code, message, data) }
}

impl Capture {
    fn key(&mut self, key: u16, held: u32) -> Option<(HWND, bool)> {
        if self.finished {
            return None;
        }
        if held == 0 && matches!(key, VK_RETURN | VK_ESCAPE) {
            self.finished = true;
            return Some((self.tray, key == VK_RETURN));
        }
        self.draft = Some(Shortcut {
            key: key.into(),
            modifiers: held,
        });
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

#[cfg(test)]
mod tests {
    use super::*;
    use windows_sys::Win32::UI::{
        Input::KeyboardAndMouse::{VK_F22, VK_F23, VK_NEXT},
        WindowsAndMessaging::{CreateWindowExW, DestroyWindow},
    };

    #[test]
    fn allows_windows_navigation_keys_and_excludes_incomplete_and_reserved_chords() {
        let page_down = Shortcut {
            key: VK_NEXT.into(),
            modifiers: MOD_CONTROL,
        };
        assert!(page_down.valid());
        assert!(page_down.label().starts_with("Ctrl + "));
        assert!(!page_down.label().ends_with("VK 22"));
        for invalid in [
            Shortcut {
                modifiers: 0,
                ..page_down
            },
            Shortcut {
                modifiers: MOD_WIN,
                ..page_down
            },
            Shortcut {
                key: VK_CONTROL.into(),
                ..page_down
            },
            Shortcut {
                key: VK_F12.into(),
                ..page_down
            },
        ] {
            assert!(!invalid.valid());
        }
    }

    #[test]
    fn failed_save_keeps_the_previous_windows_registration() {
        // SAFETY: the built-in class name is terminated and the hidden window is owned by this thread.
        let owner = unsafe {
            CreateWindowExW(
                0,
                windows_sys::w!("STATIC"),
                std::ptr::null(),
                0,
                0,
                0,
                0,
                0,
                null_mut(),
                null_mut(),
                null_mut(),
                std::ptr::null(),
            )
        };
        assert!(!owner.is_null());
        let old = Shortcut {
            modifiers: MOD_CONTROL | MOD_ALT | MOD_SHIFT,
            key: VK_F23.into(),
        };
        let candidate = Shortcut {
            key: VK_F22.into(),
            ..old
        };
        let outcome = (|| {
            replace(owner, Some(old), || Ok(()))?;
            let rejected = replace(owner, Some(candidate), || {
                Err(io::ErrorKind::PermissionDenied.into())
            });
            Ok::<_, ShortcutError>((
                rejected,
                Registration::new(owner, 3, old).err(),
                Registration::new(owner, 3, candidate).is_ok(),
            ))
        })();
        close();
        // SAFETY: the test created this window on the current thread and has released the chords.
        unsafe { DestroyWindow(owner) };
        assert!(matches!(
            outcome,
            Ok((
                Err(ShortcutError::Save),
                Some(ShortcutError::Conflict),
                true
            ))
        ));
    }
}
