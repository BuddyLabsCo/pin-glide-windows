use std::{
    fs, os::windows::fs::OpenOptionsExt, path::PathBuf, process::Command, sync::mpsc, thread,
    time::Duration,
};

use serde_json::{Value, json};
use slint::{ComponentHandle, Model};
#[cfg(debug_assertions)]
use std::sync::OnceLock;
use tokio_tungstenite::tungstenite::Message;
use windows_app::{service::DeliveryStatus, set_animation_preference_override, ui};

use super::sms_windows;
use crate::{
    add_sender_desktop::{dpapi, evidence, registration, wait_for},
    fixture::*,
    main_capture::{capture_rgb, write_ppm},
    select_backend,
};
use windows_sys::Win32::{
    Foundation::{HWND, RECT},
    UI::WindowsAndMessaging::GetWindowRect,
};

#[cfg(debug_assertions)]
#[test]
#[ignore = "requires an unlocked Windows desktop"]
fn startup_resumes_an_issued_replacement_with_unusable_old_credentials() {
    use windows_app::{launch_with_connection, service::Service};

    const CHILD: &str = "PIN_GLIDE_REPLACEMENT_STARTUP_CHILD";
    static SERVICE: OnceLock<Service> = OnceLock::new();
    let Ok(reason) = std::env::var(CHILD) else {
        for reason in ["missing", "revoked"] {
            let directory = tempfile::Builder::new()
                .prefix("PinGlideTest-")
                .tempdir()
                .unwrap();
            let child = Command::new(std::env::current_exe().unwrap())
                .args([
                    "startup_resumes_an_issued_replacement_with_unusable_old_credentials",
                    "--ignored",
                ])
                .env(CHILD, reason)
                .env("PINGLIDE_TEST_DIRECTORY", directory.path())
                .status()
                .unwrap();
            assert!(child.success(), "{reason}");
        }
        return;
    };
    let directory = std::path::PathBuf::from(std::env::var_os("PINGLIDE_TEST_DIRECTORY").unwrap());
    let path = directory.join("registration.json");
    let old = if reason == "revoked" {
        json!({"version":4,"receiver_revoked":true,"active_pairings":{}})
    } else {
        json!({"version":4,"receiver_id":"old-receiver","active_pairings":{}})
    };
    fs::write(&path, old.to_string()).unwrap();
    let (listener, service) = listen();
    let origin = format!(
        "https://localhost:{}",
        listener.local_addr().unwrap().port()
    );
    assert!(SERVICE.set(service).is_ok());
    fs::write(
        path.with_extension("pending.json"),
        json!({
            "version": 4, "active_pairings": {},
            "provisional_pairing": {
                "session_id": "resuming", "receiver_id": "receiver-1",
                "pairing_id": "new-pairing", "sender_id": "new-sender", "protected_key": dpapi::protect(&[21;32]),
            },
            "enrollment": {
                "origin": origin, "session_id": "resuming", "receiver_id": "receiver-1",
                "operation_id": "restart-fixture", "expires_at": {"secs_since_epoch":1,"nanos_since_epoch":0},
                "phase": {"Issued": dpapi::protect(RECEIVER_TOKEN.as_bytes())},
            }
        }).to_string(),
    ).unwrap();
    let server = thread::spawn(move || {
        let verify = std::panic::catch_unwind(|| {
            let mut socket = accept_delivery(accept(&acceptor(), &listener));
            socket.send(Message::Ping(vec![13].into())).unwrap();
            assert_eq!(socket.read().unwrap(), Message::Pong(vec![13].into()));
            let active: Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
            assert_eq!(active["receiver_id"], "receiver-1");
            assert_eq!(
                active["active_pairings"]["new-pairing"]["sender_id"],
                "new-sender"
            );
            assert!(active["receiver_revoked"].is_null());
            assert!(!path.with_extension("pending.json").exists());
        });
        slint::quit_event_loop().unwrap();
        verify.unwrap();
    });
    launch_with_connection(false, || SERVICE.get().cloned()).unwrap();
    server.join().unwrap();
}

