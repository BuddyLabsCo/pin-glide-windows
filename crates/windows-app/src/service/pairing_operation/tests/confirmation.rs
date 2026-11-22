use std::{fs, path::PathBuf};

use aes_gcm::{Aes256Gcm, KeyInit, Nonce, aead::Aead};
use base64::{Engine, engine::general_purpose::STANDARD};
use hpke::{Deserializable, Kem, OpModeS, PskBundle, Serializable};
use receiver_core::pairing::PairingMaterial;
use tempfile::tempdir;
use tokio_tungstenite::tungstenite::WebSocket;

use super::*;
use crate::{service::pairing_operation::drive_session, storage::RegistrationStore};

#[tokio::test]
async fn reports_only_after_the_provisional_key_is_saved_and_preserves_the_terminal_outcome() {
    for store_is_writable in [false, true] {
        let directory = tempdir().unwrap();
        let path = directory.path().join("registration.json");
        if !store_is_writable {
            fs::write(&path, br#"{"version":99}"#).unwrap();
        }
        let store = RegistrationStore::new(path.clone());
        let material = PairingMaterial::generate().unwrap();
        let qr =
            serde_json::from_str(&material.qr_payload("session-1", "receiver-1").unwrap()).unwrap();
        let (service, server) = confirmation_service(qr, path.clone(), store_is_writable);
        let (updates, progress) = progress_channel();
        let (_stop, stopping) = oneshot::channel();
        drive_session(
            &service,
            &updates,
            stopping,
            || clock(START),
            material,
            Some(&store),
        )
        .await;
        let expected = if store_is_writable {
            PairingOutcome::KeyConfirmed
        } else {
            assert_eq!(fs::read(&path).unwrap(), br#"{"version":99}"#);
            PairingOutcome::Failed
        };
        assert!(
            matches!(*progress.borrow(), PairingProgress::Finished(result) if result == expected)
        );
        server.join().unwrap();
    }
}

fn confirmation_service(
    qr: Value,
    path: PathBuf,
    writable: bool,
) -> (Service, std::thread::JoinHandle<()>) {
    let (listener, origin) = listen("localhost");
    let service = local_service(&listener, &origin, true);
    let server = std::thread::spawn(move || {
        let acceptor = acceptor();
        accept_creation(acceptor.accept(accept_loopback(&listener)).unwrap());
        let mut socket = accept_events(
            acceptor.accept(accept_loopback(&listener)).unwrap(),
            "/v1/pairing-sessions/session-1/events",
        );
        let mut claimed = snapshot("awaiting_confirmation");
        claimed["pairingId"] = json!("pairing-1");
        claimed["senderId"] = json!("sender-1");
        send_state(&mut socket, claimed.clone());
        let key = sender_handshake(&mut socket, &qr);
        let mut complete = BufReader::new(acceptor.accept(accept_loopback(&listener)).unwrap());
        let (request, headers, body) = read_request_parts(&mut complete);
        assert!(headers.iter().any(|header| {
            header.split_once(':').is_some_and(|(name, value)| {
                name.eq_ignore_ascii_case("authorization")
                    && value.trim() == format!("Bearer {TOKEN}")
            })
        }));
        if !writable {
            assert_eq!(
                request,
                "DELETE /v1/pairing-sessions/session-1 HTTP/1.1\r\n"
            );
            complete
                .get_mut()
                .write_all(b"HTTP/1.1 204 No Content\r\nConnection: close\r\n\r\n")
                .unwrap();
            return;
        }
        assert_eq!(
            request,
            "POST /v1/pairing-sessions/session-1/complete HTTP/1.1\r\n"
        );
        assert_eq!(
            serde_json::from_slice::<Value>(&body).unwrap(),
            json!({"keyConfirmation":"confirmed"})
        );
        let stored: Value = serde_json::from_slice(&fs::read(path).unwrap()).unwrap();
        let pending = &stored["provisional_pairing"];
        assert_eq!(pending["pairing_id"], "pairing-1");
        assert_eq!(pending["sender_id"], "sender-1");
        assert_eq!(pending["receiver_id"], "receiver-1");
        assert_ne!(pending["protected_key"], json!(key));
        assert!(pending["protected_key"].as_array().unwrap().len() > key.len());
        claimed["receiverConfirmed"] = json!(true);
        write_response(complete.get_mut(), "200 OK", &claimed);
        claimed["senderConfirmed"] = json!(true);
        claimed["state"] = json!("credential_ready");
        send_state(&mut socket, claimed);
        let _ = socket.close(Some(CloseFrame {
            code: CloseCode::Policy,
            reason: "fixture complete".into(),
        }));
    });
    (service, server)
}

fn sender_handshake(socket: &mut WebSocket<impl Read + Write>, qr: &Value) -> [u8; 32] {
    type KemType = hpke::kem::DhP256HkdfSha256;
    let decode = |field: &Value| STANDARD.decode(field.as_str().unwrap()).unwrap();
    let public = decode(&qr["recipientPublicKey"]);
    let psk = decode(&qr["psk"]);
    let (enc, mut sender) =
        hpke::setup_sender::<hpke::aead::AesGcm256, hpke::kdf::HkdfSha256, KemType>(
            &OpModeS::Psk(PskBundle::new(&psk, b"session-1").unwrap()),
            &<KemType as Kem>::PublicKey::from_bytes(&public).unwrap(),
            b"pin-glide/pairing/v1",
        )
        .unwrap();
    let challenge = STANDARD.encode([27; 32]);
    let init = json!({"type":"pairing_init","challengeA":challenge}).to_string();
    send_relay(
        socket,
        json!({
            "kind":"hpke_init", "cryptoVersion":1, "enc":STANDARD.encode(enc.to_bytes()),
            "ciphertext":STANDARD.encode(sender.seal(init.as_bytes(), b"").unwrap()),
        }),
    );
    let reply: Value = serde_json::from_str(socket.read().unwrap().to_text().unwrap()).unwrap();
    assert_eq!(reply["type"], "pairing_relay");
    assert_eq!(reply["version"], 1);
    assert_eq!(reply["sessionId"], "session-1");
    let payload: Value = serde_json::from_slice(&decode(&reply["payload"])).unwrap();
    let mut key = [0; 32];
    sender
        .export(b"pin-glide/message-key/v1", &mut key)
        .unwrap();
    let cipher = Aes256Gcm::new((&key).into());
    let nonce = <[u8; 12]>::try_from(decode(&payload["nonce"])).unwrap();
    let plaintext: Value = serde_json::from_slice(
        &cipher
            .decrypt(
                &Nonce::from(nonce),
                decode(&payload["ciphertext"]).as_slice(),
            )
            .unwrap(),
    )
    .unwrap();
    assert_eq!(plaintext["challengeA"], challenge);
    let confirmation = json!({
        "type":"key_confirmation", "role":"sender", "challengeW":plaintext["challengeW"],
    })
    .to_string();
    let nonce = Nonce::from([34; 12]);
    send_relay(
        socket,
        json!({
            "kind":"key_confirmation", "cryptoVersion":1, "nonce":STANDARD.encode(nonce),
            "ciphertext":STANDARD.encode(cipher.encrypt(&nonce, confirmation.as_bytes()).unwrap()),
        }),
    );
    key
}

fn send_relay(socket: &mut WebSocket<impl Read + Write>, payload: Value) {
    send_event(
        socket,
        json!({
            "type":"pairing_relay", "version":1, "sessionId":"session-1", "side":"sender",
            "payload":STANDARD.encode(payload.to_string()),
        }),
    );
}
