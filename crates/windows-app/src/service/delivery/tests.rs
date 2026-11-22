use std::{cell::Cell, future::ready};

use super::*;

#[tokio::test]
async fn attempts_the_receipt_before_authentication_even_when_sending_fails() {
    let dir = tempfile::tempdir().unwrap();
    let store = RegistrationStore::new(dir.path().join("registration.json"));
    let delivery = r#"{"type":"delivery","version":1,"cryptoVersion":1,"senderId":"sender",
        "messageId":"message","receiverId":"receiver","pairingId":"pairing","nonce":"","ciphertext":""}"#;
    for fails in [false, true] {
        let attempted = Cell::new(false);
        let authenticated = Cell::new(false);
        let mut socket = futures_util::sink::drain()
            .sink_map_err(|never| -> SocketError { match never {} })
            .with(|frame: Message| {
                let receipt: Receipt = serde_json::from_str(frame.to_text().unwrap()).unwrap();
                assert_eq!(receipt.reserved_type, "receipt");
                assert_eq!(receipt.sender_id, "sender");
                assert_eq!(receipt.message_id, "message");
                attempted.set(true);
                ready(if fails {
                    Err(SocketError::ConnectionClosed)
                } else {
                    Ok(frame)
                })
            });
        let result = handle_text(&mut socket, delivery, "receiver", &store, |_| {
            assert!(
                attempted.get(),
                "authenticate only after attempting the Receipt"
            );
            authenticated.set(true);
            Ok(())
        })
        .await;
        assert!(authenticated.get());
        assert_eq!(result.is_err(), fails);
    }
}
