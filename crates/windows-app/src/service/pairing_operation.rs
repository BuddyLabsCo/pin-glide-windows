use std::{
    path::PathBuf,
    sync::{Arc, Mutex},
    thread::{self, JoinHandle},
    time::{Duration, SystemTime},
};

use qrcode::QrCode;
use receiver_core::{
    delivery::Inbox,
    pairing::{PairingMaterial, PairingOutcome},
};
use reqwest::StatusCode;
use tokio::{
    sync::{oneshot, watch},
    time::{Instant, sleep_until, timeout},
};
use zeroize::Zeroizing;

use super::{
    CompletionError, DeliveryStatus, NewReceiverSession, Service, SessionState,
    delivery::ReceivedSms,
    pairing_events::{PairingEvent, PairingEventError, PairingEvents},
    pairing_session_view,
    pairing_snapshot::{Claim, Snapshot},
};
use crate::storage::{PairingReference, RegistrationStore};

#[cfg(test)]
pub(crate) mod tests;
mod worker;
use worker::drive;

#[derive(Clone)]
pub enum PairingProgress {
    Preparing,
    Waiting { code: Arc<QrCode>, seconds: u64 },
    Confirming,
    CredentialReady,
    Checking,
    Connecting,
    Completed { pairing_id: String },
    Finished(PairingOutcome),
}

pub struct PairingOperation {
    progress: watch::Receiver<PairingProgress>,
    delivery: watch::Receiver<DeliveryStatus>,
    worker: Option<(oneshot::Sender<()>, JoinHandle<()>)>,
    inbox: Arc<Mutex<Inbox>>,
    store: Option<RegistrationStore>,
}

#[derive(Clone, Copy)]
pub enum PairingStart {
    ResumePending,
    Fresh,
    Registered,
    AddSender,
}

pub(super) struct ProgressUpdates {
    snapshot: watch::Sender<PairingProgress>,
    pub(super) delivery: watch::Sender<DeliveryStatus>,
    notify: fn(),
    pub(super) inbox: Arc<Mutex<Inbox>>,
    pub(super) notify_sms: fn(ReceivedSms),
}

impl ProgressUpdates {
    pub(super) fn delivery_status(&self, status: DeliveryStatus) {
        if self.delivery.send_if_modified(|previous| {
            if *previous == status {
                return false;
            }
            *previous = status;
            true
        }) {
            (self.notify)();
        }
    }

