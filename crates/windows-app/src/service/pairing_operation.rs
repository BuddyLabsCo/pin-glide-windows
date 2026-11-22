use std::{
    sync::Arc,
    thread::{self, JoinHandle},
    time::{Duration, SystemTime},
};

use qrcode::QrCode;
use receiver_core::pairing::{PairingMaterial, PairingOutcome, PairingPhase, pairing_lifetime};
use reqwest::StatusCode;
use tokio::{
    sync::{oneshot, watch},
    time::{Instant, sleep_until, timeout},
};
use zeroize::Zeroizing;

use super::{
    NewReceiverSession, Service,
    pairing_events::{PairingEventError, PairingEvents},
};

#[cfg(test)]
mod tests;

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

impl PairingOperation {
    pub fn start(origin: Option<&'static str>) -> Self {
        let (updates, progress) = watch::channel(PairingProgress::Preparing);
        let (stop, stopping) = oneshot::channel();
        let worker_updates = updates.clone();
        let worker = thread::Builder::new()
            .name("pairing".into())
            .spawn(move || {
                match (
                    origin.and_then(|origin| Service::new(origin).ok()),
                    tokio::runtime::Builder::new_current_thread()
                        .enable_all()
                        .build(),
                ) {
                    (Some(service), Ok(runtime)) => {
                        runtime.block_on(drive(
                            &service,
                            &worker_updates,
                            stopping,
                            SystemTime::now,
                        ));
                    }
                    _ => {
                        worker_updates
                            .send_replace(PairingProgress::Finished(PairingOutcome::CreateFailed));
                    }
                }
            });
        let worker = worker.ok().map(|worker| (stop, worker));
        if worker.is_none() {
            updates.send_replace(PairingProgress::Finished(PairingOutcome::CreateFailed));
        }
        Self { progress, worker }
    }

    pub fn current(&self) -> PairingProgress {
        self.progress.borrow().clone()
    }
}

impl Drop for PairingOperation {
    fn drop(&mut self) {
        if let Some((stop, worker)) = self.worker.take() {
            let _ = stop.send(());
            let _ = worker.join();
        }
    }
}

async fn drive(
    service: &Service,
    updates: &watch::Sender<PairingProgress>,
    stopping: oneshot::Receiver<()>,
    now: impl FnOnce() -> SystemTime,
) {
    let outcome = run(service, updates, stopping, now).await;
    updates.send_replace(PairingProgress::Finished(outcome));
}

async fn run(
    service: &Service,
    updates: &watch::Sender<PairingProgress>,
    mut stopping: oneshot::Receiver<()>,
    now: impl FnOnce() -> SystemTime,
) -> PairingOutcome {
    let Ok(material) = PairingMaterial::generate() else {
        return PairingOutcome::CreateFailed;
    };
    let session = tokio::select! {
        result = service.new_receiver() => match result {
            Ok(session) => session,
            Err(_) => return PairingOutcome::CreateFailed,
        },
        _ = &mut stopping => return PairingOutcome::Canceled,
    };
    let lifetime = pairing_lifetime(
        now(),
        session.expires_at.into(),
        session.pairing_token_expires_at.into(),
    );
    let deadline = Instant::now() + lifetime;
    let (outcome, exiting) = tokio::select! {
        outcome = observe(service, &session, &material, deadline, updates) => (outcome, false),
        _ = &mut stopping => (PairingOutcome::Canceled, true),
    };
    if exiting
        || matches!(
            outcome,
            PairingOutcome::CreateFailed | PairingOutcome::Failed
        )
    {
        let _ = timeout(Duration::from_secs(2), service.cancel_receiver(&session)).await;
    }
    outcome
}

async fn observe(
    service: &Service,
    session: &NewReceiverSession,
    material: &PairingMaterial,
    deadline: Instant,
    updates: &watch::Sender<PairingProgress>,
) -> PairingOutcome {
    tokio::select! {
        biased;
        _ = sleep_until(deadline) => PairingOutcome::Expired,
        outcome = watch_session(service, session, material, deadline, updates) => outcome,
    }
}

async fn watch_session(
    service: &Service,
    session: &NewReceiverSession,
    material: &PairingMaterial,
    deadline: Instant,
    updates: &watch::Sender<PairingProgress>,
) -> PairingOutcome {
    let (mut events, first) = match timeout(Duration::from_secs(15), async {
        let mut events = service.pairing_events(session).await?;
        let first = events.next_state().await?;
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
    let phase = match PairingPhase::Preparing.advance(first) {
        Ok(phase) => phase,
        Err(outcome) => return outcome,
    };
    let code = if phase == PairingPhase::Waiting {
        let Some(payload) = material.qr_payload(&session.session_id, &session.receiver_id) else {
            return PairingOutcome::CreateFailed;
        };
        let Ok(code) = QrCode::new(Zeroizing::new(payload).as_bytes()) else {
            return PairingOutcome::CreateFailed;
        };
        Some(Arc::new(code))
    } else {
        None
    };
    follow_session(phase, code, deadline, updates, &mut events).await
}

async fn follow_session(
    mut phase: PairingPhase,
    mut code: Option<Arc<QrCode>>,
    deadline: Instant,
    updates: &watch::Sender<PairingProgress>,
    events: &mut PairingEvents,
) -> PairingOutcome {
    loop {
        if phase == PairingPhase::Confirming {
            code = None;
        }
        let countdown = if let Some(code) = &code {
            let remaining = deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                return PairingOutcome::Expired;
            }
            let seconds = remaining.as_secs() + u64::from(remaining.subsec_nanos() != 0);
            updates.send_replace(PairingProgress::Waiting {
                code: Arc::clone(code),
                seconds,
            });
            sleep_until(deadline - Duration::from_secs(seconds - 1))
        } else {
            updates.send_replace(PairingProgress::Confirming);
            sleep_until(deadline)
        };
        let event = tokio::select! {
            event = events.next_state() => event,
            _ = countdown, if code.is_some() => continue,
        };
        let Ok(state) = event else {
            return PairingOutcome::Failed;
        };
        phase = match phase.advance(state) {
            Ok(phase) => phase,
            Err(outcome) => return outcome,
        };
    }
}
