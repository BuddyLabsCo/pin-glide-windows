use std::{
    fs::{self, File},
    io::Write,
    path::{Path, PathBuf},
    sync::mpsc,
    thread,
    time::Duration,
};

use i_slint_core::animations::{current_tick, update_animations};
use serde_json::json;
use slint::{
    ComponentHandle, LogicalPosition, LogicalSize, Window,
    platform::{PointerEventButton, WindowEvent},
};

use super::{PAIRING, PairingStage, PairingWindow, SERVICES, select_test_backend};
use crate::{
    MainWindow,
    service::tests::pairing_fixture as fixture,
    storage::{RegistrationStore, tests::dpapi},
    ui::{MAIN, close, show_registered},
};
use fixture::*;

#[test]
#[ignore = "requires an interactive Windows desktop"]
fn captures_add_sender_and_resumes_the_same_qr_after_back() {
    select_test_backend().unwrap();
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("registration.json");
    let mut registration = json!({
        "version":4, "receiver_id":"receiver-1",
        "protected_receiver_token":dpapi::protect(RECEIVER_TOKEN.as_bytes()),
        "active_pairings":{},"provisional_pairing":null
    });
    fs::write(&path, registration.to_string()).unwrap();
    let store = RegistrationStore::new(path.clone());
    show_registered(&store, true).unwrap();
    let main = MAIN.with_borrow(|main| main.as_ref().unwrap().clone_strong());
    let evidence = std::env::var_os("PIN_GLIDE_ADD_EVIDENCE").map(PathBuf::from);
    capture(&evidence, main.window(), "main-no-phones");

    let (listener, service) = listen();
    SERVICES.with_borrow_mut(|services| services.extend((0..5).map(|_| service.clone())));
    let (next, advance) = mpsc::channel();
    let (qr_send, qr_receive) = mpsc::channel();
    let server = thread::spawn(move || {
        let tls = acceptor();
        let mut rejected = std::io::BufReader::new(accept(&tls, &listener));
        read_request_parts(&mut rejected);
        advance.recv().unwrap();
        write_response(rejected.get_mut(), "503 Service Unavailable", &json!({}));
        for (name, seconds) in [
            ("expired", 2),
            ("canceled", 30),
            ("failed", 30),
            ("completed", 272),
        ] {
            let mut initial = snapshot(name, "awaiting_authorization", seconds);
            initial["mode"] = json!("add_sender");
            accept_creation(accept(&tls, &listener), &initial);
            let mut events = accept_events(accept(&tls, &listener), name);
            send_state(&mut events, &initial);
            if name == "expired" {
                assert_peer_left(&mut events);
                continue;
            }
            advance.recv().unwrap();
            let mut claimed = claim(initial);
            if name == "canceled" {
                cancel(&mut events, claimed);
                continue;
            }
            send_state(&mut events, &claimed);
            if name == "failed" {
                send_event(
                    &mut events,
                    json!({"version":1,"type":"error","error":{"code":"fixture","detail":"fixture"}}),
                );
                assert_delete(accept(&tls, &listener), name);
                continue;
            }
            let qr = qr_receive.recv().unwrap();
            sender_handshake(&mut events, &qr);
            let mut report = std::io::BufReader::new(accept(&tls, &listener));
            let (request, headers, _) = read_request_parts(&mut report);
            assert_eq!(
                request,
                "POST /v1/pairing-sessions/completed/complete HTTP/1.1\r\n"
            );
            assert_bearer(&headers);
            claimed["receiverConfirmed"] = json!(true);
            write_response(report.get_mut(), "200 OK", &claimed);
            claimed["senderConfirmed"] = json!(true);
            claimed["state"] = json!("completed");
            send_state(&mut events, claimed);
        }
    });

    click(main.window(), 200.0, 188.0);
    let window = PAIRING.with_borrow(|view| view.as_ref().unwrap().window.clone_strong());
    assert!(window.get_adding_device());
    capture_loading(&evidence, &window, "add-preparing");
    next.send(()).unwrap();
    wait_for_stage(&window, PairingStage::CreateFailed);
    capture(&evidence, window.window(), "add-create-failed");
    window.invoke_refresh();
    wait_for_stage(&window, PairingStage::Expired);
    capture(&evidence, window.window(), "add-expired");
    window.invoke_refresh();
    wait_for_stage(&window, PairingStage::Waiting);
    next.send(()).unwrap();
    wait_for_stage(&window, PairingStage::Canceled);
    capture(&evidence, window.window(), "add-canceled");
    window.invoke_refresh();
    wait_for_stage(&window, PairingStage::Waiting);
    next.send(()).unwrap();
    wait_for_stage(&window, PairingStage::Failed);
    capture(&evidence, window.window(), "add-failed");

    registration["active_pairings"]["a-old-pairing"] =
        json!({"sender_id":"old","protected_key":dpapi::protect(&[21;32]),"name":"我的设备"});
    fs::write(&path, registration.to_string()).unwrap();
    window.invoke_refresh();
    wait_for_stage(&window, PairingStage::Waiting);
    capture(&evidence, window.window(), "add-waiting");
    let qr = window.get_qr().to_rgb8().unwrap();
    click(window.window(), 40.0, 40.0);
    assert!(!window.window().is_visible());
    assert!(main.window().is_visible());
    capture(&evidence, main.window(), "main-ready");
    click(main.window(), 200.0, 188.0);
    assert!(window.window().is_visible());
    assert!(!main.window().is_visible());
    assert_eq!(window.get_qr().to_rgb8().unwrap().as_slice(), qr.as_slice());
    let grayscale: Vec<_> = qr.as_slice().iter().map(|pixel| pixel.r).collect();
    next.send(()).unwrap();
    wait_for_stage(&window, PairingStage::Confirming);
    capture_loading(&evidence, &window, "add-confirming");
    let hidden = std::env::var_os("PIN_GLIDE_ADD_HIDDEN").is_some();
    if hidden {
        window.invoke_back();
        assert!(main.window().is_visible());
        assert!(!window.window().is_visible());
    }
    qr_send
        .send(decode_pixels(
            qr.width() as usize,
            qr.height() as usize,
            &grayscale,
        ))
        .unwrap();
    wait_for_stage(&window, PairingStage::Completed);
    let registration: serde_json::Value = serde_json::from_slice(&fs::read(path).unwrap()).unwrap();
    assert_eq!(
        dpapi::unprotect(&registration["protected_receiver_token"]),
        RECEIVER_TOKEN.as_bytes()
    );
    if hidden {
        use slint::Model;
        assert!(!window.window().is_visible());
        assert_eq!(
            main.get_sender_names().iter().collect::<Vec<_>>(),
            ["我的设备", "未命名设备"]
        );
        capture(&evidence, main.window(), "main-hidden-completion");
    } else {
        assert_eq!(window.get_sender_name(), "未命名设备");
        verify_naming(&evidence, &window, &main);
        verify_completion_motion(&evidence);
    }
    close();
    server.join().unwrap();
}

