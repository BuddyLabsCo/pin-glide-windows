use std::{cell::Cell, io, ptr};

use slint::{
    ComponentHandle, PlatformError,
    winit_030::winit::{
        platform::windows::WindowAttributesExtWindows,
        raw_window_handle::{HasWindowHandle, RawWindowHandle},
        window::{Window, WindowAttributes, WindowLevel},
    },
};
use windows_sys::Win32::{
    Foundation::{HWND, LPARAM, LRESULT, SetLastError, WPARAM},
    Graphics::Gdi::{GetMonitorInfoW, MONITOR_DEFAULTTOPRIMARY, MONITORINFO, MonitorFromWindow},
    System::RemoteDesktop::{
        WTS_CURRENT_SERVER_HANDLE, WTS_CURRENT_SESSION, WTS_SESSIONSTATE_UNLOCK, WTSActive,
        WTSFreeMemory, WTSINFOEXW, WTSQuerySessionInformationW, WTSSessionInfoEx,
    },
    UI::{
        Shell::{DefSubclassProc, RemoveWindowSubclass, SetWindowSubclass},
        WindowsAndMessaging::{
            GWL_EXSTYLE, GetWindowLongPtrW, STYLESTRUCT, SWP_NOACTIVATE, SWP_NOSIZE, SWP_NOZORDER,
            SetWindowLongPtrW, SetWindowPos, WM_NCDESTROY, WM_STYLECHANGING, WS_EX_NOACTIVATE,
            WS_EX_TOOLWINDOW,
        },
    },
};

thread_local! {
    static CREATING_POPUP: Cell<bool> = const { Cell::new(false) };
}

const POPUP_STYLES: u32 = WS_EX_NOACTIVATE | WS_EX_TOOLWINDOW;

pub(crate) fn new_popup() -> Result<crate::SmsWindow, PlatformError> {
    CREATING_POPUP.set(true);
    let popup = crate::SmsWindow::new().inspect(|popup| {
        popup.window();
    });
    CREATING_POPUP.set(false);
    popup
}

pub(crate) fn popup_attributes(attributes: WindowAttributes) -> WindowAttributes {
    if CREATING_POPUP.get() {
        attributes
            .with_active(false)
            .with_decorations(false)
            .with_skip_taskbar(true)
            .with_window_level(WindowLevel::AlwaysOnTop)
    } else {
        attributes
    }
}

pub(crate) fn prepare_popup(window: &Window) -> io::Result<HWND> {
    let handle = window.window_handle().map_err(io::Error::other)?;
    let RawWindowHandle::Win32(handle) = handle.as_raw() else {
        return Err(io::ErrorKind::Unsupported.into());
    };
    let hwnd = handle.hwnd.get() as HWND;
    // SAFETY: this thread owns the live HWND and the subclass holds no Rust data.
    unsafe {
        super::win32_result(SetWindowSubclass(hwnd, Some(popup_window_proc), 0, 0))?;
        let style = GetWindowLongPtrW(hwnd, GWL_EXSTYLE);
        SetLastError(0);
        if SetWindowLongPtrW(hwnd, GWL_EXSTYLE, style | POPUP_STYLES as isize) == 0
            && io::Error::last_os_error().raw_os_error() != Some(0)
        {
            return Err(io::Error::last_os_error());
        }
    }
    Ok(hwnd)
}

unsafe extern "system" fn popup_window_proc(
    hwnd: HWND,
    message: u32,
    wparam: WPARAM,
    lparam: LPARAM,
    id: usize,
    _data: usize,
) -> LRESULT {
    // SAFETY: WM_STYLECHANGING supplies a writable STYLESTRUCT for this call.
    unsafe {
        if message == WM_STYLECHANGING && wparam as i32 == GWL_EXSTYLE {
            (*(lparam as *mut STYLESTRUCT)).styleNew |= POPUP_STYLES;
        } else if message == WM_NCDESTROY {
            RemoveWindowSubclass(hwnd, Some(popup_window_proc), id);
        }
        DefSubclassProc(hwnd, message, wparam, lparam)
    }
}

pub(crate) fn place_popup(window: &Window, hwnd: HWND) -> io::Result<()> {
    let margin = (12.0 * window.scale_factor()).round() as i32;
    let size = window.outer_size();
    let mut monitor = MONITORINFO {
        cbSize: size_of::<MONITORINFO>() as u32,
        ..Default::default()
    };
    // SAFETY: the window keeps the HWND live and monitor is writable.
    unsafe {
        let display = MonitorFromWindow(hwnd, MONITOR_DEFAULTTOPRIMARY);
        super::win32_result(GetMonitorInfoW(display, &raw mut monitor))?;
        super::win32_result(SetWindowPos(
            hwnd,
            ptr::null_mut(),
            monitor.rcWork.right - size.width as i32 - margin,
            monitor.rcWork.bottom - size.height as i32 - margin,
            0,
            0,
            SWP_NOACTIVATE | SWP_NOSIZE | SWP_NOZORDER,
        ))
    }
}

pub(crate) fn session_unlocked() -> bool {
    let mut buffer = ptr::null_mut();
    let mut bytes = 0;
    // SAFETY: WTS supplies an aligned buffer and its byte length. Level 1 selects the union field.
    unsafe {
        if WTSQuerySessionInformationW(
            WTS_CURRENT_SERVER_HANDLE,
            WTS_CURRENT_SESSION,
            WTSSessionInfoEx,
            &raw mut buffer,
            &raw mut bytes,
        ) == 0
        {
            return false;
        }
        let unlocked = !buffer.is_null() && bytes as usize >= size_of::<WTSINFOEXW>() && {
            let info = &*buffer.cast::<WTSINFOEXW>();
            info.Level == 1
                && info.Data.WTSInfoExLevel1.SessionState == WTSActive
                && info.Data.WTSInfoExLevel1.SessionFlags == WTS_SESSIONSTATE_UNLOCK as i32
        };
        WTSFreeMemory(buffer.cast());
        unlocked
    }
}
