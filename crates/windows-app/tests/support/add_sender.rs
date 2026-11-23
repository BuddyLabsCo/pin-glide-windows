use std::{fs, io::BufReader, sync::mpsc};

use aes_gcm::{Aes256Gcm, KeyInit, Nonce, aead::Aead};
use base64::{Engine, engine::general_purpose::STANDARD};
use serde_json::{Value, json};
use tempfile::tempdir;
use tokio_tungstenite::tungstenite::protocol::{CloseFrame, frame::coding::CloseCode};
use windows_app::service::{DeliveryStatus, PairingOperation, PairingProgress, PairingStart};

use super::{dpapi, fixture::*, support::wait_for};

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
        if existing {
            deliver(&mut live, "old", &old_key, "123456", 21);
        }
        let qr = qr_in.recv().unwrap();
        let mut claimed = claim(initial);
        claimed["pairingId"] = json!("new-pairing");
        claimed["senderId"] = json!("new-sender");
        send_state(&mut events, &claimed);
        let new_key = sender_handshake(&mut events, &qr);
        let mut completed = accept_completion(accept(&tls, &listener));
        claimed["receiverConfirmed"] = json!(true);
        write_response(completed.get_mut(), "200 OK", &claimed);
        claimed["senderConfirmed"] = json!(true);
        claimed["state"] = json!("completed");
        send_state(&mut events, claimed);
        events
            .close(Some(CloseFrame {
                code: CloseCode::Policy,
                reason: "pairing_session_closed".into(),
            }))
            .unwrap();
        (live, new_key)
    });
    let delivery_service = service.clone();
    let delivery = PairingOperation::start_with_registration(
        move || Some(delivery_service),
        || {},
        path.clone(),
        PairingStart::Registered,
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
    wait_for(
        || matches!(add.current(), PairingProgress::Completed { pairing_id } if pairing_id == "new-pairing"),
        "add Sender did not complete",
    );
    let (mut live, new_key) = server.join().unwrap();
    assert_eq!(delivery.delivery_status(), DeliveryStatus::Ready);
    if existing {
        wait_for(
            || delivery.latest_otp().as_deref().map(String::as_str) == Some("123456"),
            "existing Sender stopped delivering",
        );
    }
    deliver(&mut live, "new", &new_key, "654321", 22);
    wait_for(
        || delivery.latest_otp().as_deref().map(String::as_str) == Some("654321"),
        "new Sender cannot deliver on the existing connection",
    );
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
    let directory = tempdir().unwrap();
    let path = directory.path().join("registration.json");
    let original = json!({
        "version":4, "receiver_id":"receiver-1",
        "protected_receiver_token":dpapi::protect(RECEIVER_TOKEN.as_bytes()),
        "active_pairings":{"old-pairing":{
            "sender_id":"sender-1", "name":"Original", "protected_key":dpapi::protect(&[21; 32])
        }},
        "provisional_pairing":null
    });
    fs::write(&path, original.to_string()).unwrap();
    let (listener, service) = listen();
    let (qr_out, qr_in) = mpsc::channel();
    let server = std::thread::spawn(move || {
        let tls = acceptor();
        let initial = accept_add_creation(accept(&tls, &listener));
        let mut events = accept_events(accept(&tls, &listener), "add-1");
        send_state(&mut events, &initial);
        let qr = qr_in.recv().unwrap();
        send_state(&mut events, claim(initial));
        sender_handshake(&mut events, &qr);
        assert_delete(accept(&tls, &listener), "add-1");
    });
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
        "duplicate test did not receive a QR",
    );
    wait_for(
        || {
            matches!(
                add.current(),
                PairingProgress::Finished(windows_app::service::PairingOutcome::Failed)
            )
        },
        "duplicate Sender succeeded",
    );
    server.join().unwrap();
    add.shutdown();
    assert_eq!(fs::read_to_string(path).unwrap(), original.to_string());
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

fn accept_completion(connection: Connection) -> BufReader<Connection> {
    let mut stream = BufReader::new(connection);
    let (request, headers, body) = read_request_parts(&mut stream);
    assert_eq!(
        request,
        "POST /v1/pairing-sessions/add-1/complete HTTP/1.1\r\n"
    );
    assert_bearer(&headers);
    assert_eq!(
        serde_json::from_slice::<Value>(&body).unwrap(),
        json!({"keyConfirmation":"confirmed"})
    );
    stream
}

fn deliver(socket: &mut Socket, sender: &str, key: &[u8; 32], otp: &str, nonce: u8) {
    let cipher = Aes256Gcm::new(key.into());
    let nonce = [nonce; 12];
    let plaintext = json!({"type":"verification_message", "smsText":"Synthetic SMS", "otp":otp});
    let ciphertext = cipher
        .encrypt(&Nonce::from(nonce), plaintext.to_string().as_bytes())
        .unwrap();
    send_event(
        socket,
        json!({
            "type":"delivery", "version":1, "receiverId":"receiver-1",
            "senderId":format!("{sender}-sender"), "pairingId":format!("{sender}-pairing"),
            "messageId":sender, "cryptoVersion":1,
            "nonce":STANDARD.encode(nonce), "ciphertext":STANDARD.encode(ciphertext),
        }),
    );
    let receipt: Value = serde_json::from_str(socket.read().unwrap().to_text().unwrap()).unwrap();
    assert_eq!(
        receipt,
        json!({"type":"receipt", "version":1, "senderId":format!("{sender}-sender"), "messageId":sender})
    );
}
