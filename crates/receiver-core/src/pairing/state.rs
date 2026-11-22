use std::time::{Duration, SystemTime};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SessionState {
    AwaitingAuthorization,
    AwaitingConfirmation,
    CredentialReady,
    CredentialIssued,
    Completed,
    Canceled,
    Expired,
    Superseded,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PairingPhase {
    Preparing,
    Waiting,
    Confirming,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PairingOutcome {
    CreateFailed,
    Failed,
    Expired,
    Canceled,
}

impl PairingPhase {
    pub fn advance(self, state: SessionState) -> Result<Self, PairingOutcome> {
        match state {
            SessionState::AwaitingAuthorization if self != Self::Confirming => Ok(Self::Waiting),
            SessionState::AwaitingAuthorization | SessionState::AwaitingConfirmation => {
                Ok(Self::Confirming)
            }
            SessionState::Canceled => Err(PairingOutcome::Canceled),
            SessionState::Expired => Err(PairingOutcome::Expired),
            _ => Err(PairingOutcome::Failed),
        }
    }
}

pub fn pairing_lifetime(
    now: SystemTime,
    expires: SystemTime,
    token_expires: SystemTime,
) -> Duration {
    expires
        .min(token_expires)
        .duration_since(now)
        .unwrap_or_default()
        .min(Duration::from_secs(300))
}
