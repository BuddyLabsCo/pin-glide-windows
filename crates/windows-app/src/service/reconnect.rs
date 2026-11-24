use std::{sync::LazyLock, time::Duration};

use reqwest::StatusCode;
use tokio::{
    sync::watch,
    time::{Instant, sleep_until, timeout},
};

use super::{
    Service,
    delivery::{DeliveryConnection, start_processing},
    pairing_events::PairingEventError,
    pairing_operation::ProgressUpdates,
};
use crate::storage::RegistrationStore;

#[cfg(test)]
mod tests;

const BACKOFF: [u64; 6] = [1, 2, 4, 8, 16, 30];

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DeliveryStatus {
    Unregistered,
    Connecting,
    Ready,
    Disconnected,
    Occupied,
    Protocol,
    Heartbeat,
    Suspended,
    Credential,
    MissingCredential,
    Revoked,
    Unconfigured,
}

#[derive(Clone, Copy, Default)]
pub(crate) struct Connectivity {
    pub(super) suspended: bool,
    pub(super) resumes: u64,
    improvements: u64,
}

static CONNECTIVITY: LazyLock<watch::Sender<Connectivity>> =
    LazyLock::new(|| watch::channel(Connectivity::default()).0);

#[cfg(windows)]
pub(crate) fn power_changed(suspended: bool) {
    CONNECTIVITY.send_if_modified(|state| {
        if state.suspended == suspended {
            return false;
        }
        state.suspended = suspended;
        if !suspended {
            state.resumes += 1;
        }
        true
    });
}

#[cfg(windows)]
pub(crate) fn connectivity_improved() {
    CONNECTIVITY.send_modify(|state| state.improvements += 1);
}

pub(super) struct Reconnect<'a> {
    service: &'a Service,
    token: &'a str,
    receiver_id: &'a str,
    updates: &'a ProgressUpdates,
    state: watch::Receiver<Connectivity>,
    backoff: usize,
}

impl<'a> Reconnect<'a> {
    pub(super) fn new(
        service: &'a Service,
        token: &'a str,
        receiver_id: &'a str,
        updates: &'a ProgressUpdates,
    ) -> Self {
        Self {
            service,
            token,
            receiver_id,
            updates,
            state: CONNECTIVITY.subscribe(),
            backoff: 0,
        }
    }

    pub(super) async fn open(
        &mut self,
        enrolling: bool,
    ) -> Result<DeliveryConnection, PairingEventError> {
        loop {
            let power = *self.state.borrow_and_update();
            if power.suspended {
                self.updates.delivery_status(DeliveryStatus::Suspended);
                let _ = self.state.wait_for(|state| !state.suspended).await;
                continue;
            }
            if self.backoff == 0 {
                self.updates.delivery_status(DeliveryStatus::Connecting);
            }
            let result = tokio::select! {
                biased;
                _ = self.state.wait_for(|state| state.suspended || state.resumes != power.resumes) => continue,
                result = timeout(Duration::from_secs(15), DeliveryConnection::open(
                    self.service, self.token,
                )) => result.unwrap_or(Err(PairingEventError::Transport)),
            };
            match result {
                Ok(socket) => {
                    self.backoff = 0;
                    return Ok(socket);
                }
                Err(error) => {
                    self.updates.delivery_status(upgrade_failure(&error));
                    if stops_retrying(&error, enrolling) {
                        return Err(error);
                    }
                    self.delay(power).await;
                }
            }
        }
    }

    pub(super) async fn receive(
        &mut self,
        store: &RegistrationStore,
        mut connection: DeliveryConnection,
    ) {
        let mut order = 0;
        let authenticate = start_processing(store, self.updates, self.receiver_id.to_owned());
        loop {
            self.updates.delivery_status(DeliveryStatus::Ready);
            let loss = connection
                .run(self.updates, &mut self.state, &mut order, &authenticate)
                .await;
            self.updates.delivery_status(loss.status);
            if loss.status == DeliveryStatus::Revoked {
                return;
            }
            if !loss.resume_failed {
                let power = *self.state.borrow_and_update();
                self.delay(power).await;
            }
            let Ok(next) = self.open(false).await else {
                return;
            };
            connection = next;
        }
    }

    async fn delay(&mut self, previous: Connectivity) {
        let base = BACKOFF[self.backoff];
        self.backoff = (self.backoff + 1).min(BACKOFF.len() - 1);
        let mut jitter = [128];
        let _ = getrandom::fill(&mut jitter);
        let wait = Duration::from_millis(base * (800 + u64::from(jitter[0]) * 400 / 255));
        tokio::select! {
            _ = sleep_until(Instant::now() + wait) => {}
            _ = self.state.wait_for(|state| {
                state.suspended || state.resumes != previous.resumes
                    || state.improvements != previous.improvements
            }) => {}
        }
    }
}

pub(super) fn connected_status(store: &RegistrationStore) -> DeliveryStatus {
    match store.has_pairings() {
        Ok(true) => DeliveryStatus::Ready,
        Ok(false) => DeliveryStatus::Unregistered,
        Err(_) => DeliveryStatus::Credential,
    }
}

fn upgrade_failure(error: &PairingEventError) -> DeliveryStatus {
    match error {
        PairingEventError::Rejected {
            status: StatusCode::CONFLICT,
            code,
        } if code.as_deref() == Some("receiver_already_connected") => DeliveryStatus::Occupied,
        PairingEventError::Rejected {
            status: StatusCode::UNAUTHORIZED | StatusCode::FORBIDDEN,
            ..
        } => DeliveryStatus::Credential,
        PairingEventError::Protocol => DeliveryStatus::Protocol,
        _ => DeliveryStatus::Disconnected,
    }
}

fn stops_retrying(error: &PairingEventError, enrolling: bool) -> bool {
    matches!(error, PairingEventError::Rejected { status, .. }
        if matches!(*status, StatusCode::UNAUTHORIZED | StatusCode::FORBIDDEN)
            || (enrolling && status.is_client_error() && *status != StatusCode::CONFLICT))
}
