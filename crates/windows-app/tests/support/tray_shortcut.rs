use std::{fs, ptr::null_mut, sync::mpsc, thread, time::Duration};

use serde_json::{Value, json};
use tokio_tungstenite::tungstenite::Message;
use windows_app::service::Service;
use windows_sys::Win32::{
    Foundation::HWND,
    System::{
        DataExchange::{
            CloseClipboard, GetClipboardData, GetClipboardSequenceNumber, OpenClipboard,
        },
        Memory::{GlobalLock, GlobalSize, GlobalUnlock},
        Ole::CF_UNICODETEXT,
    },
    UI::{
        HiDpi::{DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2, SetThreadDpiAwarenessContext},
        Input::KeyboardAndMouse::{MOD_ALT, MOD_CONTROL, VK_CONTROL, VK_MENU},
        WindowsAndMessaging::{
            CreateWindowExW, DispatchMessageW, GetMessageW, GetWindowTextW, MSG, SendMessageW,
            WM_HOTKEY, WS_OVERLAPPEDWINDOW, WS_VISIBLE,
        },
    },
};

use super::{
    AppProcess, Fixture, class, dpapi, keyboard,
    service_fixture::{
        self, RECEIVER_TOKEN, accept, accept_delivery, acceptor, send_authenticated_delivery,
    },
    support::wait_for,
    visible, wait_for_window,
};

const MODE: &str = "PIN_GLIDE_TRAY_SHORTCUT_TEST_MODE";
const ORIGIN: &str = "PIN_GLIDE_TRAY_SHORTCUT_TEST_ORIGIN";
const TEST: &str = "tray_shortcut::tray_shortcut_copies_with_another_process_focused";
const KEY: u8 = b'V';
const MODIFIERS: u32 = MOD_CONTROL | MOD_ALT;

#[test]
#[ignore = "requires an unlocked Windows desktop and copies a synthetic OTP"]
fn tray_shortcut_copies_with_another_process_focused() {
    match std::env::var(MODE).as_deref() {
        Ok("receiver") => return windows_app::launch_with_connection(false, connect).unwrap(),
        Ok("foreground") => return foreground_app(),
        _ => {}
    }
    // SAFETY: use physical coordinates for the test windows.
    unsafe { SetThreadDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2) };
    let fixture = Fixture::new();
    let otp = format!("PIN-GLIDE-SYNTHETIC-OTP-{}", std::process::id());
    let expected = otp.clone();
    let (listener, origin) = service_fixture::listen_origin();
    seed(fixture.path());
    let (deliver, ready) = mpsc::channel();
    let server = thread::spawn(move || {
        let mut socket = accept_delivery(accept(&acceptor(), &listener));
        ready.recv_timeout(Duration::from_secs(10)).unwrap();
        for (sequence, message_id, otp) in [(1, "with-code", otp.as_str()), (2, "without-code", "")]
        {
            send_authenticated_delivery(
                &mut socket,
                "pairing-1",
                "sender-1",
                &[7; 32],
                (message_id, sequence),
                json!({"type":"verification_message","smsText":"Synthetic SMS","otp":otp}),
            );
            loop {
                match socket.read().unwrap() {
                    Message::Text(text) => {
                        let receipt: Value = serde_json::from_str(&text).unwrap();
                        assert_eq!(receipt["type"], "receipt");
                        assert_eq!(receipt["messageId"], message_id);
                        break;
                    }
                    Message::Ping(_) => socket.flush().unwrap(),
                    frame => panic!("unexpected fixture frame: {frame:?}"),
                }
            }
        }
        socket
    });
    let receiver = child("receiver", Some((fixture.path(), &origin)));
    let tray = wait_for_window(receiver.0.id(), |window| {
        class(window) == "BuddyLabsCo.PinGlide.Tray"
    });
    let other = child("foreground", None);
    let foreground = wait_for_window(other.0.id(), visible);
    keyboard::activate(foreground);

    // SAFETY: querying the sequence number does not open or modify the clipboard.
    let sequence = unsafe { GetClipboardSequenceNumber() };
    keyboard::press(foreground, &[VK_CONTROL, VK_MENU, KEY.into()]);
    thread::sleep(Duration::from_millis(100));
    // SAFETY: the empty-inbox hotkey must leave the sequence number unchanged.
    assert_eq!(unsafe { GetClipboardSequenceNumber() }, sequence);
    deliver.send(()).unwrap();
    let popup = wait_for_window(receiver.0.id(), is_sms);
    wait_for_window(receiver.0.id(), |window| window != popup && is_sms(window));
    wait_for(
        || super::find_window(receiver.0.id(), is_sms).is_none(),
        "every SMS popup must close",
    );
    assert!(
        !has_visible_window(receiver.0.id()),
        "receiver must be tray-only",
    );

    keyboard::activate(foreground);
    let locked = Clipboard::open();
    // SAFETY: send the known registered chord to the fixture tray while this other process holds the clipboard.
    unsafe {
        SendMessageW(
            tray,
            WM_HOTKEY,
            1,
            ((u32::from(KEY) << 16) | MODIFIERS) as isize,
        )
    };
    drop(locked);
    // SAFETY: a failed clipboard write must leave the previous value in place.
    assert_eq!(unsafe { GetClipboardSequenceNumber() }, sequence);
    keyboard::press(foreground, &[VK_CONTROL, VK_MENU, KEY.into()]);
    wait_for(
        || clipboard_text().as_deref() == Some(expected.as_str()),
        "Windows did not deliver the global shortcut",
    );
    assert_eq!(keyboard::foreground(), foreground);
    assert!(!has_visible_window(receiver.0.id()));
    drop(receiver);
    drop(server.join().unwrap());
}

