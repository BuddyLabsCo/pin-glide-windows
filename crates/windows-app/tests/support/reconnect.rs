use std::{io::BufReader, sync::mpsc, thread, time::Duration};

use serde_json::json;
use tokio_tungstenite::tungstenite::{Message, protocol::frame::coding::CloseCode};
use windows_app::service::{DeliveryStatus, PairingOperation, PairingStart};

use super::{dpapi, fixture::*, support};

#[test]
fn authoritative_removal_keeps_the_socket_and_revocation_clears_registration() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("registration.json");
    let key = dpapi::protect(&[11; 32]);
    std::fs::write(
        &path,
        json!({
            "version": 4, "receiver_id": "receiver-1",
            "protected_receiver_token": dpapi::protect(RECEIVER_TOKEN.as_bytes()),
            "active_pairings": {
                "pairing-1": {"sender_id": "sender-1", "protected_key": key},
                "pairing-2": {"sender_id": "sender-2", "protected_key": key},
            },
            "provisional_pairing": {
                "session_id": "session-add", "receiver_id": "receiver-1",
                "pairing_id": "pairing-add", "sender_id": "sender-add", "protected_key": key,
            }
        })
        .to_string(),
    )
    .unwrap();
    let (listener, service) = listen();
    let (revoke, revoking) = mpsc::channel();
    let server = thread::spawn(move || {
        let mut socket = accept_delivery(accept(&acceptor(), &listener));
        send_event(
            &mut socket,
            json!({"type":"pairing_removed","version":1,"pairingId":"pairing-1","senderId":"sender-1"}),
        );
        socket.send(Message::Ping(vec![23].into())).unwrap();
        assert_eq!(socket.read().unwrap(), Message::Pong(vec![23].into()));
        revoking.recv_timeout(Duration::from_secs(8)).unwrap();
        send_event(
            &mut socket,
            json!({"type":"device_revoked","version":1,"receiverId":"receiver-1"}),
        );
        assert!(socket.read().is_err());
    });
    let operation = PairingOperation::start_with_registration(
        move || Some(service),
        || {},
        path.clone(),
        PairingStart::Registered,
    );
    let read =
        || serde_json::from_slice::<serde_json::Value>(&std::fs::read(&path).unwrap()).unwrap();
    support::wait_for(
        || read()["active_pairings"]["pairing-1"].is_null(),
        "The removed Pairing remained on disk",
    );
    assert_eq!(operation.delivery_status(), DeliveryStatus::Ready);
    assert_eq!(
        read()["active_pairings"]["pairing-2"]["sender_id"],
        "sender-2"
    );
    revoke.send(()).unwrap();
    support::wait_for(
        || read()["receiver_id"].is_null(),
        "Revocation retained the Receiver identity",
    );
    let stored = read();
    assert!(stored["protected_receiver_token"].is_null());
    assert_eq!(stored["receiver_revoked"], true);
    assert!(stored["active_pairings"].as_object().unwrap().is_empty());
    assert!(stored["provisional_pairing"].is_null());
    assert!(stored["enrollment"].is_null());
    support::wait_for(
        || operation.delivery_status() == DeliveryStatus::Revoked,
        "Revocation did not stop the Receiver",
    );
    operation.shutdown();
    server.join().unwrap();
    let restarted = PairingOperation::start_with_registration(
        || panic!("a revoked Receiver attempted to connect"),
        || {},
        path,
        PairingStart::Registered,
    );
    support::wait_for(
        || restarted.delivery_status() == DeliveryStatus::Revoked,
        "Revocation reason was lost on restart",
    );
    restarted.shutdown();
}