#[test]
#[ignore = "requires an unlocked Windows desktop"]
fn removal_and_revocation_refresh_the_live_windows() {
    const CHILD: &str = "PIN_GLIDE_RECOVERY_DESKTOP_CHILD";
    if std::env::var_os(CHILD).is_none() {
        let child = Command::new(std::env::current_exe().unwrap())
            .args([
                "removal_and_revocation_refresh_the_live_windows",
                "--ignored",
            ])
            .env(CHILD, "1")
            .status()
            .unwrap();
        assert!(child.success());
        return;
    }
    select_backend();
    set_animation_preference_override(Some(false));
    slint::spawn_local(exercise()).unwrap();
    slint::run_event_loop_until_quit().unwrap();
}

async fn exercise() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("registration.json");
    let stored = registration(
        Some(dpapi::protect(RECEIVER_TOKEN.as_bytes())),
        &[("我的设备", "sender-1"), ("备用设备", "sender-2")],
    );
    fs::write(&path, stored.to_string()).unwrap();
    let locked = fs::OpenOptions::new()
        .read(true)
        .share_mode(1)
        .open(&path)
        .unwrap();
    let (listener, service) = listen();
    let (advance, next) = mpsc::channel();
    let server = thread::spawn(move || {
        let mut socket = accept_delivery(accept(&acceptor(), &listener));
        send_authenticated_delivery(
            &mut socket,
            "0",
            "sender-1",
            &[21; 32],
            ("synthetic", 7),
            json!({
                "type": "verification_message",
                "smsText": "【鹊信】这是一条测试短信，验证码 482913。请勿用于真实登录。",
                "otp": "482913",
            }),
        );
        let receipt: Value =
            serde_json::from_str(socket.read().unwrap().to_text().unwrap()).unwrap();
        assert_eq!(receipt["messageId"], "synthetic");
        next.recv_timeout(Duration::from_secs(10)).unwrap();
        send_authenticated_delivery(
            &mut socket,
            "1",
            "sender-2",
            &[21; 32],
            ("parcel", 8),
            json!({
                "type": "verification_message",
                "smsText": "【鹊信】您的包裹已送达前台。\n请于 20:00 前领取。",
                "otp": "",
            }),
        );
        let receipt: Value =
            serde_json::from_str(socket.read().unwrap().to_text().unwrap()).unwrap();
        assert_eq!(receipt["messageId"], "parcel");
        next.recv_timeout(Duration::from_secs(10)).unwrap();
        send_event(
            &mut socket,
            json!({"type":"pairing_removed","version":1,"pairingId":"0","senderId":"sender-1"}),
        );
        socket.send(Message::Ping(vec![29].into())).unwrap();
        assert_eq!(socket.read().unwrap(), Message::Pong(vec![29].into()));
        next.recv_timeout(Duration::from_secs(10)).unwrap();
        send_event(
            &mut socket,
            json!({"type":"pairing_removed","version":1,"pairingId":"1","senderId":"sender-2"}),
        );
        next.recv_timeout(Duration::from_secs(10)).unwrap();
        send_event(
            &mut socket,
            json!({"type":"device_revoked","version":1,"receiverId":"receiver-1"}),
        );
        assert_peer_left(&mut socket);
    });
    let main = ui::open_registered(true, path.clone(), move || Some(service.clone()))
        .unwrap()
        .unwrap();
    wait_for(|| sms_windows().len() == 1, "the first synthetic SMS popup").await;
    advance.send(()).unwrap();
    wait_for(|| sms_windows().len() == 2, "both synthetic SMS popups").await;
    let before = sms_windows();
    let bottom = before
        .iter()
        .map(|&handle| window_bounds(handle).bottom)
        .max()
        .unwrap();
    capture_popup_stack(&before, bottom, "scoped-popups-before").await;
    advance.send(()).unwrap();
    wait_for(
        || main.get_sender_devices().row_count() == 1 && sms_windows().len() == 1,
        "Pairing removal closes only its popup and clears its row before the file can be replaced",
    )
    .await;
    assert_eq!(fs::read_to_string(&path).unwrap(), stored.to_string());
    drop(locked);
    let remaining = sms_windows();
    assert!(before.contains(&remaining[0]));
    assert_eq!(window_bounds(remaining[0]).bottom, bottom);
    capture_popup_stack(&remaining, bottom, "scoped-popups-after").await;
    assert_eq!(
        main.get_sender_devices().row_data(0).unwrap().name,
        "备用设备"
    );
    assert_eq!(ui::delivery_status(), DeliveryStatus::Ready);
    evidence(main.window(), "main-one-key-after-delete");
    wait_for(
        || {
            fs::read(&path)
                .ok()
                .and_then(|bytes| serde_json::from_slice::<Value>(&bytes).ok())
                .is_some_and(|stored| {
                    stored["active_pairings"]["0"].is_null()
                        && stored["active_pairings"]["1"]["sender_id"] == "sender-2"
                })
        },
        "the scoped write retries and preserves the other Pairing",
    )
    .await;
    advance.send(()).unwrap();
    wait_for(
        || main.get_sender_devices().row_count() == 0,
        "the final Pairing was removed",
    )
    .await;
    assert_eq!(ui::delivery_status(), DeliveryStatus::Unregistered);
    evidence(main.window(), "main-no-phones");
    main.invoke_open_pairing();
    let add = ui::visible_pairing().unwrap();
    assert!(add.get_adding_device());
    evidence(add.window(), "revocation-before");
    advance.send(()).unwrap();
    wait_for(
        || main.get_receiver_revoked() && main.window().is_visible(),
        "revocation closes Add a device and shows the reason on Main",
    )
    .await;
    assert_eq!(ui::delivery_status(), DeliveryStatus::Revoked);
    assert!(ui::visible_pairing().is_none());
    evidence(main.window(), "revocation-after");
    ui::close();
    server.join().unwrap();
    slint::quit_event_loop().unwrap();
}

