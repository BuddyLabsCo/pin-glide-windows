mod clipboard;
mod instance;
mod tray;

pub(crate) use clipboard::unicode_clipboard_is_valid;
pub use instance::{ActivationListener, Instance};
pub use tray::Tray;

use std::io;

use windows_sys::Win32::Globalization::GetUserDefaultUILanguage;
use windows_sys::Win32::UI::HiDpi::{
    DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2, SetProcessDpiAwarenessContext,
};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    SPI_GETCLIENTAREAANIMATION, SystemParametersInfoW,
};

pub fn enable_per_monitor_dpi() -> io::Result<()> {
    // SAFETY: called before either Win32 or Slint creates a window.
    if unsafe { SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2) } == 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(())
}

pub fn animations_enabled() -> bool {
    let mut enabled = 0i32;
    // SAFETY: SPI_GETCLIENTAREAANIMATION writes a BOOL to the supplied pointer.
    let result = unsafe {
        SystemParametersInfoW(SPI_GETCLIENTAREAANIMATION, 0, (&raw mut enabled).cast(), 0)
    };
    result != 0 && enabled != 0
}

pub fn chinese() -> bool {
    // SAFETY: this query requires no input and has no side effects.
    unsafe { GetUserDefaultUILanguage() & 0x03ff == 0x04 }
}

pub fn wide(value: &str) -> Vec<u16> {
    value.encode_utf16().chain([0]).collect()
}

fn win32_result(result: i32) -> io::Result<()> {
    if result == 0 {
        Err(io::Error::last_os_error())
    } else {
        Ok(())
    }
}
