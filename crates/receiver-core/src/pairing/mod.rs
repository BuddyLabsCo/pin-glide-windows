use base64::{Engine, engine::general_purpose::STANDARD};
use hpke::{Kem, Serializable, kem::DhP256HkdfSha256};
use zeroize::Zeroizing;

mod confirmation;
mod state;
pub use confirmation::{ConfirmationError, PairingKey, PendingConfirmation};
pub use state::{
    EnrollmentAction, EnrollmentEvent, EnrollmentPhase, PairingOutcome, SessionState,
    pairing_lifetime,
};

type P256 = DhP256HkdfSha256;

pub struct PairingMaterial {
    recipient_key: <P256 as Kem>::PrivateKey,
    public_key: <P256 as Kem>::PublicKey,
    psk: Zeroizing<[u8; 32]>,
}

impl PairingMaterial {
    pub fn generate() -> Result<Self, getrandom::Error> {
        let mut psk = Zeroizing::new([0; 32]);
        getrandom::fill(psk.as_mut())?;
        let mut seed = Zeroizing::new([0; 32]);
        getrandom::fill(seed.as_mut())?;
        let (recipient_key, public_key) = P256::derive_keypair(seed.as_ref());
        Ok(Self {
            recipient_key,
            public_key,
            psk,
        })
    }

    pub fn qr_payload(&self, session_id: &str, receiver_id: &str) -> Option<String> {
        if !valid_id(session_id) || !valid_id(receiver_id) {
            return None;
        }
        Some(
            serde_json::json!({
                "version": 1,
                "cryptoVersion": 1,
                "sessionId": session_id,
                "receiverId": receiver_id,
                "recipientPublicKey": STANDARD.encode(self.public_key.to_bytes()),
                "psk": STANDARD.encode(self.psk.as_ref()),
            })
            .to_string(),
        )
    }
}

fn valid_id(id: &str) -> bool {
    (1..=128).contains(&id.len())
        && id
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-'))
}
