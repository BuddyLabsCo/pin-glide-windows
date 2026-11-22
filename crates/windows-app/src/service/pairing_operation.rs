use std::{
    sync::Arc,
    thread::{self, JoinHandle},
    time::{Duration, SystemTime},
};

use qrcode::QrCode;
use receiver_core::pairing::{PairingMaterial, PairingOutcome, pairing_lifetime};
use reqwest::StatusCode;
use tokio::{
    sync::{oneshot, watch},
    time::{Instant, sleep_until, timeout},
};
use zeroize::Zeroizing;

use super::{
    NewReceiverSession, Service, SessionState,
    pairing_events::{Claim, PairingEvent, PairingEventError, PairingEvents},
};
use crate::storage::{PairingReference, RegistrationStore};

#[cfg(test)]
pub(crate) mod tests;

#[derive(Clone)]
pub enum PairingProgress {
    Preparing,
    Waiting { code: Arc<QrCode>, seconds: u64 },
    Confirming,
    Finished(PairingOutcome),
}

pub struct PairingOperation {
    progress: watch::Receiver<PairingProgress>,
    worker: Option<(oneshot::Sender<()>, JoinHandle<()>)>,
}

struct ProgressUpdates {
    snapshot: watch::Sender<PairingProgress>,
    notify: fn(),
}

impl ProgressUpdates {
    fn publish(&self, progress: PairingProgress) {
        if self.snapshot.send_if_modified(|snapshot| {
            let unchanged = match (&*snapshot, &progress) {
                (
                    PairingProgress::Waiting { seconds: shown, .. },
                    PairingProgress::Waiting { seconds, .. },
                ) => shown == seconds,
                (PairingProgress::Confirming, PairingProgress::Confirming) => true,
                _ => false,
            };
            if !unchanged {
                *snapshot = progress;
            }
            !unchanged
        }) {
            (self.notify)();
        }
    }
}

impl PairingOperation {
    pub fn start(connect: impl FnOnce() -> Option<Service> + Send + 'static, notify: fn()) -> Self {
        let (snapshot, progress) = watch::channel(PairingProgress::Preparing);
        let (stop, stopping) = oneshot::channel();
        let updates = ProgressUpdates {
            snapshot: snapshot.clone(),
            notify,
        };
        let worker = thread::Builder::new()
            .name("pairing".into())
            .spawn(move || {
                match (
                    connect(),
                    tokio::runtime::Builder::new_current_thread()
                        .enable_all()
                        .build(),
                ) {
                    (Some(service), Ok(runtime)) => {
                        runtime.block_on(drive(&service, &updates, stopping, SystemTime::now));
                    }
                    _ => {
                        updates.publish(PairingProgress::Finished(PairingOutcome::CreateFailed));
                    }
                }
            });
        let worker = worker.ok().map(|worker| (stop, worker));
        if worker.is_none() {
            snapshot.send_replace(PairingProgress::Finished(PairingOutcome::CreateFailed));
            notify();
        }
        Self { progress, worker }
    }

    pub fn current(&self) -> PairingProgress {
        self.progress.borrow().clone()
    }

    pub fn shutdown(mut self) {
        if let Some((stop, worker)) = self.worker.take() {
            let _ = stop.send(());
            let _ = worker.join();
        }
    }
}

async fn drive(
    service: &Service,
    updates: &ProgressUpdates,
    stopping: oneshot::Receiver<()>,
    now: impl FnOnce() -> SystemTime,
) {
    let Ok(material) = PairingMaterial::generate() else {
        updates.publish(PairingProgress::Finished(PairingOutcome::CreateFailed));
        return;
    };
    drive_session(service, updates, stopping, now, material, None).await;
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
        outcome = watch_session(service, &session, &material, deadline, updates, store) => (outcome, false),
        _ = sleep_until(deadline) => (PairingOutcome::Expired, false),
        _ = &mut stopping => (PairingOutcome::Canceled, true),
    };
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

async fn watch_session(
    service: &Service,
    session: &NewReceiverSession,
    material: &PairingMaterial,
    deadline: Instant,
    updates: &ProgressUpdates,
    store: Option<&RegistrationStore>,
) -> PairingOutcome {
    let (mut events, first) = match timeout(Duration::from_secs(15), async {
        let mut events = service.pairing_events(session).await?;
        let first = events.next_event().await?;
        Ok((events, first))
    })
    .await
    {
        Ok(Ok(initial)) => initial,
        Ok(Err(PairingEventError::Rejected { status, code }))
            if matches!(
                (status, code.as_deref()),
                (StatusCode::GONE, Some("pairing_session_expired"))
                    | (StatusCode::UNAUTHORIZED, Some("credential_expired"))
            ) =>
        {
            return PairingOutcome::Expired;
        }
        _ => return PairingOutcome::CreateFailed,
    };
    let first = match first {
        PairingEvent::State(SessionState::AwaitingAuthorization, _) => {
            match wait_for_claim(&mut events, session, material, deadline, updates).await {
                Ok(event) => event,
                Err(outcome) => return outcome,
            }
        }
        event => event,
    };
    match first {
        PairingEvent::State(SessionState::AwaitingConfirmation, Some(claim)) => {
            updates.publish(PairingProgress::Confirming);
            check_key(service, session, &claim, material, &mut events, store).await
        }
        event => outcome(event, false),
    }
}