#[test]
fn unusable_local_credentials_require_explicit_replacement_and_keep_the_old_pairings() {
    use windows_app::service::{PairingOutcome, PairingProgress};

    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("registration.json");
    for credential in [json!(null), json!([1])] {
        let saved = json!({
            "version": 4, "receiver_id": "old-receiver",
            "protected_receiver_token": credential,
            "active_pairings": {
                "old-pairing": {"sender_id": "old-sender", "protected_key": dpapi::protect(&[11; 32]), "name": "Old phone"}
            }
        })
        .to_string();
        std::fs::write(&path, &saved).unwrap();
        let startup = PairingOperation::start_with_registration(
            || panic!("startup tried automatic registration with unusable R"),
            || {},
            path.clone(),
            PairingStart::Registered,
        );
        support::wait_for(
            || startup.delivery_status() == DeliveryStatus::MissingCredential,
            "The missing local credential reason was not reported",
        );
        startup.shutdown();

        let (listener, service) = listen();
        let server = thread::spawn(move || {
            let mut connection = BufReader::new(accept(&acceptor(), &listener));
            let (request, headers, body) = read_request_parts(&mut connection);
            assert_eq!(request, "POST /v1/pairing-sessions HTTP/1.1\r\n");
            assert!(
                !headers
                    .iter()
                    .any(|header| header.to_lowercase().starts_with("authorization:"))
            );
            assert_eq!(
                serde_json::from_slice::<serde_json::Value>(&body).unwrap()["mode"],
                "new_receiver"
            );
            write_response(connection.get_mut(), "503 Service Unavailable", &json!({}));
        });
        let replacement = PairingOperation::start_with_registration(
            move || Some(service),
            || {},
            path.clone(),
            PairingStart::ResumePending,
        );
        support::wait_for(
            || {
                matches!(
                    replacement.current(),
                    PairingProgress::Finished(PairingOutcome::CreateFailed)
                )
            },
            "The replacement request did not report failure",
        );
        replacement.shutdown();
        server.join().unwrap();
        assert_eq!(std::fs::read_to_string(&path).unwrap(), saved);
    }
}

#[test]
fn startup_keeps_the_empty_receiver_connected_but_not_ready() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("registration.json");
    std::fs::write(
        &path,
        json!({
            "version": 4, "receiver_id": "receiver-1",
            "protected_receiver_token": dpapi::protect(RECEIVER_TOKEN.as_bytes()),
            "active_pairings": {},
        })
        .to_string(),
    )
    .unwrap();
    let (listener, service) = listen();
    let (pong, ready) = mpsc::channel();
    let (stop, stopped) = mpsc::channel();
    let server = thread::spawn(move || {
        let mut socket = accept_delivery(accept(&acceptor(), &listener));
        socket.send(Message::Ping(vec![17].into())).unwrap();
        assert_eq!(socket.read().unwrap(), Message::Pong(vec![17].into()));
        pong.send(()).unwrap();
        stopped.recv_timeout(Duration::from_secs(5)).unwrap();
    });
    let operation = PairingOperation::start_with_registration(
        move || Some(service),
        || {},
        path,
        PairingStart::Registered,
    );
    ready.recv_timeout(Duration::from_secs(5)).unwrap();
    assert_eq!(operation.delivery_status(), DeliveryStatus::Unregistered);
    operation.shutdown();
    stop.send(()).unwrap();
    server.join().unwrap();
}

