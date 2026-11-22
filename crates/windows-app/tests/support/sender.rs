use aes_gcm::{Aes256Gcm, KeyInit, Nonce, aead::Aead};
use base64::{Engine, engine::general_purpose::STANDARD};
use hpke::{Deserializable, Kem, OpModeS, PskBundle, Serializable};
use serde_json::{Value, json};

use super::{Socket, send_event};

pub fn decode_pixels(width: usize, height: usize, pixels: &[u8]) -> Value {
    let mut decoder = quircs::Quirc::default();
    let qr = decoder
        .identify(width, height, pixels)
        .next()
        .unwrap()
        .unwrap();
    serde_json::from_slice(&qr.decode().unwrap().payload).unwrap()
}

pub fn sender_handshake(socket: &mut Socket, qr: &Value) -> [u8; 32] {
    type KemType = hpke::kem::DhP256HkdfSha256;
    let session = qr["sessionId"].as_str().unwrap();
    let decode = |field: &Value| STANDARD.decode(field.as_str().unwrap()).unwrap();
    let public = decode(&qr["recipientPublicKey"]);
    let psk = decode(&qr["psk"]);
    let (enc, mut sender) =
        hpke::setup_sender::<hpke::aead::AesGcm256, hpke::kdf::HkdfSha256, KemType>(
            &OpModeS::Psk(PskBundle::new(&psk, session.as_bytes()).unwrap()),
            &<KemType as Kem>::PublicKey::from_bytes(&public).unwrap(),
            b"pin-glide/pairing/v1",
        )
        .unwrap();
    let challenge = STANDARD.encode([27; 32]);
    let init = json!({"type":"pairing_init","challengeA":challenge}).to_string();
    relay(
        socket,
        session,
        json!({
            "kind":"hpke_init", "cryptoVersion":1, "enc":STANDARD.encode(enc.to_bytes()),
            "ciphertext":STANDARD.encode(sender.seal(init.as_bytes(), b"").unwrap()),
        }),
    );
    let reply: Value = serde_json::from_str(socket.read().unwrap().to_text().unwrap()).unwrap();
    assert_eq!(reply["type"], "pairing_relay");
    assert_eq!(reply["version"], 1);
    assert_eq!(reply["sessionId"], session);
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
    relay(
        socket,
        session,
        json!({
            "kind":"key_confirmation", "cryptoVersion":1, "nonce":STANDARD.encode(nonce),
            "ciphertext":STANDARD.encode(cipher.encrypt(&nonce, confirmation.as_bytes()).unwrap()),
        }),
    );
    key
}

fn relay(socket: &mut Socket, session: &str, payload: Value) {
    send_event(
        socket,
        json!({
            "type":"pairing_relay", "version":1, "sessionId":session, "side":"sender",
            "payload":STANDARD.encode(payload.to_string()),
        }),
    );
}
