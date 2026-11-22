use std::time::SystemTime;

use pin_glide_http::{
    apis::urlencode,
    models::{IssuedReceiverToken, RedeemRequest},
};
use receiver_core::pairing::{EnrollmentAction, EnrollmentEvent};
use reqwest::StatusCode;
use tokio::sync::oneshot;
use zeroize::Zeroizing;

use super::{
    NewReceiverSession, PairingOutcome, PairingProgress, Service,
    pairing_events::PairingEventError, pairing_operation::ProgressUpdates, reconnect::Reconnect,
};
use crate::storage::{Enrollment, RegistrationStore};

pub(super) async fn finish_enrollment(
    service: &Service,
    store: &RegistrationStore,
    updates: &ProgressUpdates,
    mut stopping: oneshot::Receiver<()>,
    session: Option<&NewReceiverSession>,
) {
    let outcome = tokio::select! {
        biased;
        _ = &mut stopping => return,
        outcome = issue_credential(service, store, updates, session) => outcome,
    };
    match outcome {
        Ok(pending) => {
            tokio::select! {
                biased;
                _ = stopping => {}
                () = connect_and_promote(service, store, updates, pending) => {}
            }
        }
        Err(outcome) => updates.publish(PairingProgress::Finished(outcome)),
    }
}

async fn issue_credential(
    service: &Service,
    store: &RegistrationStore,
    updates: &ProgressUpdates,
    session: Option<&NewReceiverSession>,
) -> Result<Enrollment, PairingOutcome> {
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
    let mut event = if session.is_some() {
        EnrollmentEvent::Start
    } else {
        EnrollmentEvent::Resume
    };
    loop {
        let newly_issued = matches!(event, EnrollmentEvent::Issued(_));
        let action = pending
            .phase
            .advance(event, SystemTime::now(), pending.expires_at);
        match action {
            EnrollmentAction::Redeem(attempt, phase) => {
                pending.phase = phase;
                if attempt == 1 {
                    updates.publish(PairingProgress::Checking);
                    store
                        .save_enrollment(&pending)
                        .map_err(|_| PairingOutcome::Failed)?;
                }
                event = redeem(service, &pending, attempt).await?;
            }
            EnrollmentAction::Connect(phase) => {
                pending.phase = phase;
                if newly_issued {
                    store
                        .save_enrollment(&pending)
                        .map_err(|_| PairingOutcome::Failed)?;
                }
                break;
            }
            EnrollmentAction::Fail => return reject(store, &pending.operation_id),
        }
    }
    Ok(pending)
}

async fn connect_and_promote(
    service: &Service,
    store: &RegistrationStore,
    updates: &ProgressUpdates,
    pending: Enrollment,
) {
    updates.publish(PairingProgress::Connecting);
    let result = async {
        let token = pending.token().map_err(|_| PairingOutcome::Failed)?;
        let mut receiver = Reconnect::new(service, &token, &pending.receiver_id, updates);
        let connection = match receiver.open(true).await {
            Ok(connection) => connection,
            Err(PairingEventError::Rejected { .. }) => return reject(store, &pending.operation_id),
            Err(_) => return Err(PairingOutcome::Failed),
        };
        let pairing_id = store
            .finish_enrollment(&pending.operation_id, true)
            .map_err(|_| PairingOutcome::Failed)?
            .ok_or(PairingOutcome::Failed)?;
        updates.publish(PairingProgress::Completed { pairing_id });
        receiver.receive(store, connection).await;
        Ok(())
    }
    .await;
    if let Err(outcome) = result {
        updates.publish(PairingProgress::Finished(outcome));
    }
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

async fn redeem(
    service: &Service,
    pending: &Enrollment,
    attempt: i64,
) -> Result<EnrollmentEvent<Vec<u8>>, PairingOutcome> {
    if !pending
        .phase
        .can_redeem_at(SystemTime::now(), pending.expires_at)
    {
        return Ok(EnrollmentEvent::Rejected);
    }
    let Ok(token) = pending.token() else {
        return Ok(EnrollmentEvent::Rejected);
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
        Err(error) if !error.is_builder() && !error.is_connect() => {
            return Ok(EnrollmentEvent::Uncertain);
        }
        _ => return Ok(EnrollmentEvent::Rejected),
    };
    match response.json::<IssuedReceiverToken>().await {
        Ok(issued)
            if issued.receiver_id == pending.receiver_id && !issued.receiver_token.is_empty() =>
        {
            let token = Zeroizing::new(issued.receiver_token);
            Enrollment::protect_token(&token)
                .map(EnrollmentEvent::Issued)
                .map_err(|_| PairingOutcome::Failed)
        }
        Ok(_) => Ok(EnrollmentEvent::Rejected),
        Err(_) => Ok(EnrollmentEvent::Uncertain),
    }
}

fn reject<T>(store: &RegistrationStore, operation_id: &str) -> Result<T, PairingOutcome> {
    let _ = store.finish_enrollment(operation_id, false);
    Err(PairingOutcome::Failed)
}
