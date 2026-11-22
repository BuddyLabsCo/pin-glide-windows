use aes_gcm::{Aes256Gcm, KeyInit, Nonce, aead::Aead};
use base64::{Engine, engine::general_purpose::STANDARD};
use serde_json::{Value, json};
use tokio_tungstenite::tungstenite::{Message, protocol::frame::coding::CloseCode};

use super::fixture::{Socket, send_event};

pub(super) fn assert_delivery_receipts(delivery: &mut Socket, key: &[u8]) {
    let cipher = Aes256Gcm::new_from_slice(key).unwrap();
    let encrypt =
        |nonce, sms: &[u8]| STANDARD.encode(cipher.encrypt(&Nonce::from(nonce), sms).unwrap());
    let nonce = [17; 12];
    let ciphertext = encrypt(
        nonce,
        br#"{"type":"verification_message","smsText":"Synthetic SMS","otp":"A-482913"}"#,
    );
    let bad_plaintext_nonce = [18; 12];
    let bad_plaintext = encrypt(
        bad_plaintext_nonce,
        br#"{"type":"verification_message","smsText":"Rejected text","otp":123456}"#,
    );
    let envelope = |message_id: &str, nonce: String, ciphertext: String| {
        json!({
            "type":"delivery", "version":1, "senderId":"sender-1", "messageId":message_id,
            "receiverId":"receiver-1", "pairingId":"pairing-1", "cryptoVersion":1,
            "nonce":nonce, "ciphertext":ciphertext,
        })
    };
    for envelope in [
        envelope("bad-body", "not-base64".into(), "not-base64".into()),
        json!({
            "type":"delivery", "version":1, "senderId":"sender-1", "messageId":"missing-body",
        }),
        envelope("valid-body", STANDARD.encode(nonce), ciphertext.clone()),
        envelope("bad-tag", STANDARD.encode([19; 12]), ciphertext),
        envelope(
            "bad-plaintext",
            STANDARD.encode(bad_plaintext_nonce),
            bad_plaintext,
        ),
    ] {
        let expected = json!({
            "type":"receipt", "version":1,
            "senderId":"sender-1", "messageId":envelope["messageId"],
        });
        send_event(delivery, envelope);
        let receipt: Value =
            serde_json::from_str(delivery.read().unwrap().to_text().unwrap()).unwrap();
        assert_eq!(receipt, expected);
    }
    send_event(
        delivery,
        json!({"type":"delivery", "version":1, "messageId":"no-sender"}),
    );
    assert!(matches!(
        delivery.read().unwrap(),
        Message::Close(Some(frame)) if frame.code == CloseCode::Protocol
    ));
}
