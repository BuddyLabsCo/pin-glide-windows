use std::cell::{Cell, RefCell};
use std::io;
use std::mem::size_of;
use std::os::windows::io::{AsRawHandle, OwnedHandle};
use std::ptr::{null, null_mut};
use std::sync::{Arc, OnceLock};

use windows_sys::Win32::Foundation::{HWND, LPARAM, LRESULT, POINT, WAIT_OBJECT_0, WPARAM};
use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
use windows_sys::Win32::System::Threading::WaitForSingleObject;
use windows_sys::Win32::UI::Shell::{
    NIF_ICON, NIF_MESSAGE, NIF_TIP, NIM_ADD, NIM_DELETE, NIM_MODIFY, NOTIFYICONDATAW,
    Shell_NotifyIconW,
};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    AppendMenuW, CreateIconFromResourceEx, CreatePopupMenu, CreateWindowExW, DefWindowProcW,
    DestroyIcon, DestroyMenu, DestroyWindow, DispatchMessageW, GetCursorPos, GetMessageW, HICON,
    KillTimer, MF_SEPARATOR, MF_STRING, MSG, PBT_APMRESUMEAUTOMATIC, PBT_APMSUSPEND, PostMessageW,
    PostQuitMessage, RegisterClassW, RegisterWindowMessageW, SetForegroundWindow, SetTimer,
    TPM_NONOTIFY, TPM_RETURNCMD, TPM_RIGHTBUTTON, TrackPopupMenu, TranslateMessage,
    UnregisterClassW, WM_APP, WM_LBUTTONDBLCLK, WM_NULL, WM_POWERBROADCAST, WM_RBUTTONUP,
    WM_SETTINGCHANGE, WM_TIMER, WM_WTSSESSION_CHANGE, WNDCLASSW, WS_POPUP, WTS_SESSION_LOCK,
    WTS_SESSION_UNLOCK,
};

use super::{
    chinese,
    session::{SessionNotifications, session_changed},
    wide, win32_result,
};
use crate::service::{DeliveryStatus, power_changed};

const TRAY_EVENT: u32 = WM_APP + 1;
pub(super) const ACTIVATE: u32 = WM_APP + 2;
const OPEN: u32 = 1;
const SETTINGS: u32 = 2;
const EXIT: u32 = 3;
const ICON_RETRY: usize = 1;
const ACTIVATION_POLL: usize = 2;
const SESSION_RETRY: usize = 3;
const TRAY_CLASS: &str = "BuddyLabsCo.PinGlide.Tray";
static EXPLORER_RESTART: OnceLock<u32> = OnceLock::new();

thread_local! {
    static WITHOUT_UI: Cell<bool> = const { Cell::new(false) };
    static ACTIVATION_EVENT: RefCell<Option<Arc<OwnedHandle>>> = const { RefCell::new(None) };
    static SESSION: RefCell<Option<SessionNotifications>> = const { RefCell::new(None) };
    static TRAY_WINDOW: Cell<HWND> = const { Cell::new(null_mut()) };
    static DELIVERY_STATUS: Cell<DeliveryStatus> = const { Cell::new(DeliveryStatus::Unregistered) };
}

pub struct Tray {
    window: HWND,
}

impl Tray {
    pub fn new(window_name: &[u16]) -> io::Result<Self> {
        let class = wide(TRAY_CLASS);
        // SAFETY: the current module handle and terminated strings remain valid for registration
        // and synchronous window creation. No Rust data is passed to the window procedure.
        let window = unsafe {
            let module = GetModuleHandleW(null());
            let window_class = WNDCLASSW {
                hInstance: module,
                lpszClassName: class.as_ptr(),
                lpfnWndProc: Some(window_proc),
                ..Default::default()
            };
            if RegisterClassW(&window_class) == 0 {
                return Err(io::Error::last_os_error());
            }
            EXPLORER_RESTART
                .get_or_init(|| RegisterWindowMessageW(wide("TaskbarCreated").as_ptr()));
            CreateWindowExW(
                0,
                class.as_ptr(),
                window_name.as_ptr(),
                WS_POPUP,
                0,
                0,
                0,
                0,
                null_mut(),
                null_mut(),
                module,
                null(),
            )
        };
        if window.is_null() {
            return Err(io::Error::last_os_error());
        }
        let tray = Self { window };
        TRAY_WINDOW.set(window);
        SESSION.with_borrow_mut(|session| *session = Some(SessionNotifications::new(window)));
        register_session(window);
        register_icon(window)?;
        Ok(tray)
    }

    pub fn window(&self) -> HWND {
        self.window
    }

    pub fn poll_activation(&self, event: Arc<OwnedHandle>) -> io::Result<()> {
        // SAFETY: the tray owns this HWND and pumps messages on this thread.
        if unsafe { SetTimer(self.window, ACTIVATION_POLL, 200, None) } == 0 {
            return Err(io::Error::last_os_error());
        }
        ACTIVATION_EVENT.set(Some(event));
        Ok(())
    }

