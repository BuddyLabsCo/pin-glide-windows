use std::{
    sync::{
        atomic::{AtomicUsize, Ordering},
        mpsc,
    },
    thread,
    time::Duration,
};

use aes_gcm::{Aes256Gcm, KeyInit, Nonce, aead::Aead};
use base64::{Engine, engine::general_purpose::STANDARD};
use receiver_core::delivery::Sms;
use serde_json::{Value, json};
use slint::ComponentHandle;
use tokio_tungstenite::tungstenite::{Message, WebSocket};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    PBT_APMRESUMEAUTOMATIC, PBT_APMSUSPEND, SendMessageW, WM_POWERBROADCAST, WM_WTSSESSION_CHANGE,
    WTS_SESSION_LOCK, WTS_SESSION_UNLOCK,
};

use crate::{
    platform,
    service::{
        PairingOperation, ReceivedSms,
        tests::{accept_loopback, acceptor, listen, local_service, socket},
    },
    startup_tests::{select_test_backend, wait_for},
    storage::Enrollment,
    ui,
};

static QUEUED_POPUPS: AtomicUsize = AtomicUsize::new(0);
const TOKEN: &str = "fixture-receiver-r";

fn queue_popup(message: ReceivedSms) {
    ui::receive_sms(message);
    QUEUED_POPUPS.fetch_add(1, Ordering::Release);
}

#[test]
#[ignore = "requires an unlocked interactive Windows desktop"]
fn lock_immediately_closes_every_sms_and_unlock_only_shows_new_sms() {
    platform::enable_per_monitor_dpi().unwrap();
    select_test_backend().unwrap();
    let name: Vec<_> = "Pin Glide session fixture"
        .encode_utf16()
        .chain([0])
        .collect();
    let tray = platform::Tray::new(&name).unwrap();
    let handle = tray.window() as usize;
    let (listener, origin) = listen("localhost");
    let service = local_service(&listener, &origin, true);
    let fixture_dir = tempfile::tempdir().unwrap();
    let path = fixture_dir.path().join("registration.json");
    std::fs::write(&path, json!({
        "version": 4,
        "receiver_id": "receiver-1",
        "protected_receiver_token": Enrollment::protect_token(TOKEN).unwrap(),
        "active_pairings": {
            "pairing-1": {
                "sender_id": "sender-1", "protected_key": Enrollment::protect_token("abcdefghijklmnopqrstuvwxyz123456").unwrap(), "name": "我的设备",
            },
        },
    }).to_string()).unwrap();
    let (deliver, messages) = mpsc::channel();
    let (ack, receipts) = mpsc::channel();
    let server = thread::spawn(move || {
        let stream = acceptor().accept(accept_loopback(&listener)).unwrap();
        let mut socket = socket::accept_socket(stream, "/v1/receiver/events", TOKEN);
        while let Ok((sequence, otp)) = messages.recv() {
            deliver_sms(&mut socket, sequence, otp);
            ack.send(sequence).unwrap();
        }
    });
    let operation = PairingOperation::start_registered(
        move || Some(service),
        || {},
        crate::storage::RegistrationStore::new(path),
        queue_popup,
    );
    slint::spawn_local(async move {
        let first = show_sample(
            "【鹊信】这是一条测试短信，验证码 482913。请勿用于真实登录。",
            "482913",
        )
        .await
        .unwrap();
        let second = show_sample("【鹊信】您的包裹已送达前台。\n请于 20:00 前领取。", "")
            .await
            .unwrap();
        assert!(first.window().is_visible() && second.window().is_visible());
        let signal = |message, event| {
            // SAFETY: the test owns the live tray window on the current thread.
            unsafe { SendMessageW(handle as _, message, event as usize, 0) };
        };
        deliver.send((1, "111111")).unwrap();
        assert_eq!(receipts.recv_timeout(Duration::from_secs(5)).unwrap(), 1);
        wait_for(
            || QUEUED_POPUPS.load(Ordering::Acquire) == 1,
            "the Service popup was not queued before lock",
        );
        signal(WM_WTSSESSION_CHANGE, WTS_SESSION_LOCK);
        for popup in [&first, &second] {
            assert!(!popup.window().is_visible());
            assert!(popup.get_sms().is_empty() && popup.get_otp().is_empty());
        }
        deliver.send((2, "222222")).unwrap();
        assert_eq!(receipts.recv_timeout(Duration::from_secs(5)).unwrap(), 2);
        wait_for(
            || {
                operation
                    .latest_otp()
                    .is_some_and(|otp| otp.as_str() == "222222")
            },
            "locked delivery did not update the latest OTP",
        );
        assert_eq!(QUEUED_POPUPS.load(Ordering::Acquire), 1);
        signal(WM_POWERBROADCAST, PBT_APMSUSPEND);
        signal(WM_POWERBROADCAST, PBT_APMRESUMEAUTOMATIC);
        assert!(show("SMS received on resume while locked").await.is_none());
        signal(WM_WTSSESSION_CHANGE, WTS_SESSION_UNLOCK);
        assert!(!first.window().is_visible() && !second.window().is_visible());
        next_gui_turn().await;
        assert_eq!(ui::popup_count(), 0, "queued SMS reappeared after unlock");
        deliver.send((3, "333333")).unwrap();
        assert_eq!(receipts.recv_timeout(Duration::from_secs(5)).unwrap(), 3);
        wait_for(
            || QUEUED_POPUPS.load(Ordering::Acquire) == 2,
            "the new Service popup was not queued after unlock",
        );
        next_gui_turn().await;
        assert_eq!(ui::popup_count(), 1);
        ui::close_sms();
        drop(deliver);
        operation.shutdown();
        slint::quit_event_loop().unwrap();
    })
    .unwrap();
    slint::run_event_loop_until_quit().unwrap();
    server.join().unwrap();
    drop(tray);
}

