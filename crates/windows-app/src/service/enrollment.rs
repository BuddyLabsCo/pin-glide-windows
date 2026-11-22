use std::time::{Duration, SystemTime};

use pin_glide_http::{
    apis::urlencode,
    models::{IssuedReceiverToken, RedeemRequest},
};
use reqwest::StatusCode;
use tokio::time::timeout;
use zeroize::Zeroizing;

use super::{
    NewReceiverSession, PairingOutcome, PairingProgress, Service, delivery::DeliveryConnection,
    pairing_events::PairingEventError, pairing_operation::ProgressUpdates,
};
use crate::storage::{Enrollment, EnrollmentPhase, RegistrationStore};

enum Redemption {
    Issued(Zeroizing<String>),
    Uncertain,
    Rejected,
}

pub(super) async fn enroll(
    service: &Service,
    store: &RegistrationStore,
    updates: &ProgressUpdates,
    session: Option<&NewReceiverSession>,
) -> Result<DeliveryConnection, PairingOutcome> {
    let mut pending = match session {
        Some(session) => {
            updates.publish(PairingProgress::CredentialReady);
            let pending = start(service, session)?;
            store
                .save_enrollment(&pending)
                .map_err(|_| PairingOutcome::Failed)?;
            pending
        }
        None => store
            .enrollment()
            .ok()
            .flatten()
            .ok_or(PairingOutcome::Failed)?,
    };
    if pending.origin != service.config.base_path {
        return Err(PairingOutcome::Failed);
    }
    if session.is_some() {
        match redeem(service, &pending, 0).await {
            Redemption::Issued(token) => save_issued(store, &mut pending, &token)?,
            Redemption::Uncertain => {}
            Redemption::Rejected => return reject(store, &pending),
        }
    }
    match pending.phase {
        EnrollmentPhase::Attempt0(_) => {
            updates.publish(PairingProgress::Checking);
            if pending.expires_at <= SystemTime::now() {
                return reject(store, &pending);
            }
            pending.retry().map_err(|_| PairingOutcome::Failed)?;
            store
                .save_enrollment(&pending)
                .map_err(|_| PairingOutcome::Failed)?;
            match redeem(service, &pending, 1).await {
                Redemption::Issued(token) => save_issued(store, &mut pending, &token)?,
                Redemption::Rejected | Redemption::Uncertain => return reject(store, &pending),
            }
        }
        EnrollmentPhase::Attempt1(_) => return reject(store, &pending),
        EnrollmentPhase::Issued(_) => {}
    }
    updates.publish(PairingProgress::Connecting);
    let token = pending.token().map_err(|_| PairingOutcome::Failed)?;
    let result = timeout(
        Duration::from_secs(15),
        DeliveryConnection::open(service, &token),
    )
    .await;
    let connection = match result {
        Ok(Ok(connection)) => connection,
        Ok(Err(PairingEventError::Rejected { status, .. }))
            if status.is_client_error() && status != StatusCode::CONFLICT =>
        {
            return reject(store, &pending);
        }
        _ => return Err(PairingOutcome::Failed),
    };
    store
        .finish_enrollment(&pending.operation_id, true)
        .map_err(|_| PairingOutcome::Failed)?;
    Ok(connection)
}

fn start(service: &Service, session: &NewReceiverSession) -> Result<Enrollment, PairingOutcome> {
    let mut bytes = [0_u8; 16];
    getrandom::fill(&mut bytes).map_err(|_| PairingOutcome::Failed)?;
    let operation_id = format!("{:032x}", u128::from_be_bytes(bytes));
    Enrollment::new(
        service.config.base_path.clone(),
        session.session_id.clone(),
        session.receiver_id.clone(),
        operation_id,
        session
            .expires_at
            .min(session.pairing_token_expires_at)
            .into(),
        &session.pairing_token,
    )
    .map_err(|_| PairingOutcome::Failed)
}

async fn redeem(service: &Service, pending: &Enrollment, attempt: i64) -> Redemption {
    if pending.expires_at <= SystemTime::now() {
        return Redemption::Rejected;
    }
    let Ok(token) = pending.token() else {
        return Redemption::Rejected;
    };
    let request = RedeemRequest::new(attempt, pending.operation_id.clone());
    let config = &service.config;
    let session_id = urlencode(&pending.session_id);
    let response = config
        .client
        .post(format!(
            "{}/v1/pairing-sessions/{session_id}/receiver-token",
            config.base_path
        ))
        .bearer_auth(token.as_str())
        .json(&request)
        .send()
        .await;
    let response = match response {
        Ok(response) if response.status() == StatusCode::OK => response,
        Err(error) if !error.is_builder() && !error.is_connect() => return Redemption::Uncertain,
        _ => return Redemption::Rejected,
    };
    match response.json::<IssuedReceiverToken>().await {
        Ok(issued)
            if issued.receiver_id == pending.receiver_id && !issued.receiver_token.is_empty() =>
        {
            Redemption::Issued(Zeroizing::new(issued.receiver_token))
        }
        Ok(_) => Redemption::Rejected,
        Err(_) => Redemption::Uncertain,
    }
}

fn save_issued(
    store: &RegistrationStore,
    pending: &mut Enrollment,
    token: &str,
) -> Result<(), PairingOutcome> {
    pending
        .issue(token)
        .and_then(|()| store.save_enrollment(pending))
        .map_err(|_| PairingOutcome::Failed)
}

fn reject<T>(store: &RegistrationStore, pending: &Enrollment) -> Result<T, PairingOutcome> {
    let _ = store.finish_enrollment(&pending.operation_id, false);
    Err(PairingOutcome::Failed)
}