    pub fn run_without_ui(&self) -> io::Result<bool> {
        WITHOUT_UI.set(true);
        let mut message = MSG::default();
        loop {
            // SAFETY: message is writable and only this thread reads and dispatches its queue.
            unsafe {
                match GetMessageW(&raw mut message, null_mut(), 0, 0) {
                    -1 => return Err(io::Error::last_os_error()),
                    0 => {
                        WITHOUT_UI.set(false);
                        return Ok(message.wParam != 0);
                    }
                    _ => {
                        TranslateMessage(&message);
                        DispatchMessageW(&message);
                    }
                }
            }
        }
    }
}

impl Drop for Tray {
    fn drop(&mut self) {
        TRAY_WINDOW.set(null_mut());
        DELIVERY_STATUS.set(DeliveryStatus::Unregistered);
        drop(SESSION.take());
        // SAFETY: Tray is dropped on the thread that created its window.
        unsafe {
            KillTimer(self.window, ACTIVATION_POLL);
            KillTimer(self.window, SESSION_RETRY);
            Shell_NotifyIconW(NIM_DELETE, &icon_data(self.window));
            DestroyWindow(self.window);
            UnregisterClassW(wide(TRAY_CLASS).as_ptr(), GetModuleHandleW(null()));
        }
        ACTIVATION_EVENT.set(None);
    }
}

fn load_icon() -> io::Result<HICON> {
    let mut image = include_bytes!("../../ui/assets/app-icon.png").to_vec();
    // SAFETY: the original app icon is a trusted PNG in writable memory for this call.
    let icon = unsafe {
        CreateIconFromResourceEx(image.as_mut_ptr(), image.len() as u32, 1, 0x30000, 0, 0, 0)
    };
    if icon.is_null() {
        Err(io::Error::last_os_error())
    } else {
        Ok(icon)
    }
}

fn icon_data(window: HWND) -> NOTIFYICONDATAW {
    NOTIFYICONDATAW {
        cbSize: size_of::<NOTIFYICONDATAW>() as u32,
        hWnd: window,
        uID: 1,
        ..Default::default()
    }
}

fn add_icon(window: HWND, icon: HICON, chinese: bool) -> io::Result<()> {
    let mut data = icon_data(window);
    data.uFlags = NIF_MESSAGE | NIF_ICON | NIF_TIP;
    data.uCallbackMessage = TRAY_EVENT;
    data.hIcon = icon;
    set_tooltip(&mut data, chinese);
    // SAFETY: data describes the live tray window and icon and remains valid for the call.
    win32_result(unsafe { Shell_NotifyIconW(NIM_ADD, &data) })
}

pub(crate) fn set_delivery_status(status: DeliveryStatus) {
    if DELIVERY_STATUS.replace(status) == status || TRAY_WINDOW.get().is_null() {
        return;
    }
    let mut data = icon_data(TRAY_WINDOW.get());
    data.uFlags = NIF_TIP;
    set_tooltip(&mut data, chinese());
    #[expect(
        clippy::undocumented_unsafe_blocks,
        reason = "data identifies the current tray icon"
    )]
    unsafe {
        Shell_NotifyIconW(NIM_MODIFY, &data);
    }
}

fn set_tooltip(data: &mut NOTIFYICONDATAW, chinese: bool) {
    use DeliveryStatus::*;
    let (english, chinese_reason) = match DELIVERY_STATUS.get() {
        Ready => ("Ready", "就绪"),
        Unregistered => ("Not Ready: No paired device", "未就绪：没有已配对的设备"),
        Connecting => (
            "Not Ready: Checking the connection",
            "未就绪：正在检查接收连接",
        ),
        Disconnected => ("Not Ready: Service unavailable", "未就绪：暂时无法连接服务"),
        Occupied => (
            "Not Ready: Receiver already connected",
            "未就绪：接收端已连接",
        ),
        Protocol => (
            "Not Ready: Invalid Service response",
            "未就绪：服务响应无效",
        ),
        Heartbeat => ("Not Ready: Connection timed out", "未就绪：接收连接超时"),
        Suspended => ("Not Ready: Windows suspended", "未就绪：Windows 已暂停"),
        Credential => (
            "Not Ready: Receiver credential unavailable",
            "未就绪：本地接收凭据不可用",
        ),
        Unconfigured => ("Not Ready: Service unavailable", "未就绪：暂时无法连接服务"),
    };
    let tip = if chinese {
        format!("鹊信 · {chinese_reason}")
    } else {
        format!("Pin Glide · {english}")
    };
    let tip = wide(&tip);
    data.szTip[..tip.len()].copy_from_slice(&tip);
}

fn register_session(window: HWND) {
    let registered = SESSION.with_borrow_mut(|session| {
        session
            .as_mut()
            .is_some_and(SessionNotifications::register_session)
    });
    #[expect(
        clippy::undocumented_unsafe_blocks,
        reason = "the retry timer belongs to the live tray HWND"
    )]
    unsafe {
        if registered {
            KillTimer(window, SESSION_RETRY);
        } else {
            SetTimer(window, SESSION_RETRY, 1000, None);
        }
    }
}