fn click(window: &Window, x: f32, y: f32) {
    let position = LogicalPosition::new(x, y);
    let button = PointerEventButton::Left;
    window.dispatch_event(WindowEvent::PointerMoved { position });
    window.dispatch_event(WindowEvent::PointerPressed { position, button });
    window.dispatch_event(WindowEvent::PointerReleased { position, button });
    window.dispatch_event(WindowEvent::PointerExited);
}

fn wait_for_stage(window: &PairingWindow, stage: PairingStage) {
    crate::startup_tests::wait_for(
        || {
            PAIRING.with_borrow_mut(|view| view.as_mut().unwrap().render());
            window.get_stage() == stage
        },
        &format!("expected {stage:?}"),
    );
}

fn verify_naming(evidence: &Option<PathBuf>, window: &PairingWindow, main: &MainWindow) {
    window.set_sender_name("备用设备".into());
    capture(evidence, window.window(), "pair-added");
    for (suffix, input, expected) in [
        ("empty", String::new(), "请填写设备名称。"),
        (
            "duplicate",
            "我的设备".into(),
            "已有一台同名设备，请换一个名称。",
        ),
        (
            "too-long",
            "我的备用设备".repeat(7),
            "设备名称最多 40 个字符。",
        ),
        (
            "invalid-character",
            String::new(),
            "名称包含无效字符，请重新输入。",
        ),
    ] {
        window.set_name_error("".into());
        window.set_sender_name(input.into());
        window.set_invalid_paste(suffix == "invalid-character");
        if suffix != "invalid-character" {
            capture(
                evidence,
                window.window(),
                &format!("pair-added-input-{suffix}"),
            );
        }
        window.invoke_finish_naming();
        assert_eq!(window.get_name_error(), expected);
        assert!(window.get_name_focused());
        capture(
            evidence,
            window.window(),
            &format!("pair-added-error-{suffix}"),
        );
    }
    window.set_invalid_paste(false);
    let (name, screenshot) = if std::env::var_os("PIN_GLIDE_ADD_RENAMED_EVIDENCE").is_some() {
        ("办公室手机", "pair-added-name-saved")
    } else {
        ("备用设备", "main-two-phones")
    };
    window.set_sender_name(name.into());
    window.invoke_finish_naming();
    assert!(!window.window().is_visible());
    assert!(main.window().is_visible());
    use slint::Model;
    assert_eq!(
        main.get_sender_names().iter().collect::<Vec<_>>(),
        ["我的设备", name]
    );
    capture(evidence, main.window(), screenshot);
}

