use std::{fs, io::BufReader, sync::mpsc, time::Duration};

use serde_json::{Value, json};
use tempfile::tempdir;
use tokio_tungstenite::tungstenite::protocol::{CloseFrame, frame::coding::CloseCode};
use windows_app::service::{DeliveryStatus, PairingOperation, PairingProgress, PairingStart};

use super::{dpapi, fixture::*, support::wait_for};

const WAIT: Duration = Duration::from_secs(5);
const HELD_CAPACITY: u8 = 32;

#[test]
fn adds_a_sender_on_the_existing_receiver_and_delivery_connection() {
    exercise_add_sender(true);
}

#[test]
fn adds_the_first_local_sender_to_an_already_connected_empty_receiver() {
    exercise_add_sender(false);
}

fn exercise_add_sender(existing: bool) {
    let directory = tempdir().unwrap();
    let path = directory.path().join("registration.json");
    let old_key = [21; 32];
    let mut original = json!({
        "version":4, "receiver_id":"receiver-1",
        "protected_receiver_token":dpapi::protect(RECEIVER_TOKEN.as_bytes()),
        "active_pairings":{"old-pairing":{
            "sender_id":"old-sender", "name":"未命名设备", "protected_key":dpapi::protect(&old_key)
        }},
        "provisional_pairing":null
    });
    if !existing {
        original["active_pairings"] = json!({});
    }
    fs::write(&path, original.to_string()).unwrap();
    let (listener, service) = listen();
    let (qr_out, qr_in) = mpsc::channel();
    let (online, connected) = mpsc::channel();
    let (advance, release) = mpsc::channel();
    let (sms, received) = mpsc::channel();
    let held = if existing { HELD_CAPACITY } else { 1 };
    let stored_path = path.clone();
    let expected = original.clone();
    let server = std::thread::spawn(move || {
        let tls = acceptor();
        let mut live = accept_delivery(accept(&tls, &listener));
        live.send(tokio_tungstenite::tungstenite::Message::Ping(
            vec![9].into(),
        ))
        .unwrap();
        assert!(live.read().unwrap().is_pong());
        online.send(()).unwrap();
        let initial = accept_add_creation(accept(&tls, &listener));
        let mut events = accept_events(accept(&tls, &listener), "add-1");
        send_state(&mut events, &initial);
        let qr = qr_in.recv().unwrap();
        let mut claimed = claim(initial);
        claimed["pairingId"] = json!("new-pairing");
        claimed["senderId"] = json!("new-sender");
        send_state(&mut events, &claimed);
        let new_key = sender_handshake(&mut events, &qr);
        let mut completed = accept_completion(accept(&tls, &listener), "add-1");
        let stored: Value = serde_json::from_slice(&fs::read(stored_path).unwrap()).unwrap();
        assert_eq!(stored["receiver_id"], expected["receiver_id"]);
        assert_eq!(
            stored["protected_receiver_token"],
            expected["protected_receiver_token"]
        );
        assert_eq!(stored["active_pairings"], expected["active_pairings"]);
        let pending = &stored["provisional_pairing"];
        assert_eq!(pending["session_id"], "add-1");
        assert_eq!(pending["receiver_id"], "receiver-1");
        assert_eq!(pending["pairing_id"], "new-pairing");
        assert_eq!(pending["sender_id"], "new-sender");
        assert_eq!(dpapi::unprotect(&pending["protected_key"]), new_key);
        claimed["receiverConfirmed"] = json!(true);
        write_response(completed.get_mut(), "200 OK", &claimed);
        for sequence in 1..=held {
            deliver(&mut live, "new", "new", &new_key, &otp(sequence), sequence);
        }
        if existing {
            for sequence in held + 1..=held + 40 {
                send_delivery(&mut live, "new", "new", &new_key, &otp(sequence), sequence);
            }
            deliver(&mut live, "old", "old", &old_key, "123456", 21);
        }
        release.recv_timeout(WAIT).unwrap();
        claimed["senderConfirmed"] = json!(true);
        claimed["state"] = json!("completed");
        send_state(&mut events, claimed);
        events
            .close(Some(CloseFrame {
                code: CloseCode::Policy,
                reason: "pairing_session_closed".into(),
            }))
            .unwrap();
        release.recv_timeout(WAIT).unwrap();
        deliver(&mut live, "new", "new", &new_key, "987654", 90);
        (live, new_key)
    });
    let delivery_service = service.clone();
    let delivery = PairingOperation::start_receiving_with_registration(
        move || Some(delivery_service),
        path.clone(),
        move |message| sms.send(message.otp).unwrap(),
    );
    connected
        .recv_timeout(std::time::Duration::from_secs(5))
        .unwrap();
    assert_eq!(
        delivery.delivery_status(),
        if existing {
            DeliveryStatus::Ready
        } else {
            DeliveryStatus::Unregistered
        }
    );
    let add = PairingOperation::start_with_registration(
        move || Some(service),
        || {},
        path.clone(),
        PairingStart::AddSender,
    );
    wait_for(
        || {
            let PairingProgress::Waiting { code, .. } = add.current() else {
                return false;
            };
            let qr = decode_qr(&code);
            assert_eq!(qr["receiverId"], "receiver-1");
            assert_eq!(qr["sessionId"], "add-1");
            qr_out.send(qr).unwrap();
            true
        },
        "add Sender never offered a QR",
    );
    if existing {
        assert_eq!(
            received
                .recv_timeout(WAIT)
                .expect("active Sender's SMS while pending buffer is full"),
            "123456"
        );
    }
    assert_eq!(received.try_recv(), Err(mpsc::TryRecvError::Empty));
    advance.send(()).unwrap();
    wait_for(
        || matches!(add.current(), PairingProgress::Completed { pairing_id } if pairing_id == "new-pairing"),
        "add Sender did not complete",
    );
    advance.send(()).unwrap();
    for sequence in 1..=held {
        assert_eq!(received.recv_timeout(WAIT).unwrap(), otp(sequence));
    }
    assert_eq!(received.recv_timeout(WAIT).unwrap(), "987654");
    let (_live, new_key) = server.join().unwrap();
    assert_eq!(delivery.delivery_status(), DeliveryStatus::Ready);
    let stored: Value = serde_json::from_slice(&fs::read(path).unwrap()).unwrap();
    assert_eq!(
        stored["protected_receiver_token"],
        original["protected_receiver_token"]
    );
    assert_eq!(
        stored["active_pairings"]["old-pairing"],
        original["active_pairings"]["old-pairing"]
    );
    assert_eq!(
        stored["active_pairings"]["new-pairing"]["name"],
        if existing {
            "未命名设备-2"
        } else {
            "未命名设备"
        }
    );
    assert_eq!(
        dpapi::unprotect(&stored["active_pairings"]["new-pairing"]["protected_key"]),
        new_key
    );
    assert!(stored["provisional_pairing"].is_null());
    assert!(stored["enrollment"].is_null());
    add.shutdown();
    delivery.shutdown();
}