fn register_icon(window: HWND) -> io::Result<()> {
    let icon = load_icon()?;
    let result = add_icon(window, icon, chinese());
    // SAFETY: Shell_NotifyIconW copies the icon. The retry timer belongs to this tray window.
    unsafe {
        DestroyIcon(icon);
        if result.is_ok() {
            KillTimer(window, ICON_RETRY);
        } else if SetTimer(window, ICON_RETRY, 1000, None) == 0 {
            return Err(io::Error::last_os_error());
        }
    }
    Ok(())
}

unsafe extern "system" fn window_proc(
    window: HWND,
    message: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    match message {
        ACTIVATE => {
            open();
            0
        }
        TRAY_EVENT => {
            tray_event(window, lparam as u32);
            0
        }
        WM_SETTINGCHANGE => {
            if !WITHOUT_UI.get() {
                let _ = slint::invoke_from_event_loop(crate::ui::update_motion);
            }
            0
        }
        WM_TIMER if wparam == ICON_RETRY => {
            let _ = register_icon(window);
            0
        }
        WM_TIMER if wparam == SESSION_RETRY => {
            register_session(window);
            0
        }
        WM_WTSSESSION_CHANGE if matches!(wparam as u32, WTS_SESSION_LOCK | WTS_SESSION_UNLOCK) => {
            session_changed(wparam as u32 == WTS_SESSION_UNLOCK);
            0
        }
        WM_POWERBROADCAST if matches!(wparam as u32, PBT_APMSUSPEND | PBT_APMRESUMEAUTOMATIC) => {
            power_changed(wparam as u32 == PBT_APMSUSPEND);
            1
        }
        WM_TIMER if wparam == ACTIVATION_POLL => {
            let pending = ACTIVATION_EVENT.with_borrow(|event| {
                event.as_ref().is_some_and(|event| {
                    // SAFETY: the live auto-reset event is owned by the tray thread.
                    unsafe { WaitForSingleObject(event.as_raw_handle(), 0) == WAIT_OBJECT_0 }
                })
            });
            if pending {
                open();
            }
            0
        }
        _ if EXPLORER_RESTART
            .get()
            .is_some_and(|restart| *restart != 0 && *restart == message) =>
        {
            let _ = register_icon(window);
            0
        }
        // SAFETY: unhandled Windows messages use the default procedure with the original arguments.
        _ => unsafe { DefWindowProcW(window, message, wparam, lparam) },
    }
}

fn tray_event(window: HWND, event: u32) {
    let choice = match event {
        WM_LBUTTONDBLCLK => OPEN,
        WM_RBUTTONUP => show_menu(window, chinese()),
        _ => return,
    };
    match choice {
        OPEN => open(),
        SETTINGS if WITHOUT_UI.get() => open(),
        SETTINGS => {
            let _ = slint::invoke_from_event_loop(crate::ui::open_settings);
        }
        EXIT if WITHOUT_UI.get() => {
            // SAFETY: the menu callback runs on the thread pumping the native queue.
            unsafe { PostQuitMessage(0) };
        }
        EXIT => {
            let _ = slint::quit_event_loop();
        }
        _ => {}
    }
}

fn open() {
    if WITHOUT_UI.get() {
        // SAFETY: the native loop treats a nonzero quit code as an explicit reopen request.
        unsafe { PostQuitMessage(1) };
    } else {
        let _ = slint::invoke_from_event_loop(crate::ui::open);
    }
}

fn show_menu(window: HWND, chinese: bool) -> u32 {
    let (open, settings, exit) = if chinese {
        ("打开鹊信", "设置", "退出")
    } else {
        ("Open Pin Glide", "Settings", "Exit")
    };
    // SAFETY: the menu belongs to this thread and label pointers live through each AppendMenuW.
    unsafe {
        let menu = CreatePopupMenu();
        if menu.is_null() {
            return 0;
        }
        let choice = if AppendMenuW(menu, MF_STRING, OPEN as usize, wide(open).as_ptr()) != 0
            && AppendMenuW(menu, MF_STRING, SETTINGS as usize, wide(settings).as_ptr()) != 0
            && AppendMenuW(menu, MF_SEPARATOR, 0, null()) != 0
            && AppendMenuW(menu, MF_STRING, EXIT as usize, wide(exit).as_ptr()) != 0
        {
            let mut position = POINT::default();
            GetCursorPos(&raw mut position);
            SetForegroundWindow(window);
            let choice = TrackPopupMenu(
                menu,
                TPM_RETURNCMD | TPM_NONOTIFY | TPM_RIGHTBUTTON,
                position.x,
                position.y,
                0,
                window,
                null(),
            );
            PostMessageW(window, WM_NULL, 0, 0);
            choice as u32
        } else {
            0
        };
        DestroyMenu(menu);
        choice
    }
}
