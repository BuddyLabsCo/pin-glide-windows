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
pub enum PairingOutcome {
    CreateFailed,
    Failed,
    Expired,
    Canceled,
    KeyConfirmed(SessionState),
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