fn verify_completion_motion(evidence: &Option<PathBuf>) {
    for motion in [true, false] {
        let window = PairingWindow::new().unwrap();
        window.window().set_size(LogicalSize::new(440.0, 560.0));
        window.set_chinese(true);
        window.set_adding_device(true);
        window.set_sender_name("备用设备".into());
        window.set_motion_enabled(motion);
        window.set_completion_motion(true);
        window.set_stage(PairingStage::Confirming);
        let start = advance_animation(330);
        window.window().take_snapshot().unwrap();
        window.set_stage(PairingStage::Completed);
        let first = window.window().take_snapshot().unwrap();
        if motion {
            update_animations((start + Duration::from_millis(90)).into());
            let middle = window.window().take_snapshot().unwrap();
            assert!(first.as_slice() != middle.as_slice());
            capture(evidence, window.window(), "add-completion-transition");
            update_animations((start + Duration::from_millis(180)).into());
            let settled = window.window().take_snapshot().unwrap();
            assert!(middle.as_slice() != settled.as_slice());
        } else {
            update_animations((start + Duration::from_millis(90)).into());
            let middle = window.window().take_snapshot().unwrap();
            assert!(first.as_slice() == middle.as_slice());
            capture(evidence, window.window(), "add-completion-reduced");
        }
    }
}

fn capture_loading(evidence: &Option<PathBuf>, window: &PairingWindow, name: &str) {
    window.set_motion_enabled(true);
    advance_animation(100);
    capture(evidence, window.window(), name);
    window.set_motion_enabled(false);
}

fn advance_animation(phase: u64) -> Duration {
    let period = 1800;
    let elapsed = current_tick().as_millis();
    let next = Duration::from_millis((elapsed / period + 1) * period + phase);
    update_animations(next.into());
    next
}

fn capture(directory: &Option<PathBuf>, window: &Window, name: &str) {
    let Some(directory) = directory else { return };
    let pixels = window.take_snapshot().unwrap();
    let path = Path::new(directory).join(format!("{name}.ppm"));
    let mut file = File::create(path).unwrap();
    write!(file, "P6\n{} {}\n255\n", pixels.width(), pixels.height()).unwrap();
    let rgb: Vec<_> = pixels
        .as_slice()
        .iter()
        .flat_map(|pixel| [pixel.r, pixel.g, pixel.b])
        .collect();
    file.write_all(&rgb).unwrap();
}
