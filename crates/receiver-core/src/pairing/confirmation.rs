use aes_gcm::{Aes256Gcm, KeyInit, Nonce, aead::Aead};
use base64::{Engine, engine::general_purpose::STANDARD};
use hpke::{Deserializable, Kem, OpModeR, PskBundle, aead::AesGcm256, kdf::HkdfSha256};
use serde::Deserialize;
use serde_json::json;
use subtle::ConstantTimeEq;
use zeroize::Zeroizing;

use super::{P256, PairingMaterial, valid_id};

#[derive(Debug)]
pub struct ConfirmationError;

pub struct PairingKey(Zeroizing<[u8; 32]>);

impl PairingKey {
    pub fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }
}

pub struct PendingConfirmation {
    key: PairingKey,
    challenge: Zeroizing<[u8; 32]>,
}

#[derive(Deserialize)]
#[serde(
    tag = "kind",
    rename_all = "snake_case",
    rename_all_fields = "camelCase"
)]
enum Relay {
    HpkeInit {
        crypto_version: u8,
        enc: String,
        ciphertext: String,
    },
    KeyConfirmation {
        crypto_version: u8,
        nonce: String,
        ciphertext: String,
    },
}

#[derive(Deserialize)]
#[serde(
    tag = "type",
    rename_all = "snake_case",
    rename_all_fields = "camelCase"
)]
enum Plaintext {
    PairingInit {
        #[serde(deserialize_with = "secret_string")]
        challenge_a: Zeroizing<String>,
    },
    KeyConfirmation {
        role: String,
        #[serde(deserialize_with = "secret_string")]
        challenge_w: Zeroizing<String>,
    },
}

impl PairingMaterial {
    pub fn begin_confirmation(
        &self,
        session_id: &str,
        payload: &str,
    ) -> Result<(PendingConfirmation, String), ConfirmationError> {
        if !valid_id(session_id) {
            return Err(ConfirmationError);
        }
        let relay = decode(payload)?;
        let Relay::HpkeInit {
            crypto_version: 1,
            enc,
            ciphertext,
        } = serde_json::from_slice(&relay).map_err(|_| ConfirmationError)?
        else {
            return Err(ConfirmationError);
        };
        let enc = decode(&enc)?;
        if enc.len() != 65 || enc[0] != 4 {
            return Err(ConfirmationError);
        }
        let enc = <P256 as Kem>::EncappedKey::from_bytes(&enc).map_err(|_| ConfirmationError)?;
        let psk = PskBundle::new(self.psk.as_ref(), session_id.as_bytes())
            .map_err(|_| ConfirmationError)?;
        let mut context = hpke::setup_receiver::<AesGcm256, HkdfSha256, P256>(
            &OpModeR::Psk(psk),
            &self.recipient_key,
            &enc,
            b"pin-glide/pairing/v1",
        )
        .map_err(|_| ConfirmationError)?;
        let plaintext = Zeroizing::new(
            context
                .open(&decode(&ciphertext)?, b"")
                .map_err(|_| ConfirmationError)?,
        );
        let Plaintext::PairingInit { challenge_a } =
            serde_json::from_slice(&plaintext).map_err(|_| ConfirmationError)?
        else {
            return Err(ConfirmationError);
        };
        if Zeroizing::new(decode(&challenge_a)?).len() != 32 {
            return Err(ConfirmationError);
        }
        let mut key = PairingKey(Zeroizing::new([0; 32]));
        context
            .export(b"pin-glide/message-key/v1", key.0.as_mut())
            .map_err(|_| ConfirmationError)?;
        let mut challenge = Zeroizing::new([0; 32]);
        getrandom::fill(challenge.as_mut()).map_err(|_| ConfirmationError)?;
        let pending = PendingConfirmation { key, challenge };
        let reply = pending.reply(&challenge_a)?;
        Ok((pending, reply))
    }
}

impl PendingConfirmation {
    fn reply(&self, challenge_a: &str) -> Result<String, ConfirmationError> {
        let mut nonce = [0; 12];
        getrandom::fill(&mut nonce).map_err(|_| ConfirmationError)?;
        let challenge_w = Zeroizing::new(STANDARD.encode(self.challenge.as_ref()));
        let plaintext = Zeroizing::new(format!(
            r#"{{"type":"key_confirmation","role":"receiver","challengeA":"{challenge_a}","challengeW":"{}"}}"#,
            challenge_w.as_str()
        ));
        let ciphertext = Aes256Gcm::new(self.key.as_bytes().into())
            .encrypt(&Nonce::from(nonce), plaintext.as_bytes())
            .map_err(|_| ConfirmationError)?;
        Ok(STANDARD.encode(
            json!({
                "kind": "key_confirmation",
                "cryptoVersion": 1,
                "nonce": STANDARD.encode(nonce),
                "ciphertext": STANDARD.encode(ciphertext),
            })
            .to_string(),
        ))
    }

    pub fn verify(self, payload: &str) -> Result<PairingKey, ConfirmationError> {
        let relay = decode(payload)?;
        let Relay::KeyConfirmation {
            crypto_version: 1,
            nonce,
            ciphertext,
        } = serde_json::from_slice(&relay).map_err(|_| ConfirmationError)?
        else {
            return Err(ConfirmationError);
        };
        let nonce = <[u8; 12]>::try_from(decode(&nonce)?).map_err(|_| ConfirmationError)?;
        let plaintext = Zeroizing::new(
            Aes256Gcm::new(self.key.as_bytes().into())
                .decrypt(&Nonce::from(nonce), decode(&ciphertext)?.as_slice())
                .map_err(|_| ConfirmationError)?,
        );
        let Plaintext::KeyConfirmation { role, challenge_w } =
            serde_json::from_slice(&plaintext).map_err(|_| ConfirmationError)?
        else {
            return Err(ConfirmationError);
        };
        let echoed = Zeroizing::new(decode(&challenge_w)?);
        if role != "sender" || !bool::from(self.challenge.as_slice().ct_eq(&echoed)) {
            return Err(ConfirmationError);
        }
        Ok(self.key)
    }
}

fn decode(encoded: &str) -> Result<Vec<u8>, ConfirmationError> {
    STANDARD.decode(encoded).map_err(|_| ConfirmationError)
}

fn secret_string<'de, D: serde::Deserializer<'de>>(
    deserializer: D,
) -> Result<Zeroizing<String>, D::Error> {
    String::deserialize(deserializer).map(Zeroizing::new)
}
