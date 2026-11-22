use std::{fs::File, io::Write, path::Path, ptr, sync::mpsc, thread};

use receiver_core::delivery::Sms;
use serde::Deserialize;
use slint::{
    LogicalPosition,
    platform::{PointerEventButton, WindowEvent},
    winit_030::winit::raw_window_handle::{HasWindowHandle, RawWindowHandle},
};
use windows_sys::Win32::{
    Foundation::HWND,
    System::DataExchange::{CloseClipboard, OpenClipboard},
    UI::WindowsAndMessaging::{
        GWL_EXSTYLE, GetForegroundWindow, GetWindowLongPtrW, WS_EX_NOACTIVATE, WS_EX_TOOLWINDOW,
    },
};

use super::*;

#[derive(Deserialize)]
struct Sample {
    key: String,
    sms: String,
    otp: String,
}

#[test]
#[ignore = "requires an unlocked interactive Windows desktop"]
fn renders_and_dismisses_the_five_approved_sms_states() {
    platform::enable_per_monitor_dpi().unwrap();
    crate::application::select_backend().unwrap();
    assert!(platform::session_unlocked());
    slint::spawn_local(async {
        let gallery = include_str!("../../../../../docs/prototypes/ui-tool/data/messages.js");
        let samples: Vec<Sample> = serde_json::from_str(
            gallery
                .trim()
                .strip_prefix("window.UI_CASES = ")
                .unwrap()
                .strip_suffix(';')
                .unwrap(),
        )
        .unwrap();
        for sample in samples {
            let name = &sample.key;
            show_sample(&sample).await;
            let height = match name.as_str() {
                "sms-no-otp" => 129,
                "sms-long" => 240,
                _ => 173,
            };
            check_popup(name, height, false);
            if name == "sms-long" {
                check_popup("sms-long-body-scrolled", height, true);
            }
            let window = last_popup();
            check_shell_behavior(window.window()).await;
            if name == "sms-long-code" {
                dismiss_with_clipboard_busy(window.window());
            } else {
                click(window.window(), LogicalPosition::new(30.0, 95.0));
            }
            assert!(!window.window().is_visible());
        }
        slint::quit_event_loop().unwrap();
    })
    .unwrap();
    slint::run_event_loop_until_quit().unwrap();
}

async fn show_sample(sample: &Sample) {
    show(
        ReceivedSms {
            sms: Sms {
                body: sample.sms.clone(),
                otp: sample.otp.clone(),
            },
            sender_name: "我的设备".into(),
        },
        true,
    )
    .await
    .unwrap();
}

fn last_popup() -> SmsWindow {
    POPUPS.with_borrow(|popups| popups.last_key_value().unwrap().1.0.clone_strong())
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

fn check_popup(name: &str, height: u32, scroll: bool) {
    let window = last_popup();
    let window = window.window();
    let scale = window.scale_factor();
    let size = window.size().to_logical(scale);
    assert!((size.width - 380.0).abs() <= 1.0, "{name}: {size:?}");
    assert!(
        (size.height - height as f32).abs() <= 1.0,
        "{name}: {size:?}"
    );
    if scroll {
        window.dispatch_event(WindowEvent::PointerScrolled {
            position: LogicalPosition::new(150.0, 115.0),
            delta_x: 0.0,
            delta_y: -3000.0,
        });
    }
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
