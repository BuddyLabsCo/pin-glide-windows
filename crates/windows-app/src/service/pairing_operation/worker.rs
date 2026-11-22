use std::time::{Duration, SystemTime};

use receiver_core::pairing::{PairingMaterial, PairingOutcome, pairing_lifetime};
use tokio::{
    sync::oneshot,
    time::{Instant, sleep_until, timeout},
};

use super::{
    PairingProgress, PairingStart, ProgressUpdates, RegistrationStore, Service, SessionState,
    watch_session,
};
use crate::service::{DeliveryStatus, enrollment::finish_enrollment, reconnect::Reconnect};

async fn receive_registered(
    service: &Service,
    store: &RegistrationStore,
    updates: &ProgressUpdates,
) {
    let Ok((receiver_id, token)) = store.receiver_credential() else {
        updates.delivery_status(DeliveryStatus::Credential);
        return;
    };
    let mut receiver = Reconnect::new(service, &token, &receiver_id, updates);
    if let Ok(connection) = receiver.open(false).await {
        receiver.receive(store, connection).await;
    }
}

pub(super) async fn drive(
    service: &Service,
    updates: &ProgressUpdates,
    mut stopping: oneshot::Receiver<()>,
    now: impl FnOnce() -> SystemTime,
    store: Option<&RegistrationStore>,
    start: PairingStart,
) {
    if matches!(start, PairingStart::Registered) {
        if let Some(store) = store {
            tokio::select! {
                biased;
                _ = &mut stopping => {}
                () = receive_registered(service, store, updates) => {}
            }
        }
        return;
    }
    if let Some(store) = store {
        match (store.enrollment(), start) {
            (Ok(Some(_)), PairingStart::ResumePending) => {
                finish_enrollment(service, store, updates, stopping, None).await;
                return;
            }
            (Ok(Some(pending)), PairingStart::Fresh)
                if store
                    .finish_enrollment(&pending.operation_id, false)
                    .is_err() =>
            {
                updates.publish(PairingProgress::Finished(PairingOutcome::CreateFailed));
                return;
            }
            _ => {}
        }
    }
    let Ok(material) = PairingMaterial::generate() else {
        updates.publish(PairingProgress::Finished(PairingOutcome::CreateFailed));
        return;
    };
    drive_session(service, updates, stopping, now, material, store).await;
}

async fn drive_session(
    service: &Service,
    updates: &ProgressUpdates,
    mut stopping: oneshot::Receiver<()>,
    now: impl FnOnce() -> SystemTime,
    material: PairingMaterial,
    store: Option<&RegistrationStore>,
) {
    let created = tokio::select! {
        result = service.new_receiver() => result,
        _ = &mut stopping => {
            updates.publish(PairingProgress::Finished(PairingOutcome::Canceled));
            return;
        }
    };
    let Ok(session) = created else {
        updates.publish(PairingProgress::Finished(PairingOutcome::CreateFailed));
        return;
    };
    let lifetime = pairing_lifetime(
        now(),
        session.expires_at.into(),
        session.pairing_token_expires_at.into(),
    );
    let deadline = Instant::now() + lifetime;
    let (outcome, exiting) = tokio::select! {
        biased;
        _ = &mut stopping => (PairingOutcome::Canceled, true),
        outcome = watch_session(service, &session, &material, deadline, updates, store) => (outcome, false),
        _ = sleep_until(deadline) => (PairingOutcome::Expired, false),
    };
    if outcome == PairingOutcome::KeyConfirmed(SessionState::CredentialReady) {
        if let Some(store) = store {
            finish_enrollment(service, store, updates, stopping, Some(&session)).await;
        } else {
            updates.publish(PairingProgress::Finished(PairingOutcome::Failed));
        }
        return;
    }
    updates.publish(PairingProgress::Finished(outcome));
    if exiting
        || matches!(
            outcome,
            PairingOutcome::CreateFailed | PairingOutcome::Failed
        )
    {
        let _ = timeout(Duration::from_secs(2), service.cancel_receiver(&session)).await;
    }
}