#[test]
fn duplicate_sender_cancels_with_b_and_preserves_the_saved_association() {
    rejected_add_sender(true);
}

#[test]
fn failed_confirmation_discards_staged_delivery_without_interrupting_the_existing_sender() {
    rejected_add_sender(false);
}

fn rejected_add_sender(duplicate: bool) {
    let directory = tempdir().unwrap();
    let path = directory.path().join("registration.json");
    let old_key = [21; 32];
    let original = json!({
        "version":4, "receiver_id":"receiver-1",
        "protected_receiver_token":dpapi::protect(RECEIVER_TOKEN.as_bytes()),
        "active_pairings":{"old-pairing":{
            "sender_id":"old-sender", "name":"Original", "protected_key":dpapi::protect(&old_key)
        }},
        "provisional_pairing":null
    });
    fs::write(&path, original.to_string()).unwrap();
    let (listener, service) = listen();
    let (qr_out, qr_in) = mpsc::channel();
    let server = std::thread::spawn(move || {
        let tls = acceptor();
        let mut live = accept_delivery(accept(&tls, &listener));
        let initial = accept_add_creation(accept(&tls, &listener));
        let mut events = accept_events(accept(&tls, &listener), "add-1");
        send_state(&mut events, &initial);
        let qr = qr_in.recv_timeout(Duration::from_secs(5)).unwrap();
        let mut claimed = claim(initial);
        claimed["pairingId"] = json!("new-pairing");
        claimed["senderId"] = if duplicate {
            json!("old-sender")
        } else {
            json!("new-sender")
        };
        send_state(&mut events, &claimed);
        let pending_key = sender_handshake(&mut events, &qr);
        if !duplicate {
            let mut report = accept_completion(accept(&tls, &listener), "add-1");
            claimed["receiverConfirmed"] = json!(true);
            write_response(report.get_mut(), "200 OK", &claimed);
            deliver(
                &mut live,
                "new",
                "new",
                &pending_key,
                "should-be-discarded",
                22,
            );
            events
                .close(Some(CloseFrame {
                    code: CloseCode::Policy,
                    reason: "pairing_session_closed".into(),
                }))
                .unwrap();
        }
        assert_delete(accept(&tls, &listener), "add-1");
        deliver(&mut live, "old", "old", &old_key, "123456", 21);
        live
    });
    let delivery_service = service.clone();
    let delivery = PairingOperation::start_with_registration(
        move || Some(delivery_service),
        || {},
        path.clone(),
        PairingStart::Registered,
    );
    wait_for(
        || delivery.delivery_status() == DeliveryStatus::Ready,
        "old Sender never connected",
    );
    let add = PairingOperation::start_with_registration(
        move || Some(service),
        || {},
        path.clone(),
        PairingStart::AddSender,
    );
    wait_for(
        || {
            let PairingProgress::Waiting { code, .. } = add.current() else {
                return false;
            };
            qr_out.send(decode_qr(&code)).unwrap();
            true
        },
        "rejected pairing did not receive a QR",
    );
    wait_for(
        || {
            matches!(
                add.current(),
                PairingProgress::Finished(windows_app::service::PairingOutcome::Failed)
            )
        },
        "rejected pairing succeeded",
    );
    let _live = server.join().unwrap();
    wait_for(
        || delivery.latest_otp().as_deref().map(String::as_str) == Some("123456"),
        "old Sender stopped delivering or rejected Sender updated the inbox",
    );
    add.shutdown();
    delivery.shutdown();
    assert_eq!(
        serde_json::from_slice::<Value>(&fs::read(path).unwrap()).unwrap(),
        original
    );
}