    pub(super) fn publish(&self, progress: PairingProgress) {
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
    pub fn start(
        connect: impl FnOnce() -> Option<Service> + Send + 'static,
        notify: fn(),
        start: PairingStart,
    ) -> Self {
        Self::start_receiving(connect, notify, start, drop)
    }

    pub(crate) fn start_receiving(
        connect: impl FnOnce() -> Option<Service> + Send + 'static,
        notify: fn(),
        start: PairingStart,
        notify_sms: fn(ReceivedSms),
    ) -> Self {
        Self::spawn(
            connect,
            notify,
            RegistrationStore::for_current_user().ok(),
            start,
            notify_sms,
        )
    }

    pub(crate) fn has_pending_enrollment() -> bool {
        RegistrationStore::for_current_user()
            .and_then(|store| store.enrollment())
            .is_ok_and(|pending| pending.is_some())
    }

    pub fn start_with_registration(
        connect: impl FnOnce() -> Option<Service> + Send + 'static,
        notify: fn(),
        path: PathBuf,
        start: PairingStart,
    ) -> Self {
        let store = RegistrationStore::new(path);
        Self::spawn(connect, notify, Some(store), start, drop)
    }

    pub(crate) fn spawn(
        connect: impl FnOnce() -> Option<Service> + Send + 'static,
        notify: fn(),
        store: Option<RegistrationStore>,
        start: PairingStart,
        notify_sms: fn(ReceivedSms),
    ) -> Self {
        let (snapshot, progress) = watch::channel(PairingProgress::Preparing);
        let (delivery_updates, delivery) = watch::channel(DeliveryStatus::Unregistered);
        let (stop, stopping) = oneshot::channel();
        let inbox = Arc::new(Mutex::new(Inbox::default()));
        let updates = ProgressUpdates {
            snapshot: snapshot.clone(),
            delivery: delivery_updates,
            notify,
            inbox: Arc::clone(&inbox),
            notify_sms,
        };
        let window_store = store.clone();
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
                        runtime.block_on(drive(
                            &service,
                            &updates,
                            stopping,
                            SystemTime::now,
                            store.as_ref(),
                            start,
                        ));
                    }
                    _ => {
                        updates.delivery_status(DeliveryStatus::Unconfigured);
                        updates.publish(PairingProgress::Finished(PairingOutcome::CreateFailed));
                    }
                }
            });
        let worker = worker.ok().map(|worker| (stop, worker));
        if worker.is_none() {
            snapshot.send_replace(PairingProgress::Finished(PairingOutcome::CreateFailed));
            notify();
        }
        Self {
            progress,
            delivery,
            worker,
            inbox,
            store: window_store,
        }
    }

    #[cfg(windows)]
    pub(crate) fn registration(&self) -> Option<&RegistrationStore> {
        self.store.as_ref()
    }

    #[cfg(windows)]
    pub fn sender_label(&self, pairing_id: &str) -> std::io::Result<Option<String>> {
        match self.registration() {
            Some(store) => super::delivery::sender_label(store, pairing_id),
            None => Ok(None),
        }
    }

    pub fn current(&self) -> PairingProgress {
        self.progress.borrow().clone()
    }

    pub fn delivery_status(&self) -> DeliveryStatus {
        let status = *self.delivery.borrow();
        if status == DeliveryStatus::Ready
            && let Some(store) = &self.store
        {
            return super::reconnect::connected_status(store);
        }
        status
    }

    pub fn latest_otp(&self) -> Option<Zeroizing<String>> {
        Some(Zeroizing::new(self.inbox.lock().ok()?.latest()?.to_owned()))
    }

    pub fn shutdown(mut self) {
        if let Some((stop, worker)) = self.worker.take() {
            let _ = stop.send(());
            let _ = worker.join();
        }
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
        PairingEvent::State(ref snapshot)
            if snapshot.state == SessionState::AwaitingAuthorization =>
        {
            match wait_for_claim(&mut events, session, material, deadline, updates).await {
                Ok(event) => event,
                Err(outcome) => return outcome,
            }
        }
        event => event,
    };
    let PairingEvent::State(snapshot) = first else {
        return PairingOutcome::Failed;
    };
    if snapshot.state != SessionState::AwaitingConfirmation {
        return outcome(snapshot.state, None);
    }
    let Some(claim) = snapshot.into_claim() else {
        return PairingOutcome::Failed;
    };
    updates.publish(PairingProgress::Confirming);
    if let Err(outcome) = verify_and_save(&mut events, session, &claim, material, store).await {
        return outcome;
    }
    let milestone = if session.mode == pairing_session_view::Mode::AddSender {
        SessionState::Completed
    } else {
        SessionState::CredentialReady
    };
    wait_for_confirmation(
        service.confirm_receiver(session),
        confirmation_milestone(&mut events, &claim, milestone),
        &claim,
        milestone,
    )
    .await
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
        if !matches!(&event, PairingEvent::State(snapshot) if snapshot.state == SessionState::AwaitingAuthorization)
        {
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
            PairingEvent::State(snapshot) if snapshot.matches_claim(claim) => {
                if snapshot.state != SessionState::AwaitingConfirmation {
                    return Err(outcome(snapshot.state, None));
                }
            }
            PairingEvent::Relay(payload) => return Ok(payload),
            _ => return Err(PairingOutcome::Failed),
        }
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
    let store = store.ok_or(PairingOutcome::Failed)?;
    store
        .commit_provisional(
            &reference,
            key.as_bytes(),
            session.mode == pairing_session_view::Mode::AddSender,
        )
        .map_err(|_| PairingOutcome::Failed)
}

async fn wait_for_confirmation(
    report: impl Future<Output = Result<Snapshot, CompletionError>>,
    milestone: impl Future<Output = Option<PairingOutcome>>,
    claim: &Claim,
    expected: SessionState,
) -> PairingOutcome {
    let mut milestone = std::pin::pin!(milestone);
    let response = tokio::select! {
        biased;
        state = &mut milestone => return state.unwrap_or(PairingOutcome::Failed),
        response = report => response,
    };
    let fallback = match response {
        Ok(snapshot) if snapshot.matches_claim(claim) && snapshot.receiver_confirmed => {
            if snapshot.state == SessionState::AwaitingConfirmation {
                return milestone.await.unwrap_or(PairingOutcome::Failed);
            }
            return outcome(snapshot.state, Some(expected));
        }
        Err(CompletionError::Rejected {
            status: StatusCode::GONE,
            code,
        }) if code.as_deref() == Some("pairing_session_closed") => {
            return milestone.await.unwrap_or(PairingOutcome::Failed);
        }
        Err(CompletionError::Rejected {
            status: StatusCode::GONE,
            code,
        }) if code.as_deref() == Some("pairing_session_expired") => PairingOutcome::Expired,
        _ => PairingOutcome::Failed,
    };
    timeout(Duration::from_secs(2), milestone)
        .await
        .ok()
        .flatten()
        .unwrap_or(fallback)
}

async fn confirmation_milestone(
    events: &mut PairingEvents,
    claim: &Claim,
    expected: SessionState,
) -> Option<PairingOutcome> {
    while let Ok(event) = events.next_event().await {
        match event {
            PairingEvent::State(snapshot) if snapshot.matches_claim(claim) => {
                if snapshot.state != SessionState::AwaitingConfirmation {
                    return Some(outcome(snapshot.state, Some(expected)));
                }
            }
            PairingEvent::CredentialReady => {}
            _ => break,
        }
    }
    None
}

fn outcome(state: SessionState, expected: Option<SessionState>) -> PairingOutcome {
    match state {
        SessionState::Canceled => PairingOutcome::Canceled,
        SessionState::Expired => PairingOutcome::Expired,
        state if Some(state) == expected => PairingOutcome::KeyConfirmed(state),
        _ => PairingOutcome::Failed,
    }
}
