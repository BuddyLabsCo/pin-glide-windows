use std::{fs, process::Command, sync::mpsc, thread, time::Duration};

use serde_json::{Value, json};
use slint::{ComponentHandle, Model};
use tokio_tungstenite::tungstenite::Message;
use windows_app::{service::DeliveryStatus, set_animation_preference_override, ui};

use super::visible_sms;
use crate::{
    add_sender_desktop::{dpapi, evidence, registration, wait_for},
    fixture::*,
    select_backend,
};

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
        assert!(socket.read().is_err());
    });
    let main = ui::open_registered(true, path, move || Some(service.clone()))
        .unwrap()
        .unwrap();
    wait_for(|| visible_sms().is_some(), "the synthetic SMS popup").await;
    advance.send(()).unwrap();
    wait_for(
        || main.get_sender_devices().row_count() == 1 && visible_sms().is_none(),
        "Pairing removal closes its popup and clears its row",
    )
    .await;
    assert_eq!(
        main.get_sender_devices().row_data(0).unwrap().name,
        "备用设备"
    );
    assert_eq!(ui::delivery_status(), DeliveryStatus::Ready);
    evidence(main.window(), "main-one-key-after-delete");
    advance.send(()).unwrap();
    wait_for(
        || main.get_sender_devices().row_count() == 0,
        "the final Pairing was removed",
    )
    .await;
    assert_eq!(ui::delivery_status(), DeliveryStatus::Unregistered);
    evidence(main.window(), "main-no-phones");
    main.invoke_open_pairing();
    assert!(ui::visible_pairing().unwrap().get_adding_device());
    advance.send(()).unwrap();
    wait_for(
        || main.get_receiver_revoked() && main.window().is_visible(),
        "revocation closes Add a device and shows the reason on Main",
    )
    .await;
    assert_eq!(ui::delivery_status(), DeliveryStatus::Revoked);
    assert!(ui::visible_pairing().is_none());
    evidence(main.window(), "main-revoked");
    ui::close();
    server.join().unwrap();
    slint::quit_event_loop().unwrap();
}