fn window_bounds(window: HWND) -> RECT {
    let mut rect = RECT::default();
    #[expect(
        clippy::undocumented_unsafe_blocks,
        reason = "the visible fixture window and its output are valid"
    )]
    unsafe {
        assert_ne!(GetWindowRect(window, &raw mut rect), 0);
    }
    rect
}

async fn capture_popup_stack(windows: &[HWND], bottom: i32, name: &str) {
    let Some(folder) = std::env::var_os("PIN_GLIDE_ADD_EVIDENCE").map(PathBuf::from) else {
        return;
    };
    wait_for(
        || {
            windows.iter().all(|&window| {
                let bounds = window_bounds(window);
                capture_rgb(
                    std::ptr::null_mut(),
                    (bounds.left + 200, bounds.top + 12),
                    (1, 1),
                )
                .into_iter()
                .all(|channel| channel > 245)
            })
        },
        "both layered popups have rendered on the desktop",
    )
    .await;
    let (width, height) = (404, 340);
    let mut image = [232, 240, 237].repeat(width * height);
    for &window in windows {
        let bounds = window_bounds(window);
        let size = (bounds.right - bounds.left, bounds.bottom - bounds.top);
        assert_eq!(size.0, 380);
        let pixels = capture_rgb(std::ptr::null_mut(), (bounds.left, bounds.top), size);
        let top = height as i32 - 12 + bounds.top - bottom;
        for (row, pixels) in pixels.as_chunks::<{ 380 * 3 }>().0.iter().enumerate() {
            let offset = ((top as usize + row) * width + 12) * 3;
            image[offset..offset + pixels.len()].copy_from_slice(pixels);
        }
    }
    write_ppm(
        &folder.join(format!("{name}.ppm")),
        (width as i32, height as i32),
        &image,
    );
}
