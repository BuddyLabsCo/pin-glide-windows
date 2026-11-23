#![cfg(windows)]

use std::{fs::File, io::Write, path::Path, ptr, sync::mpsc, thread, time::SystemTime};

use aes_gcm::{Aes256Gcm, KeyInit, Nonce, aead::Aead};
use base64::{Engine, engine::general_purpose::STANDARD};
use receiver_core::delivery::{Arrival, Envelope, Inbox, Sms};
use serde::Deserialize;
use slint::{
    ComponentHandle, LogicalPosition, Window,
    platform::{Key, PointerEventButton, WindowEvent},
    winit_030::WinitWindowAccessor,
    winit_030::winit::raw_window_handle::{HasWindowHandle, RawWindowHandle},
};
use windows_sys::Win32::{
    Foundation::{HWND, RECT},
    Graphics::Gdi::{GetMonitorInfoW, MONITOR_DEFAULTTOPRIMARY, MONITORINFO, MonitorFromWindow},
    System::{
        DataExchange::{CloseClipboard, GetClipboardData, OpenClipboard},
        Memory::{GlobalLock, GlobalSize, GlobalUnlock},
        Ole::CF_UNICODETEXT,
    },
    UI::HiDpi::{DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2, SetProcessDpiAwarenessContext},
    UI::WindowsAndMessaging::{
        GWL_EXSTYLE, GetForegroundWindow, GetWindowLongPtrW, GetWindowRect, WS_EX_NOACTIVATE,
        WS_EX_TOOLWINDOW,
    },
};

use windows_app::{select_backend, set_animation_preference_override, ui::show_sms};

#[path = "support/sms_motion.rs"]
mod sms_motion;

const SENDER_NAME: &str = "我的设备";

#[derive(Deserialize)]
struct Sample {
    key: String,
    sms: String,
    otp: String,
}

#[test]
#[ignore = "requires an unlocked interactive Windows desktop and writes a synthetic OTP to the clipboard"]
fn renders_stacks_and_dismisses_approved_sms_popups() {
    // SAFETY: set DPI awareness before creating any window in this test process.
    let enabled =
        unsafe { SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2) };
    assert_ne!(enabled, 0);
    select_backend().unwrap();
    set_animation_preference_override(Some(false));
    slint::spawn_local(async {
        check_stack().await;
        let gallery = include_str!("../../../docs/prototypes/ui-tool/data/messages.js");
        let samples: Vec<Sample> = serde_json::from_str(
            gallery
                .trim()
                .strip_prefix("window.UI_CASES = ")
                .unwrap()
                .strip_suffix(';')
                .unwrap(),
        )
        .unwrap();
        let mut inbox = Inbox::default();
        for sample in &samples {
            check_sample(&mut inbox, sample).await;
        }
        sms_motion::check(&samples).await;
        slint::quit_event_loop().unwrap();
    })
    .unwrap();
    slint::run_event_loop_until_quit().unwrap();
}

async fn check_stack() {
    let mut popups = Vec::new();
    for body in ["Oldest SMS", "Middle SMS\nSecond line", "Newest SMS"] {
        let popup = show_sms(
            Sms {
                body: body.into(),
                otp: String::new(),
            },
            SENDER_NAME.into(),
            true,
        )
        .await
        .unwrap()
        .unwrap();
        popups.push(popup);
    }
    let (oldest, work) = native_bounds(popups[0].window()).await;
    let (middle, _) = native_bounds(popups[1].window()).await;
    let (newest, _) = native_bounds(popups[2].window()).await;
    let scale = popups[0].window().scale_factor();
    let margin = (12.0 * scale).round() as i32;
    let gap = (8.0 * scale).round() as i32;
    assert_eq!(oldest.right, work.right - margin);
    assert_eq!(oldest.bottom, work.bottom - margin);
    assert_eq!(middle.bottom, oldest.top - gap);
    assert_eq!(newest.bottom, middle.top - gap);

    click(popups[1].window(), LogicalPosition::new(30.0, 65.0));
    assert!(!popups[1].window().is_visible());
    let (still_oldest, _) = native_bounds(popups[0].window()).await;
    let (moved_newest, _) = native_bounds(popups[2].window()).await;
    assert_eq!(still_oldest.top, oldest.top);
    assert_eq!(moved_newest.bottom, oldest.top - gap);
    for popup in [popups.first().unwrap(), popups.last().unwrap()] {
        click(popup.window(), LogicalPosition::new(30.0, 65.0));
        assert!(!popup.window().is_visible());
    }
}

async fn native_handle(window: &Window) -> HWND {
    let native = window.winit_window().await.unwrap();
    let RawWindowHandle::Win32(handle) = native.window_handle().unwrap().as_raw() else {
        panic!("expected a native Windows popup");
    };
    handle.hwnd.get() as HWND
}

