use aes_gcm::{Aes256Gcm, KeyInit, Nonce, aead::Aead};
use base64::{Engine, engine::general_purpose::STANDARD};
use receiver_core::delivery::{Arrival, Envelope, Inbox, ReceiveError, Sms};
use std::time::{Duration, UNIX_EPOCH};

fn receive(
    inbox: &mut Inbox,
    pairing: &str,
    plaintext: &str,
    seconds: u64,
    order: u64,
) -> Result<Sms, ReceiveError> {
    let key = [7; 32];
    let nonce = [3; 12];
    let ciphertext = Aes256Gcm::new((&key).into())
        .encrypt(&Nonce::from(nonce), plaintext.as_bytes())
        .unwrap();
    inbox.receive(
        Arrival {
            time: UNIX_EPOCH + Duration::from_secs(seconds),
            order,
        },
        Envelope {
            pairing_id: pairing,
            crypto_version: 1,
            nonce: &STANDARD.encode(nonce),
            ciphertext: &STANDARD.encode(ciphertext),
        },
        |id| match id {
            "phone" | "spare" => Some(zeroize::Zeroizing::new(key)),
            "other" => Some(zeroize::Zeroizing::new([8; 32])),
            _ => None,
        },
    )
}

#[test]
fn removal_clears_only_the_owning_code_and_keeps_its_order() {
    let mut inbox = Inbox::default();
    let sms = r#"{"type":"verification_message","smsText":"Synthetic SMS","otp":"123456"}"#;
    receive(&mut inbox, "phone", sms, 10, 1).unwrap();
    inbox.remove_pairing("spare".into());
    assert_eq!(inbox.latest(), Some("123456"));
    assert_eq!(
        receive(&mut inbox, "spare", sms, 11, 2).err(),
        Some(ReceiveError::PairingKeyUnavailable)
    );

    let mut inbox = Inbox::default();
    receive(&mut inbox, "spare", sms, 9, 1).unwrap();
    receive(&mut inbox, "phone", sms, 10, 2).unwrap();
    inbox.remove_pairing("phone".into());
    assert_eq!(inbox.latest(), None);
    receive(&mut inbox, "spare", sms, 9, 3).unwrap();
    assert_eq!(inbox.latest(), None);
    receive(&mut inbox, "spare", sms, 11, 4).unwrap();
    assert_eq!(inbox.latest(), Some("123456"));
    inbox.revoke();
    assert_eq!(inbox.latest(), None);
    assert_eq!(
        receive(&mut inbox, "spare", sms, 12, 5).err(),
        Some(ReceiveError::PairingKeyUnavailable)
    );
}

#[test]
fn only_authenticated_sms_display_and_empty_otp_preserves_the_latest_code() {
    let mut inbox = Inbox::default();
    let sms = r#"{"type":"verification_message","smsText":"Original\n body","otp":"A-123 xyz"}"#;
    assert_eq!(
        receive(&mut inbox, "other", sms, 1, 1).err(),
        Some(ReceiveError::AuthenticationFailed)
    );
    let displayed = receive(&mut inbox, "phone", sms, 1, 2).unwrap();
    assert_eq!(displayed.body, "Original\n body");
    assert_eq!(displayed.otp, "A-123 xyz");
    for invalid in [
        r#"{"type":"verification_message","smsText":"message"}"#,
        r#"{"type":"verification_message","smsText":"message","otp":null}"#,
        r#"{"type":"verification_message","smsText":"message","otp":12}"#,
        r#"{"type":"key_confirmation","smsText":"message","otp":"1234"}"#,
    ] {
        assert_eq!(
            receive(&mut inbox, "phone", invalid, 4, 3).err(),
            Some(ReceiveError::InvalidPlaintext)
        );
    }
    let no_code = r#"{"type":"verification_message","smsText":"Parcel delivered","otp":""}"#;
    let displayed = receive(&mut inbox, "phone", no_code, 5, 4).unwrap();
    assert_eq!(displayed.body, "Parcel delivered");
    assert!(displayed.otp.is_empty());
    assert_eq!(inbox.latest(), Some("A-123 xyz"));
}

#[test]
fn latest_uses_source_milliseconds_then_arrival_order_with_unusable_time_fallback() {
    let mut inbox = Inbox::default();
    let sms = |otp, timestamp| {
        serde_json::json!({
            "type": "verification_message",
            "smsText": "Test SMS",
            "otp": otp,
            "receivedAt": timestamp
        })
        .to_string()
    };
    let newer = sms("newer", "1970-01-01T00:00:03.123Z");
    let older = sms("older", "1970-01-01T00:00:03.120Z");
    let earlier_arrival = sms("tie", "1970-01-01T00:00:03.123Z");
    assert!(receive(&mut inbox, "phone", &newer, 8, 3).is_ok());
    assert!(receive(&mut inbox, "phone", &older, 9, 4).is_ok());
    assert!(receive(&mut inbox, "phone", &earlier_arrival, 7, 2).is_ok());
    assert_eq!(inbox.latest(), Some("newer"));

    let fallback = sms("fallback", "invalid");
    assert!(receive(&mut inbox, "phone", &fallback, 5, 1).is_ok());
    assert_eq!(inbox.latest(), Some("fallback"));
}
