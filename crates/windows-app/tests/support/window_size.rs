use windows_sys::Win32::{
    Foundation::{HWND, RECT},
    UI::{HiDpi::GetDpiForWindow, WindowsAndMessaging::GetClientRect},
};

pub fn client_size(window: HWND, width: i32, height: i32) -> Option<(i32, i32)> {
    let mut rect = RECT::default();
    // SAFETY: callers obtain a live HWND from Windows, and rect is writable.
    let dpi = unsafe {
        if GetClientRect(window, &raw mut rect) == 0 {
            return None;
        }
        GetDpiForWindow(window) as i32
    };
    let physical = ((width * dpi + 48) / 96, (height * dpi + 48) / 96);
    (dpi != 0 && (rect.right, rect.bottom) == physical).then_some(physical)
}
