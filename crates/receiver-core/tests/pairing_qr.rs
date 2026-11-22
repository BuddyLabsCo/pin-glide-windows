use base64::{Engine, engine::general_purpose::STANDARD};
use p256::PublicKey;
use receiver_core::pairing::PairingMaterial;
use serde_json::Value;

#[test]
fn encodes_only_the_shared_v1_fields_and_rotates_handshake_material() {
    let first: Value = serde_json::from_str(
        &PairingMaterial::generate()
            .unwrap()
            .qr_payload("session-1", "receiver-1")
            .unwrap(),
    )
    .unwrap();
    let refreshed: Value = serde_json::from_str(
        &PairingMaterial::generate()
            .unwrap()
            .qr_payload("session-2", "receiver-2")
            .unwrap(),
    )
    .unwrap();

    for (payload, session, receiver) in [
        (&first, "session-1", "receiver-1"),
        (&refreshed, "session-2", "receiver-2"),
    ] {
        assert_eq!(payload.as_object().unwrap().len(), 6);
        assert_eq!(payload["version"], 1);
        assert_eq!(payload["cryptoVersion"], 1);
        assert_eq!(payload["sessionId"], session);
        assert_eq!(payload["receiverId"], receiver);
        let public_key = STANDARD
            .decode(payload["recipientPublicKey"].as_str().unwrap())
            .unwrap();
        assert_eq!(public_key.len(), 65);
        assert_eq!(public_key[0], 4);
        assert!(PublicKey::from_sec1_bytes(&public_key).is_ok());
        assert_eq!(
            STANDARD
                .decode(payload["psk"].as_str().unwrap())
                .unwrap()
                .len(),
            32
        );
    }
    assert_ne!(first["recipientPublicKey"], refreshed["recipientPublicKey"]);
    assert_ne!(first["psk"], refreshed["psk"]);
}

#[test]
fn refuses_ids_the_sender_cannot_scan() {
    let material = PairingMaterial::generate().unwrap();
    assert!(material.qr_payload("session", "").is_none());
    assert!(material.qr_payload("session", &"x".repeat(129)).is_none());
    assert!(material.qr_payload("session/", "receiver").is_none());
    assert!(material.qr_payload("session", "receiver_1-2").is_some());
}
