use aes_gcm::{Aes256Gcm, KeyInit, Nonce, aead::Aead};
use base64::{Engine, engine::general_purpose::STANDARD};
use hpke::{Deserializable, Kem, OpModeS, PskBundle, Serializable};
use receiver_core::pairing::PairingMaterial;
use serde_json::{Value, json};

type P256 = hpke::kem::DhP256HkdfSha256;

#[test]
fn confirms_only_the_sender_that_echoes_the_fresh_receiver_challenge() {
    let decode = |field: &Value| STANDARD.decode(field.as_str().unwrap()).unwrap();
    for invalid in [
        None,
        Some("role"),
        Some("type"),
        Some("challenge"),
        Some("tag"),
    ] {
        let material = PairingMaterial::generate().unwrap();
        let qr: Value =
            serde_json::from_str(&material.qr_payload("fixture", "receiver").unwrap()).unwrap();
        let public = decode(&qr["recipientPublicKey"]);
        let psk = decode(&qr["psk"]);
        let (enc, mut sender) =
            hpke::setup_sender::<hpke::aead::AesGcm256, hpke::kdf::HkdfSha256, P256>(
                &OpModeS::Psk(PskBundle::new(&psk, b"fixture").unwrap()),
                &<P256 as Kem>::PublicKey::from_bytes(&public).unwrap(),
                b"pin-glide/pairing/v1",
            )
            .unwrap();
        let challenge = STANDARD.encode([255; 32]);
        let init = json!({"type": "pairing_init", "challengeA": challenge})
            .to_string()
            .replace('/', r"\/");
        let sealed = sender.seal(init.as_bytes(), b"").unwrap();
        let relay = pack(json!({
            "kind": "hpke_init", "cryptoVersion": 1,
            "enc": STANDARD.encode(enc.to_bytes()), "ciphertext": STANDARD.encode(sealed),
        }));
        assert!(
            material
                .begin_confirmation("another-session", &relay)
                .is_err()
        );
        let (pending, reply) = material.begin_confirmation("fixture", &relay).unwrap();
        let mut key = [0; 32];
        sender
            .export(b"pin-glide/message-key/v1", &mut key)
            .unwrap();
        let cipher = Aes256Gcm::new_from_slice(&key).unwrap();
        let reply: Value = serde_json::from_slice(&STANDARD.decode(reply).unwrap()).unwrap();
        assert_eq!(reply["kind"], "key_confirmation");
        assert_eq!(reply["cryptoVersion"], 1);
        let nonce = <[u8; 12]>::try_from(decode(&reply["nonce"])).unwrap();
        let plaintext: Value = serde_json::from_slice(
            &cipher
                .decrypt(&Nonce::from(nonce), decode(&reply["ciphertext"]).as_slice())
                .unwrap(),
        )
        .unwrap();
        assert_eq!(plaintext["type"], "key_confirmation");
        assert_eq!(plaintext["role"], "receiver");
        assert_eq!(plaintext["challengeA"], challenge);
        assert_eq!(decode(&plaintext["challengeW"]).len(), 32);
        let mut confirmation = json!({
            "type": "key_confirmation", "role": "sender", "challengeW": plaintext["challengeW"]
        });
        match invalid {
            Some("role") => confirmation["role"] = json!("receiver"),
            Some("type") => confirmation["type"] = json!("verification_message"),
            Some("challenge") => confirmation["challengeW"] = json!(STANDARD.encode([0; 32])),
            _ => {}
        }
        let nonce = Nonce::from([23; 12]);
        let mut ciphertext = cipher
            .encrypt(&nonce, confirmation.to_string().as_bytes())
            .unwrap();
        if invalid == Some("tag") {
            ciphertext[0] ^= 1;
        }
        let result = pending.verify(&pack(json!({
            "kind": "key_confirmation", "cryptoVersion": 1,
            "nonce": STANDARD.encode(nonce), "ciphertext": STANDARD.encode(ciphertext)
        })));
        if invalid.is_some() {
            assert!(result.is_err(), "accepted invalid {invalid:?}");
        } else {
            assert_eq!(result.unwrap().as_bytes(), &key);
        }
    }
}

fn pack(fields: Value) -> String {
    STANDARD.encode(fields.to_string())
}