async fn wait_for_claim(
    events: &mut PairingEvents,
    session: &NewReceiverSession,
    material: &PairingMaterial,
    deadline: Instant,
    updates: &ProgressUpdates,
) -> Result<PairingEvent, PairingOutcome> {
    let payload = material
        .qr_payload(&session.session_id, &session.receiver_id)
        .ok_or(PairingOutcome::CreateFailed)?;
    let code = QrCode::new(Zeroizing::new(payload).as_bytes())
        .map(Arc::new)
        .map_err(|_| PairingOutcome::CreateFailed)?;
    loop {
        let remaining = deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            return Err(PairingOutcome::Expired);
        }
        let seconds = remaining.as_secs() + u64::from(remaining.subsec_nanos() != 0);
        updates.publish(PairingProgress::Waiting {
            code: Arc::clone(&code),
            seconds,
        });
        let event = tokio::select! {
            event = events.next_event() => event.map_err(|_| PairingOutcome::Failed)?,
            _ = sleep_until(deadline - Duration::from_secs(seconds - 1)) => continue,
        };
        if !matches!(
            event,
            PairingEvent::State(SessionState::AwaitingAuthorization, _)
        ) {
            return Ok(event);
        }
    }
}

async fn next_relay(events: &mut PairingEvents, claim: &Claim) -> Result<String, PairingOutcome> {
    loop {
        match events
            .next_event()
            .await
            .map_err(|_| PairingOutcome::Failed)?
        {
            PairingEvent::State(SessionState::AwaitingConfirmation, Some(current))
                if &current == claim => {}
            PairingEvent::Relay(payload) => return Ok(payload),
            event => return Err(outcome(event, false)),
        }
    }
}

async fn check_key(
    service: &Service,
    session: &NewReceiverSession,
    claim: &Claim,
    material: &PairingMaterial,
    events: &mut PairingEvents,
    store: Option<&RegistrationStore>,
) -> PairingOutcome {
    match verify_and_save(events, session, claim, material, store).await {
        Ok(()) => wait_for_confirmation(service, session, claim, events).await,
        Err(outcome) => outcome,
    }
}

async fn verify_and_save(
    events: &mut PairingEvents,
    session: &NewReceiverSession,
    claim: &Claim,
    material: &PairingMaterial,
    store: Option<&RegistrationStore>,
) -> Result<(), PairingOutcome> {
    let init = next_relay(events, claim).await?;
    let (pending, reply) = material
        .begin_confirmation(&session.session_id, &init)
        .map_err(|_| PairingOutcome::Failed)?;
    events
        .send_relay(reply)
        .await
        .map_err(|_| PairingOutcome::Failed)?;
    let reply = next_relay(events, claim).await?;
    let key = pending.verify(&reply).map_err(|_| PairingOutcome::Failed)?;
    let reference = PairingReference {
        session_id: &session.session_id,
        receiver_id: &session.receiver_id,
        pairing_id: &claim.pairing_id,
        sender_id: &claim.sender_id,
    };
    let local;
    let store = match store {
        Some(store) => store,
        None => {
            local = RegistrationStore::for_current_user().map_err(|_| PairingOutcome::Failed)?;
            &local
        }
    };
    store
        .commit_provisional(&reference, key.as_bytes())
        .map_err(|_| PairingOutcome::Failed)
}

async fn wait_for_confirmation(
    service: &Service,
    session: &NewReceiverSession,
    claim: &Claim,
    events: &mut PairingEvents,
) -> PairingOutcome {
    let mut report = std::pin::pin!(service.confirm_receiver(session, claim));
    let mut sent = false;
    loop {
        tokio::select! {
            event = events.next_event() => match event {
                Ok(PairingEvent::State(SessionState::AwaitingConfirmation, Some(current)))
                    if &current == claim => {}
                Ok(PairingEvent::State(_, Some(ref current))) if current != claim => {
                    return PairingOutcome::Failed;
                }
                Ok(event) => return outcome(event, true),
                Err(_) => return PairingOutcome::Failed,
            },
            response = &mut report, if !sent => match response {
                Some(SessionState::AwaitingConfirmation) => sent = true,
                Some(state) => return outcome(PairingEvent::State(state, None), true),
                None => return PairingOutcome::Failed,
            },
        }
    }
}

fn outcome(event: PairingEvent, saved: bool) -> PairingOutcome {
    match event {
        PairingEvent::State(SessionState::Canceled, _) => PairingOutcome::Canceled,
        PairingEvent::State(SessionState::Expired, _) => PairingOutcome::Expired,
        PairingEvent::State(
            SessionState::CredentialReady
            | SessionState::CredentialIssued
            | SessionState::Completed,
            _,
        )
        | PairingEvent::CredentialReady
            if saved =>
        {
            PairingOutcome::KeyConfirmed
        }
        _ => PairingOutcome::Failed,
    }
}