async fn native_bounds(window: &Window) -> (RECT, RECT) {
    let hwnd = native_handle(window).await;
    let mut bounds = RECT::default();
    let mut monitor = MONITORINFO {
        cbSize: size_of::<MONITORINFO>() as u32,
        ..Default::default()
    };
    // SAFETY: the window keeps its handle live and both output buffers are writable.
    unsafe {
        assert_ne!(GetWindowRect(hwnd, &raw mut bounds), 0);
        let display = MonitorFromWindow(hwnd, MONITOR_DEFAULTTOPRIMARY);
        assert_ne!(GetMonitorInfoW(display, &raw mut monitor), 0);
    }
    (bounds, monitor.rcWork)
}

async fn check_sample(inbox: &mut Inbox, sample: &Sample) {
    let Sample { key, sms, otp } = sample;
    let height = match key.as_str() {
        "sms-no-otp" => 129,
        "sms-long" => 240,
        _ => 173,
    };
    let message = deliver(inbox, sms, otp);
    let popup = show_sms(message, SENDER_NAME.into(), true)
        .await
        .unwrap()
        .unwrap();
    let window = popup.window();
    check_shell_behavior(window).await;
    check_popup(window, key, height);
    match key.as_str() {
        "sms-otp" => {
            check_control_states(window);
            click(window, LogicalPosition::new(30.0, 95.0));
        }
        "sms-long" => {
            check_body_only_scroll(window);
            click(window, LogicalPosition::new(30.0, 95.0));
        }
        "sms-long-code" => {
            dismiss_with_clipboard_busy(window);
            check_copy(inbox, sms, otp).await;
        }
        _ => click(window, LogicalPosition::new(30.0, 95.0)),
    }
    assert!(!window.is_visible());
    assert!(popup.get_sms().is_empty());
}

fn deliver(inbox: &mut Inbox, body: &str, otp: &str) -> Sms {
    let key = [7; 32];
    let mut nonce = [0; 12];
    getrandom::fill(&mut nonce).unwrap();
    let plaintext = serde_json::json!({
        "type": "verification_message", "smsText": body, "otp": otp,
        "receivedAt": if otp == "newest-code" { "2100-01-01T00:00:00Z" } else { "2000-01-01T00:00:00Z" },
    }).to_string();
    let ciphertext = Aes256Gcm::new((&key).into())
        .encrypt(&Nonce::from(nonce), plaintext.as_bytes())
        .unwrap();
    inbox
        .receive(
            Arrival {
                time: SystemTime::now(),
                order: 1,
            },
            Envelope {
                pairing_id: "pairing",
                crypto_version: 1,
                nonce: &STANDARD.encode(nonce),
                ciphertext: &STANDARD.encode(ciphertext),
            },
            |_| Some(zeroize::Zeroizing::new(key)),
        )
        .unwrap()
}

async fn check_shell_behavior(window: &slint::Window) {
    let native = window.winit_window().await.unwrap();
    let RawWindowHandle::Win32(handle) = native.window_handle().unwrap().as_raw() else {
        panic!("expected a native Windows popup");
    };
    let hwnd = handle.hwnd.get() as HWND;
    // SAFETY: the native window keeps the HWND valid throughout these queries.
    let (style, foreground) =
        unsafe { (GetWindowLongPtrW(hwnd, GWL_EXSTYLE), GetForegroundWindow()) };
    let required = (WS_EX_NOACTIVATE | WS_EX_TOOLWINDOW) as isize;
    assert_eq!(style & required, required);
    assert_ne!(hwnd, foreground);
}

fn dismiss_with_clipboard_busy(window: &slint::Window) {
    let (ready, locked) = mpsc::channel();
    let (release, finished) = mpsc::channel();
    let owner = thread::spawn(move || {
        // SAFETY: this thread opens and closes the clipboard without changing its contents.
        unsafe {
            let opened = OpenClipboard(ptr::null_mut()) != 0;
            ready.send(opened).unwrap();
            if opened {
                let _ = finished.recv();
                CloseClipboard();
            }
        }
    });
    assert!(locked.recv().unwrap());
    click(window, LogicalPosition::new(318.0, 60.0));
    release.send(()).unwrap();
    owner.join().unwrap();
}

async fn check_copy(inbox: &mut Inbox, sms: &str, long_code: &str) {
    let mut nonce = [0; 16];
    getrandom::fill(&mut nonce).unwrap();
    let expected = format!("{long_code}-{}", STANDARD.encode(nonce));
    let message = deliver(inbox, sms, &expected);
    let popup = show_sms(message, SENDER_NAME.into(), true)
        .await
        .unwrap()
        .unwrap();
    let window = popup.window();
    deliver(inbox, "New SMS", "newest-code");
    assert_eq!(inbox.latest(), Some("newest-code"));
    click(window, LogicalPosition::new(318.0, 60.0));
    assert_eq!(clipboard_text(), expected);
    assert_eq!(inbox.latest(), Some("newest-code"));
    assert!(!window.is_visible());
}

