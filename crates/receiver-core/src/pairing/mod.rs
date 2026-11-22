use base64::{Engine, engine::general_purpose::STANDARD};
use p256::{SecretKey, elliptic_curve::sec1::ToEncodedPoint};
use rand_core::{OsRng, RngCore};
use zeroize::Zeroizing;

mod state;
pub use state::{PairingOutcome, PairingPhase, SessionState, pairing_lifetime};

pub struct PairingMaterial {
    recipient_key: SecretKey,
    psk: Zeroizing<[u8; 32]>,
}

impl PairingMaterial {
    pub fn generate() -> Result<Self, rand_core::Error> {
        let mut psk = Zeroizing::new([0; 32]);
        OsRng.try_fill_bytes(psk.as_mut())?;
        let mut candidate = Zeroizing::new([0; 32]);
        let recipient_key = loop {
            OsRng.try_fill_bytes(candidate.as_mut())?;
            if let Ok(key) = SecretKey::from_slice(candidate.as_ref()) {
                break key;
            }
        };
        Ok(Self { recipient_key, psk })
    }

    pub fn qr_payload(&self, session_id: &str, receiver_id: &str) -> Option<String> {
        if [session_id, receiver_id].iter().any(|id| {
            !(1..=128).contains(&id.len())
                || !id
                    .bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-'))
        }) {
            return None;
        }
        let public_key = self.recipient_key.public_key().to_encoded_point(false);
        Some(
            serde_json::json!({
                "version": 1,
                "cryptoVersion": 1,
                "sessionId": session_id,
                "receiverId": receiver_id,
                "recipientPublicKey": STANDARD.encode(public_key.as_bytes()),
                "psk": STANDARD.encode(self.psk.as_ref()),
            })
            .to_string(),
        )
    }
}
