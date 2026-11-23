mod clipboard;
mod instance;
mod popup;
mod session;
mod tray;

pub(crate) use clipboard::write_unicode_clipboard;
pub(crate) use clipboard::{ClipboardError, read_unicode_clipboard};
pub use instance::{ActivationListener, Instance};
pub(crate) use popup::{new_popup, place_popup, popup_attributes, prepare_popup};
pub(crate) use session::popup_epoch;
pub use tray::Tray;
pub(crate) use tray::{delivery_status, set_delivery_status};

use std::{cell::Cell, io};

use slint::{
    Window,
    winit_030::{WinitWindowAccessor, winit::window::WindowButtons},
};
use windows_sys::Win32::Globalization::GetUserDefaultUILanguage;
use windows_sys::Win32::UI::HiDpi::{
    DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2, SetProcessDpiAwarenessContext,
};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    SPI_GETCLIENTAREAANIMATION, SystemParametersInfoW,
};

thread_local! {
    static ANIMATION_PREFERENCE_OVERRIDE: Cell<Option<bool>> = const { Cell::new(None) };
}

pub fn set_animation_preference_override(enabled: Option<bool>) {
    ANIMATION_PREFERENCE_OVERRIDE.set(enabled);
}

pub fn enable_per_monitor_dpi() -> io::Result<()> {
    // SAFETY: called before either Win32 or Slint creates a window.
    if unsafe { SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2) } == 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(())
}

pub fn animations_enabled() -> bool {
    if let Some(enabled) = ANIMATION_PREFERENCE_OVERRIDE.get() {
        return enabled;
    }
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

pub(crate) fn configure_main_chrome(window: &Window) {
    window.with_winit_window(|native| {
        native.set_enabled_buttons(WindowButtons::MINIMIZE | WindowButtons::CLOSE);
    });
}

fn win32_result(result: i32) -> io::Result<()> {
    if result == 0 {
        Err(io::Error::last_os_error())
    } else {
        Ok(())
    }
}
