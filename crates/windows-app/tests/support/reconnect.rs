use std::{io::BufReader, sync::mpsc, thread, time::Duration};

use serde_json::json;
use tokio_tungstenite::tungstenite::{Message, protocol::frame::coding::CloseCode};
use windows_app::service::{DeliveryStatus, PairingOperation, PairingStart};

use super::{dpapi, fixture::*};

#[path = "mod.rs"]
mod support;

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
