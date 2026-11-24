use std::{fs, os::windows::fs::OpenOptionsExt, sync::mpsc, thread, time::Duration};

use serde_json::json;
use tokio_tungstenite::tungstenite::{Message, protocol::frame::coding::CloseCode};
use windows_app::service::{DeliveryStatus, PairingOperation, PairingStart};

use super::{dpapi, fixture::*, support::wait_for};

const KEY: [u8; 32] = [11; 32];
const LIMIT: Duration = Duration::from_secs(5);

#[test]
fn mismatched_sender_does_not_remove_the_pairing_or_its_current_code() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("registration.json");
    let original = registration();
    fs::write(&path, &original).unwrap();
    let (listener, service) = listen();
    let (proceed, proceed_received) = mpsc::channel();
    let server = thread::spawn(move || {
        let tls = acceptor();
        let mut socket = accept_delivery(accept(&tls, &listener));
        send_code(&mut socket, "123456", 1);
        proceed_received.recv_timeout(LIMIT).unwrap();
        send_event(
            &mut socket,
            json!({"type":"pairing_removed","version":1,"pairingId":"pairing-1","senderId":"other-sender"}),
        );
        assert!(
            matches!(socket.read().unwrap(), Message::Close(Some(frame)) if frame.code == CloseCode::Protocol)
        );
        proceed_received.recv_timeout(LIMIT).unwrap();
        let mut next = accept_delivery(accept(&tls, &listener));
        send_code(&mut next, "654321", 2);
    });
    let operation = PairingOperation::start_with_registration(
        move || Some(service),
        || {},
        path.clone(),
        PairingStart::Registered,
    );
    let current = |expected| {
        operation
            .latest_otp()
            .is_some_and(|otp| otp.as_str() == expected)
    };
    wait_for(|| current("123456"), "the first code was authenticated");
    proceed.send(()).unwrap();
    wait_for(
        || operation.delivery_status() == DeliveryStatus::Protocol,
        "conflicting Sender identity triggers a protocol close",
    );
    assert!(current("123456"));
    assert_eq!(fs::read_to_string(&path).unwrap(), original);
    assert!(!path.with_extension("cleanup.json").exists());
    proceed.send(()).unwrap();
    wait_for(|| current("654321"), "the Pairing can still deliver");
    server.join().unwrap();
    operation.shutdown();
}

#[test]
fn immediate_shutdown_journals_revocation_before_the_previous_cleanup_can_retry() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("registration.json");
    let journal = path.with_extension("cleanup.json");
    let original = registration();
    fs::write(&path, &original).unwrap();
    let locked = fs::OpenOptions::new()
        .read(true)
        .share_mode(1)
        .open(&path)
        .unwrap();
    let (listener, service) = listen();
    let (revoke, revoking) = mpsc::channel();
    let server = thread::spawn(move || {
        let mut socket = accept_delivery(accept(&acceptor(), &listener));
        send_event(
            &mut socket,
            json!({"type":"pairing_removed","version":1,"pairingId":"pairing-1","senderId":"sender-1"}),
        );
        revoking.recv_timeout(LIMIT).unwrap();
        send_event(
            &mut socket,
            json!({"type":"device_revoked","version":1,"receiverId":"receiver-1"}),
        );
        assert_peer_left(&mut socket);
    });
    let operation = PairingOperation::start_with_registration(
        move || Some(service),
        || {},
        path.clone(),
        PairingStart::Registered,
    );
    let journal_contains =
        |kind| fs::read_to_string(&journal).is_ok_and(|text| text.contains(kind));
    wait_for(
        || journal_contains("PairingRemoved"),
        "the failed write leaves a removal to retry",
    );
    let locked_journal = fs::OpenOptions::new()
        .read(true)
        .share_mode(1)
        .open(&journal)
        .unwrap();
    revoke.send(()).unwrap();
    wait_for(
        || operation.delivery_status() == DeliveryStatus::Revoked,
        "revocation was accepted",
    );
    assert!(
        !journal_contains("Revoked"),
        "only the earlier removal is durable at exit"
    );
    drop(locked_journal);
    operation.shutdown();
    assert!(
        journal_contains("Revoked"),
        "normal shutdown must flush the accepted revocation"
    );
    assert_eq!(fs::read_to_string(&path).unwrap(), original);
    server.join().unwrap();

    let restarted = tempfile::tempdir().unwrap();
    let reopened_path = restarted.path().join("registration.json");
    fs::copy(&path, &reopened_path).unwrap();
    fs::copy(&journal, reopened_path.with_extension("cleanup.json")).unwrap();
    let reopened = PairingOperation::start_with_registration(
        || panic!("startup used the revoked Receiver credential"),
        || {},
        reopened_path.clone(),
        PairingStart::Registered,
    );
    wait_for(
        || reopened.delivery_status() == DeliveryStatus::Revoked,
        "restart sees revocation",
    );
    reopened.shutdown();
    drop(locked);
    for stored in [&path, &reopened_path] {
        wait_for(
            || !stored.with_extension("cleanup.json").exists(),
            "the revoked registration eventually replaces the old file",
        );
    }
}

fn registration() -> String {
    json!({
        "version":4, "receiver_id":"receiver-1",
        "protected_receiver_token":dpapi::protect(RECEIVER_TOKEN.as_bytes()),
        "active_pairings": {
            "pairing-1": {"sender_id":"sender-1","protected_key":dpapi::protect(&KEY),"name":"Fixture"},
        },
    }).to_string()
}

fn send_code(socket: &mut Socket, code: &str, sequence: u8) {
    send_authenticated_delivery(
        socket,
        "pairing-1",
        "sender-1",
        &KEY,
        (code, sequence),
        json!({
            "type":"verification_message", "smsText":"Synthetic SMS", "otp":code,
        }),
    );
    let receipt: serde_json::Value =
        serde_json::from_str(socket.read().unwrap().to_text().unwrap()).unwrap();
    assert_eq!(receipt["messageId"], code);
}