fn deliver_sms(
    socket: &mut WebSocket<native_tls::TlsStream<std::net::TcpStream>>,
    sequence: u8,
    otp: &str,
) {
    let nonce = [sequence; 12];
    let cipher = Aes256Gcm::new_from_slice(b"abcdefghijklmnopqrstuvwxyz123456").unwrap();
    let sms = json!({
        "type": "verification_message", "smsText": "Synthetic SMS from the Service", "otp": otp,
    });
    let ciphertext = cipher
        .encrypt(&Nonce::from(nonce), sms.to_string().as_bytes())
        .unwrap();
    let message_id = sequence.to_string();
    socket
        .send(Message::text(
            json!({
                "version":1, "type":"delivery", "cryptoVersion":1, "nonce":STANDARD.encode(nonce),
                "ciphertext":STANDARD.encode(ciphertext), "messageId":message_id,
                "receiverId":"receiver-1", "pairingId":"pairing-1", "senderId":"sender-1",
            })
            .to_string(),
        ))
        .unwrap();
    let receipt: Value = loop {
        match socket.read().unwrap() {
            Message::Text(text) => break serde_json::from_str(&text).unwrap(),
            Message::Ping(_) => socket.flush().unwrap(),
            frame => panic!("unexpected Service fixture frame: {frame:?}"),
        }
    };
    assert_eq!(
        receipt,
        json!({
            "version":1,"type":"receipt","senderId":"sender-1","messageId":message_id,
        })
    );
}

async fn next_gui_turn() {
    let (send, mut ready) = tokio::sync::oneshot::channel();
    slint::Timer::single_shot(Duration::from_millis(60), move || {
        let _ = send.send(());
    });
    (&mut ready).await.unwrap();
}

async fn show(body: &str) -> Option<crate::SmsWindow> {
    show_sample(body, "123456").await
}

async fn show_sample(body: &str, otp: &str) -> Option<crate::SmsWindow> {
    ui::show_sms(
        Sms {
            body: body.into(),
            otp: otp.into(),
        },
        "我的设备".into(),
        true,
    )
    .await
    .unwrap()
}
