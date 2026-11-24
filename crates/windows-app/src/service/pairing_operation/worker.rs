use std::{
    thread::JoinHandle,
    time::{Duration, SystemTime},
};

use receiver_core::pairing::{PairingMaterial, PairingOutcome, pairing_lifetime};
use tokio::{
    sync::oneshot,
    time::{Instant, sleep_until, timeout},
};

use super::{
    PairingProgress, PairingStart, ProgressUpdates, RegistrationStore, Service, SessionState,
    watch_session,
};
use crate::service::{
    CreateError, DeliveryStatus, PairingSession, enrollment::finish_enrollment,
    pairing_session_view, reconnect::Reconnect,
};

pub(super) fn shutdown(
    worker: Option<(oneshot::Sender<()>, JoinHandle<()>)>,
    store: Option<&RegistrationStore>,
) {
    if let Some((stop, worker)) = worker {
        let _ = stop.send(());
        let _ = worker.join();
    }
    if let Some(store) = store {
        store.flush_cleanup_on_shutdown();
    }
}

pub(super) fn startup_status(
    start: PairingStart,
    store: &RegistrationStore,
) -> Option<DeliveryStatus> {
    store.resume_cleanup();
    if !matches!(start, PairingStart::Registered) || store.receiver_credential().is_ok() {
        return None;
    }
    Some(if store.receiver_revoked() {
        DeliveryStatus::Revoked
    } else {
        DeliveryStatus::MissingCredential
    })
}

async fn receive_registered(
    service: &Service,
    store: &RegistrationStore,
    updates: &ProgressUpdates,
) {
    let Ok((receiver_id, token)) = store.receiver_credential() else {
        updates.delivery_status(DeliveryStatus::MissingCredential);
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
    let existing = if matches!(start, PairingStart::AddSender) {
        let Some(credential) = store.and_then(|store| store.receiver_credential().ok()) else {
            updates.publish(PairingProgress::Finished(PairingOutcome::CreateFailed));
            return;
        };
        Some(credential)
    } else {
        None
    };
    let create = async {
        match existing {
            Some((id, token)) => service.add_sender(id, &token).await,
            None => service.new_receiver().await,
        }
    };
    drive_session(service, updates, stopping, now, material, store, create).await;
}

async fn drive_session(
    service: &Service,
    updates: &ProgressUpdates,
    mut stopping: oneshot::Receiver<()>,
    now: impl FnOnce() -> SystemTime,
    material: PairingMaterial,
    store: Option<&RegistrationStore>,
    create: impl Future<Output = Result<PairingSession, CreateError>>,
) {
    let created = tokio::select! {
        result = create => result,
        _ = &mut stopping => {
            updates.publish(PairingProgress::Finished(PairingOutcome::Canceled));
            return;
        }
    };
    let Ok(session) = created else {
        updates.publish(PairingProgress::Finished(PairingOutcome::CreateFailed));
        return;
    };
    let pending_add = store
        .filter(|_| session.mode == pairing_session_view::Mode::AddSender)
        .map(|store| store.pending_add_guard(&session.session_id));
    let lifetime = pairing_lifetime(
        now(),
        session.expires_at.into(),
        session.pairing_token_expires_at.into(),
    );
    let deadline = Instant::now() + lifetime;
    let (mut outcome, exiting) = tokio::select! {
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
    if session.mode == pairing_session_view::Mode::AddSender
        && let Some(store) = store
    {
        let completed = outcome == PairingOutcome::KeyConfirmed(SessionState::Completed);
        let result = store.finish_add_sender(&session.session_id, &session.receiver_id, completed);
        drop(pending_add);
        if completed {
            if let Ok(Some(pairing_id)) = result {
                updates.publish(PairingProgress::Completed { pairing_id });
                return;
            }
            outcome = PairingOutcome::Failed;
        }
    } else if let Some(store) = store {
        let _ = store.discard_enrollment_key(&session.session_id);
    }
    updates.publish(PairingProgress::Finished(outcome));
    if exiting
        || matches!(
            outcome,
            PairingOutcome::CreateFailed | PairingOutcome::Failed
        )
    {
        let _ = timeout(Duration::from_secs(2), service.cancel_session(&session)).await;
    }
}
