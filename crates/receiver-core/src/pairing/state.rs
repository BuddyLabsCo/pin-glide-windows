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

#[derive(Clone, Debug, Eq, PartialEq, serde::Deserialize, serde::Serialize)]
pub enum EnrollmentPhase<Token> {
    Attempt0(Token),
    Attempt1(Token),
    Issued(Token),
}

pub enum EnrollmentEvent<Token> {
    Start,
    Resume,
    Uncertain,
    Rejected,
    Issued(Token),
}

#[derive(Debug, Eq, PartialEq)]
pub enum EnrollmentAction<Token> {
    Redeem(i64, EnrollmentPhase<Token>),
    Connect(EnrollmentPhase<Token>),
    Fail,
}

impl<Token> EnrollmentPhase<Token> {
    pub fn can_redeem_at(&self, now: SystemTime, expires_at: SystemTime) -> bool {
        matches!(self, Self::Attempt0(_) | Self::Attempt1(_)) && now < expires_at
    }

    pub fn advance(
        self,
        event: EnrollmentEvent<Token>,
        now: SystemTime,
        expires_at: SystemTime,
    ) -> EnrollmentAction<Token> {
        let can_redeem = self.can_redeem_at(now, expires_at);
        match (self, event) {
            (phase @ Self::Attempt0(_), EnrollmentEvent::Start) if can_redeem => {
                EnrollmentAction::Redeem(0, phase)
            }
            (Self::Attempt0(token), EnrollmentEvent::Resume | EnrollmentEvent::Uncertain)
                if can_redeem =>
            {
                EnrollmentAction::Redeem(1, Self::Attempt1(token))
            }
            (Self::Attempt0(_) | Self::Attempt1(_), EnrollmentEvent::Issued(token)) => {
                EnrollmentAction::Connect(Self::Issued(token))
            }
            (phase @ Self::Issued(_), EnrollmentEvent::Resume) => EnrollmentAction::Connect(phase),
            _ => EnrollmentAction::Fail,
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