fn seed(path: &std::path::Path) {
    fs::write(
        path.join("settings.json"),
        json!({
            "version":3, "language":"english", "popup_duration_seconds":2,
            "shortcut":{"key":KEY, "modifiers":MODIFIERS}
        })
        .to_string(),
    )
    .unwrap();
    fs::write(path.join("registration.json"), json!({
        "version":4, "receiver_id":"receiver-1",
        "protected_receiver_token":dpapi::protect(RECEIVER_TOKEN.as_bytes()),
        "active_pairings":{"pairing-1":{
            "sender_id":"sender-1", "protected_key":dpapi::protect(&[7;32]), "name":"Synthetic Sender"
        }}
    }).to_string()).unwrap();
}

fn child(mode: &str, service: Option<(&std::path::Path, &str)>) -> AppProcess {
    let mut command = std::process::Command::new(std::env::current_exe().unwrap());
    command.args([TEST, "--ignored", "--exact"]).env(MODE, mode);
    if let Some((path, origin)) = service {
        command
            .env("PINGLIDE_TEST_DIRECTORY", path)
            .env(ORIGIN, origin);
    }
    AppProcess(command.spawn().unwrap())
}

fn connect() -> Option<Service> {
    Some(service_fixture::trusted_service(
        &std::env::var(ORIGIN).unwrap(),
    ))
}

fn foreground_app() {
    // SAFETY: STATIC is a built-in class, the strings are terminated, and the message buffer is writable.
    unsafe {
        let window = CreateWindowExW(
            0,
            windows_sys::w!("STATIC"),
            windows_sys::w!("Pin Glide keyboard fixture"),
            WS_VISIBLE | WS_OVERLAPPEDWINDOW,
            100,
            100,
            320,
            180,
            null_mut(),
            null_mut(),
            null_mut(),
            std::ptr::null(),
        );
        assert!(!window.is_null());
        let mut message = MSG::default();
        while GetMessageW(&raw mut message, null_mut(), 0, 0) > 0 {
            DispatchMessageW(&message);
        }
    }
}

fn title(window: HWND) -> String {
    let mut text = [0; 96];
    // SAFETY: the output buffer is writable for its specified capacity.
    let count = unsafe { GetWindowTextW(window, text.as_mut_ptr(), text.len() as i32) };
    String::from_utf16_lossy(&text[..count as usize])
}

fn is_sms(window: HWND) -> bool {
    visible(window) && title(window) == "Pin Glide · New SMS"
}

fn has_visible_window(process: u32) -> bool {
    super::find_window(process, |window| {
        visible(window) && class(window) != "Winit Thread Event Target"
    })
    .is_some()
}

struct Clipboard;

impl Clipboard {
    fn open() -> Self {
        // SAFETY: the test thread closes the clipboard through Drop.
        wait_for(
            || unsafe { OpenClipboard(null_mut()) != 0 },
            "clipboard must open",
        );
        Self
    }
}

impl Drop for Clipboard {
    fn drop(&mut self) {
        // SAFETY: this guard owns an open clipboard on the test thread.
        unsafe { CloseClipboard() };
    }
}

fn clipboard_text() -> Option<String> {
    let _clipboard = Clipboard::open();
    // SAFETY: hold the clipboard open and keep its allocation locked during the bounded read.
    unsafe {
        let handle = GetClipboardData(u32::from(CF_UNICODETEXT));
        let text = GlobalLock(handle).cast::<u16>();
        if text.is_null() {
            return None;
        }
        let units = std::slice::from_raw_parts(text, GlobalSize(handle) / 2);
        let end = units
            .iter()
            .position(|&unit| unit == 0)
            .unwrap_or(units.len());
        let value = String::from_utf16(&units[..end]).ok();
        GlobalUnlock(handle);
        value
    }
}
