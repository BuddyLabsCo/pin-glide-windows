use std::time::SystemTime;

use aes_gcm::{Aes256Gcm, KeyInit, Nonce, aead::Aead};
use base64::{Engine, engine::general_purpose::STANDARD};
use serde::Deserialize;
use time::{OffsetDateTime, format_description::well_known::Rfc3339};
use zeroize::Zeroizing;

pub struct Arrival {
    pub time: SystemTime,
    pub order: u64,
}

pub struct Envelope<'a> {
    pub pairing_id: &'a str,
    pub crypto_version: i64,
    pub nonce: &'a str,
    pub ciphertext: &'a str,
}

pub struct Sms {
    pub body: String,
    pub otp: String,
}

#[derive(Debug, PartialEq, Eq)]
pub enum ReceiveError {
    UnsupportedCrypto,
    PairingKeyUnavailable,
    InvalidEncoding,
    AuthenticationFailed,
    InvalidPlaintext,
}

#[derive(Default)]
pub struct Inbox {
    latest: Option<((SystemTime, u64), Zeroizing<String>)>,
}

#[derive(Deserialize)]
#[serde(
    tag = "type",
    rename_all = "snake_case",
    rename_all_fields = "camelCase"
)]
enum Plaintext {
    VerificationMessage {
        sms_text: String,
        otp: String,
        received_at: Option<serde_json::Value>,
    },
}

impl Inbox {
    pub fn receive(
        &mut self,
        arrival: Arrival,
        envelope: Envelope<'_>,
        key_for_pairing: impl FnOnce(&str) -> Option<Zeroizing<[u8; 32]>>,
    ) -> Result<Sms, ReceiveError> {
        if envelope.crypto_version != 1 {
            return Err(ReceiveError::UnsupportedCrypto);
        }
        let key =
            key_for_pairing(envelope.pairing_id).ok_or(ReceiveError::PairingKeyUnavailable)?;
        let nonce = <[u8; 12]>::try_from(decode(envelope.nonce)?)
            .map_err(|_| ReceiveError::InvalidEncoding)?;
        let ciphertext = decode(envelope.ciphertext)?;
        let plaintext = Zeroizing::new(
            Aes256Gcm::new((&*key).into())
                .decrypt(&Nonce::from(nonce), ciphertext.as_slice())
                .map_err(|_| ReceiveError::AuthenticationFailed)?,
        );
        let Plaintext::VerificationMessage {
            sms_text,
            otp,
            received_at,
        } = serde_json::from_slice(&plaintext).map_err(|_| ReceiveError::InvalidPlaintext)?;
        let effective_time = received_at
            .as_ref()
            .and_then(serde_json::Value::as_str)
            .and_then(|stamp| OffsetDateTime::parse(stamp, &Rfc3339).ok())
            .filter(|stamp| stamp.offset().is_utc())
            .map_or(arrival.time, SystemTime::from);
        let order = (effective_time, arrival.order);
        if !otp.is_empty()
            && self
                .latest
                .as_ref()
                .is_none_or(|(latest, _)| order > *latest)
        {
            self.latest = Some((order, Zeroizing::new(otp.clone())));
        }
        Ok(Sms {
            body: sms_text,
            otp,
        })
    }

    pub fn latest(&self) -> Option<&str> {
        self.latest.as_ref().map(|(_, otp)| otp.as_str())
    }
}

fn decode(encoded: &str) -> Result<Vec<u8>, ReceiveError> {
    STANDARD
        .decode(encoded)
        .map_err(|_| ReceiveError::InvalidEncoding)
}