fn clipboard_text() -> String {
    // SAFETY: keep the clipboard open and the handle locked throughout the bounded read.
    unsafe {
        assert_ne!(OpenClipboard(ptr::null_mut()), 0);
        let handle = GetClipboardData(u32::from(CF_UNICODETEXT));
        let text = GlobalLock(handle).cast::<u16>();
        let result = if text.is_null() {
            None
        } else {
            let units = std::slice::from_raw_parts(text, GlobalSize(handle) / size_of::<u16>());
            let value = units
                .iter()
                .position(|&unit| unit == 0)
                .and_then(|end| String::from_utf16(&units[..end]).ok());
            GlobalUnlock(handle);
            value
        };
        CloseClipboard();
        result.expect("the clipboard contains Unicode text")
    }
}

fn check_popup(window: &Window, name: &str, height: u32) {
    let scale = window.scale_factor();
    let size = window.size().to_logical(scale);
    assert!((size.width - 380.0).abs() <= 1.0, "{name}: {size:?}");
    assert!(
        (size.height - height as f32).abs() <= 1.0,
        "{name}: {size:?}"
    );
    capture(window, name);
}

fn check_body_only_scroll(window: &Window) {
    let before = window.take_snapshot().unwrap();
    window.dispatch_event(WindowEvent::PointerScrolled {
        position: LogicalPosition::new(150.0, 115.0),
        delta_x: 0.0,
        delta_y: -3000.0,
    });
    let after = window.take_snapshot().unwrap();
    let scale = window.scale_factor();
    let pixel = |position: f32| (position * scale).round() as usize;
    let width = before.width() as usize;
    let row = |position| pixel(position) * width;
    let frames = (before.as_slice(), after.as_slice());
    assert!(
        frames.0[..row(86.0)] == frames.1[..row(86.0)],
        "the code row moved"
    );
    assert!(
        (pixel(86.0)..pixel(193.0)).any(|line| {
            let text_only = line * width + pixel(14.0)..line * width + pixel(356.0);
            frames.0[text_only.clone()] != frames.1[text_only]
        }),
        "the SMS text did not scroll"
    );
    assert!(
        frames.0[row(193.0)..] == frames.1[row(193.0)..],
        "the Sender footer moved"
    );
    capture(window, "sms-long-body-scrolled");
}

fn check_control_states(window: &Window) {
    let hover = LogicalPosition::new(270.0, 60.0);
    assert_eq!(pixel_at(window, hover), [255, 255, 255]);
    window.dispatch_event(WindowEvent::PointerMoved { position: hover });
    assert_eq!(pixel_at(window, hover), [239, 242, 245]);
    capture(window, "sms-copy-hover");
    window.dispatch_event(WindowEvent::PointerExited);
    assert_eq!(pixel_at(window, hover), [255, 255, 255]);

    window.dispatch_event(WindowEvent::WindowActiveChanged(true));
    let button_edge = LogicalPosition::new(318.0, 43.0);
    let body_edge = LogicalPosition::new(200.0, 87.0);
    let mut focused_button = false;
    let mut focused_body = false;
    for _ in 0..2 {
        press(window, Key::Tab);
        if pixel_at(window, button_edge) == [76, 128, 116] {
            focused_button = true;
            capture(window, "sms-copy-focus");
        } else {
            assert_eq!(pixel_at(window, body_edge), [76, 128, 116]);
            focused_body = true;
            capture(window, "sms-body-focus");
        }
    }
    assert!(focused_button && focused_body);
    window.dispatch_event(WindowEvent::WindowActiveChanged(false));
}

fn press(window: &Window, key: Key) {
    let text: slint::SharedString = key.into();
    window.dispatch_event(WindowEvent::KeyPressed { text: text.clone() });
    window.dispatch_event(WindowEvent::KeyReleased { text });
}

fn pixel_at(window: &Window, position: LogicalPosition) -> [u8; 3] {
    let frame = window.take_snapshot().unwrap();
    let point = position.to_physical(window.scale_factor());
    let pixel = &frame.as_slice()[(point.y as u32 * frame.width() + point.x as u32) as usize];
    [pixel.r, pixel.g, pixel.b]
}

fn capture(window: &Window, name: &str) {
    if let Some(directory) = std::env::var_os("PINGLIDE_SMS_EVIDENCE_DIR") {
        let path = Path::new(&directory).join(format!("{name}.ppm"));
        let pixels = window.take_snapshot().unwrap();
        let mut file = File::create(path).unwrap();
        writeln!(file, "P6\n{} {}\n255", pixels.width(), pixels.height()).unwrap();
        let rgb: Vec<_> = pixels
            .as_slice()
            .iter()
            .flat_map(|pixel| [pixel.r, pixel.g, pixel.b])
            .collect();
        file.write_all(&rgb).unwrap();
    }
}

fn click(window: &slint::Window, position: LogicalPosition) {
    window.dispatch_event(WindowEvent::PointerPressed {
        position,
        button: PointerEventButton::Left,
    });
    window.dispatch_event(WindowEvent::PointerReleased {
        position,
        button: PointerEventButton::Left,
    });
}