fn accept_add_creation(connection: Connection) -> Value {
    let mut stream = BufReader::new(connection);
    let (request, headers, body) = read_request_parts(&mut stream);
    assert_eq!(request, "POST /v1/pairing-sessions HTTP/1.1\r\n");
    assert!(headers.iter().any(
        |header| header.split_once(':').is_some_and(|(name, value)| {
            name.eq_ignore_ascii_case("authorization")
                && value.trim() == format!("Bearer {RECEIVER_TOKEN}")
        })
    ));
    assert_eq!(
        serde_json::from_slice::<Value>(&body).unwrap(),
        json!({
            "mode":"add_sender", "receiverId":"receiver-1"
        })
    );
    let mut initial = snapshot("add-1", "awaiting_authorization", 300);
    initial["mode"] = json!("add_sender");
    write_response(
        stream.get_mut(),
        "201 Created",
        &json!({
            "session":initial, "pairingToken":TOKEN, "pairingTokenExpiresAt":initial["expiresAt"]
        }),
    );
    initial
}

fn otp(sequence: u8) -> String {
    format!("{sequence:06}")
}

fn deliver(socket: &mut Socket, pairing: &str, sender: &str, key: &[u8; 32], otp: &str, nonce: u8) {
    send_delivery(socket, pairing, sender, key, otp, nonce);
    let receipt: Value = serde_json::from_str(socket.read().unwrap().to_text().unwrap()).unwrap();
    assert_eq!(
        receipt,
        json!({"type":"receipt", "version":1, "senderId":format!("{sender}-sender"), "messageId":format!("{sender}-{nonce}")})
    );
}

fn send_delivery(
    socket: &mut Socket,
    pairing: &str,
    sender: &str,
    key: &[u8; 32],
    otp: &str,
    sequence: u8,
) {
    let received_at = if pairing == "new" {
        "2026-01-01T00:01:00Z"
    } else {
        "2026-01-01T00:00:00Z"
    };
    let plaintext = json!({"type":"verification_message", "smsText":"Synthetic SMS", "otp":otp, "receivedAt":received_at});
    send_authenticated_delivery(
        socket,
        &format!("{pairing}-pairing"),
        &format!("{sender}-sender"),
        key,
        (&format!("{sender}-{sequence}"), sequence),
        plaintext,
    );
}
