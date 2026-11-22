use std::io;
use std::os::windows::io::{AsRawHandle, FromRawHandle, OwnedHandle, RawHandle};
use std::ptr::{null, null_mut};
use std::thread::{self, JoinHandle};

use windows_sys::Win32::Foundation::{
    ERROR_ALREADY_EXISTS, GetLastError, HANDLE, HWND, LocalFree, WAIT_OBJECT_0,
};
use windows_sys::Win32::Security::Authorization::ConvertSidToStringSidW;
use windows_sys::Win32::Security::{GetTokenInformation, TOKEN_QUERY, TOKEN_USER, TokenUser};
use windows_sys::Win32::System::Threading::{
    CreateEventW, CreateMutexW, GetCurrentProcess, INFINITE, OpenProcessToken, SetEvent,
    WaitForMultipleObjects,
};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    AllowSetForegroundWindow, FindWindowW, GetWindowThreadProcessId, PostMessageW,
};

use super::tray::ACTIVATE;
use super::{wide, win32_result};

pub struct Instance {
    _mutex: OwnedHandle,
    open: OwnedHandle,
    window_name: Vec<u16>,
}

impl Instance {
    pub fn claim(explicit: bool) -> io::Result<Option<Self>> {
        let unique_name = format!("BuddyLabsCo.PinGlide.{}", user_sid()?);
        let window_name = wide(&unique_name);
        let open_name = wide(&format!(r"Global\{unique_name}.Open"));
        let mutex_name = wide(&format!(r"Global\{unique_name}.Instance"));
        let open = create_event(0, open_name.as_ptr())?;
        // SAFETY: mutex_name is NUL-terminated and default security attributes are valid.
        let (mutex, status) = unsafe {
            let mutex = CreateMutexW(null(), 0, mutex_name.as_ptr());
            (owned_handle(mutex)?, GetLastError())
        };
        if status == ERROR_ALREADY_EXISTS {
            if explicit {
                allow_foreground(&window_name);
                // SAFETY: open owns a live event handle.
                win32_result(unsafe { SetEvent(open.as_raw_handle()) })?;
            }
            return Ok(None);
        }
        Ok(Some(Self {
            _mutex: mutex,
            open,
            window_name,
        }))
    }

    pub fn window_name(&self) -> &[u16] {
        &self.window_name
    }

    pub fn listen(&self, window: HWND) -> io::Result<ActivationListener> {
        let open = self.open.try_clone()?;
        let stop = create_event(1, null())?;
        let stop_wait = stop.try_clone()?;
        let window = window as usize;
        let thread = thread::Builder::new()
            .name("instance-activation".into())
            .spawn(move || {
                let events = [stop_wait.as_raw_handle(), open.as_raw_handle()];
                // SAFETY: both owned handles and the array stay alive for each wait.
                while unsafe { WaitForMultipleObjects(2, events.as_ptr(), 0, INFINITE) }
                    == WAIT_OBJECT_0 + 1
                {
                    // SAFETY: the caller keeps the tray window alive until the listener joins.
                    if unsafe { PostMessageW(window as HWND, ACTIVATE, 0, 0) } == 0 {
                        break;
                    }
                }
            })?;
        Ok(ActivationListener {
            stop,
            thread: Some(thread),
        })
    }
}

pub struct ActivationListener {
    stop: OwnedHandle,
    thread: Option<JoinHandle<()>>,
}

impl Drop for ActivationListener {
    fn drop(&mut self) {
        // SAFETY: stop owns a live event handle until the waiting thread joins.
        unsafe { SetEvent(self.stop.as_raw_handle()) };
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

fn owned_handle(handle: HANDLE) -> io::Result<OwnedHandle> {
    if handle.is_null() {
        return Err(io::Error::last_os_error());
    }
    // SAFETY: callers transfer a freshly acquired handle that must be closed exactly once.
    Ok(unsafe { OwnedHandle::from_raw_handle(handle as RawHandle) })
}

fn create_event(manual_reset: i32, name: *const u16) -> io::Result<OwnedHandle> {
    // SAFETY: callers supply either a NUL-terminated name or null for an unnamed event.
    owned_handle(unsafe { CreateEventW(null(), manual_reset, 0, name) })
}

fn allow_foreground(window_name: &[u16]) {
    let mut pid = 0;
    // SAFETY: window_name is NUL-terminated and pid is a valid output pointer.
    unsafe {
        let window = FindWindowW(null(), window_name.as_ptr());
        if !window.is_null() && GetWindowThreadProcessId(window, &mut pid) != 0 {
            AllowSetForegroundWindow(pid);
        }
    }
}

fn user_sid() -> io::Result<String> {
    let mut required = 0;
    let mut token = null_mut();
    // SAFETY: token is a valid output slot. The null-buffer call obtains the required byte count.
    let token = unsafe {
        win32_result(OpenProcessToken(
            GetCurrentProcess(),
            TOKEN_QUERY,
            &raw mut token,
        ))?;
        let token = owned_handle(token)?;
        GetTokenInformation(
            token.as_raw_handle(),
            TokenUser,
            null_mut(),
            0,
            &raw mut required,
        );
        token
    };
    if required == 0 {
        return Err(io::Error::last_os_error());
    }
    let mut buffer = vec![0usize; (required as usize).div_ceil(size_of::<usize>())];
    // SAFETY: buffer is suitably aligned and holds at least required bytes.
    unsafe {
        win32_result(GetTokenInformation(
            token.as_raw_handle(),
            TokenUser,
            buffer.as_mut_ptr().cast(),
            required,
            &raw mut required,
        ))?;
        let user = &*buffer.as_ptr().cast::<TOKEN_USER>();
        let mut text = null_mut();
        win32_result(ConvertSidToStringSidW(user.User.Sid, &raw mut text))?;
        let length = (0..).take_while(|offset| *text.add(*offset) != 0).count();
        let sid = String::from_utf16_lossy(std::slice::from_raw_parts(text, length));
        LocalFree(text.cast());
        Ok(sid)
    }
}
