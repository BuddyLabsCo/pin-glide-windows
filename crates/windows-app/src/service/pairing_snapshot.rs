use super::SessionState;

pub(super) struct Snapshot {
    pub session_id: String,
    pub receiver_id: String,
    pub pairing_id: Option<String>,
    pub sender_id: Option<String>,
    pub state: SessionState,
    pub receiver_confirmed: bool,
    pub sender_confirmed: bool,
}

pub(super) struct Claim {
    pub pairing_id: String,
    pub sender_id: String,
}

impl Snapshot {
    pub fn validated(self, session_id: &str, receiver_id: &str) -> Option<Self> {
        if self.session_id != session_id
            || self.receiver_id != receiver_id
            || self.pairing_id.as_deref() == Some("")
            || self.sender_id.as_deref() == Some("")
        {
            return None;
        }
        let has_claim = self.pairing_id.is_some() && self.sender_id.is_some();
        let valid = match self.state {
            SessionState::AwaitingAuthorization => {
                self.pairing_id.is_none() && self.sender_id.is_none()
            }
            SessionState::AwaitingConfirmation => has_claim,
            SessionState::CredentialReady
            | SessionState::CredentialIssued
            | SessionState::Completed => {
                has_claim && self.receiver_confirmed && self.sender_confirmed
            }
            SessionState::Canceled | SessionState::Expired | SessionState::Superseded => {
                self.pairing_id.is_none() || has_claim
            }
        };
        valid.then_some(self)
    }

    pub fn into_claim(self) -> Option<Claim> {
        Some(Claim {
            pairing_id: self.pairing_id?,
            sender_id: self.sender_id?,
        })
    }

    pub fn matches_claim(&self, claim: &Claim) -> bool {
        self.pairing_id
            .as_ref()
            .is_none_or(|id| id == &claim.pairing_id)
            && self
                .sender_id
                .as_ref()
                .is_none_or(|id| id == &claim.sender_id)
    }
}