#[test]
fn promotes_the_issued_credential_on_restart_after_the_pairing_deadline() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("registration.json");
    let (listener, service) = listen();
    let origin = format!(
        "https://localhost:{}",
        listener.local_addr().unwrap().port()
    );
    std::fs::write(
        &path,
        json!({
            "version": 4,
            "active_pairings": {},
            "provisional_pairing": {
                "session_id": "session-1", "receiver_id": "receiver-1",
                "pairing_id": "pairing-1", "sender_id": "sender-1",
                "protected_key": dpapi::protect(&[11; 32]),
            },
            "enrollment": {
                "origin": origin, "session_id": "session-1", "receiver_id": "receiver-1",
                "operation_id": "fixture-issue-16",
                "expires_at": {"secs_since_epoch": 1, "nanos_since_epoch": 0},
                "phase": {"Issued": dpapi::protect(RECEIVER_TOKEN.as_bytes())},
            },
        })
        .to_string(),
    )
    .unwrap();
    let (stop, stopped) = mpsc::channel();
    let server = thread::spawn(move || {
        let _socket = accept_delivery(accept(&acceptor(), &listener));
        stopped.recv_timeout(Duration::from_secs(5)).unwrap();
    });
    let operation = PairingOperation::start_with_registration(
        move || Some(service),
        || {},
        path.clone(),
        PairingStart::ResumePending,
    );
    support::wait_for(
        || operation.delivery_status() == DeliveryStatus::Ready,
        "Restart did not recover the accepted Receiver credential",
    );
    let stored: serde_json::Value = serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap();
    assert!(stored["enrollment"].is_null() && stored["provisional_pairing"].is_null());
    assert_eq!(
        stored["active_pairings"]["pairing-1"]["sender_id"],
        "sender-1"
    );
    operation.shutdown();
    stop.send(()).unwrap();
    server.join().unwrap();
}

pub(super) fn reject_occupied_slot(connection: Connection) {
    let mut occupied = BufReader::new(connection);
    let (request, headers, _) = read_request_parts(&mut occupied);
    assert_eq!(request, "GET /v1/receiver/events HTTP/1.1\r\n");
    assert!(headers.iter().any(|header| {
        header
            .trim()
            .eq_ignore_ascii_case(&format!("Authorization: Bearer {RECEIVER_TOKEN}"))
    }));
    write_response(
        occupied.get_mut(),
        "409 Conflict",
        &json!({"code":"receiver_already_connected"}),
    );
}

#[test]
fn registered_receiver_recovers_from_an_unsupported_frame() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("registration.json");
    std::fs::write(
        &path,
        json!({
            "version": 4,
            "receiver_id": "receiver-1",
            "protected_receiver_token": dpapi::protect(RECEIVER_TOKEN.as_bytes()),
            "active_pairings": {
                "pairing-1": {
                    "sender_id": "sender-1",
                    "protected_key": dpapi::protect(&[11; 32]),
                    "name": "Fixture phone"
                }
            }
        })
        .to_string(),
    )
    .unwrap();
    let (listener, service) = listen();
    let (send_unsupported, unsupported) = mpsc::channel();
    let (reconnected, connected) = mpsc::channel();
    let (stop, stopped) = mpsc::channel();
    let server = thread::spawn(move || {
        let acceptor = acceptor();
        let mut first = accept_delivery(accept(&acceptor, &listener));
        unsupported.recv_timeout(Duration::from_secs(5)).unwrap();
        first.send(Message::Binary(vec![1].into())).unwrap();
        assert!(matches!(
            first.read().unwrap(),
            Message::Close(Some(frame)) if frame.code == CloseCode::Protocol
        ));
        let mut next = accept_delivery(accept(&acceptor, &listener));
        next.send(Message::Ping(vec![42].into())).unwrap();
        assert_eq!(next.read().unwrap(), Message::Pong(vec![42].into()));
        reconnected.send(()).unwrap();
        stopped.recv_timeout(Duration::from_secs(5)).unwrap();
    });
    let operation = PairingOperation::start_with_registration(
        move || Some(service),
        || {},
        path,
        PairingStart::Registered,
    );
    support::wait_for(
        || operation.delivery_status() == DeliveryStatus::Ready,
        "Receiver did not become Ready",
    );
    send_unsupported.send(()).unwrap();
    support::wait_for(
        || operation.delivery_status() == DeliveryStatus::Protocol,
        "Receiver did not expose a protocol error",
    );
    connected.recv_timeout(Duration::from_secs(5)).unwrap();
    support::wait_for(
        || operation.delivery_status() == DeliveryStatus::Ready,
        "Receiver stayed Not Ready after reconnecting",
    );
    operation.shutdown();
    stop.send(()).unwrap();
    server.join().unwrap();
}
