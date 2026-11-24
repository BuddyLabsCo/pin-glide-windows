use std::{collections::VecDeque, sync::Arc, time::Duration};

use pin_glide_wss::{Delivery, DeviceRevoked, PairingRemoved, Receipt};
use receiver_core::delivery::{Arrival, Envelope, Inbox, Sms};
use serde::Deserialize;
use tokio::sync::{OwnedSemaphorePermit, Semaphore, TryAcquireError, mpsc, watch};
use tokio_tungstenite::tungstenite::Utf8Bytes;

use super::{
    DeliveryStatus, Service,
    pairing_events::{PairingEventError, Socket},
    pairing_operation::{ProgressUpdates, SmsReceiver},
    reconnect::Connectivity,
};

use crate::storage::{Cleanup, DeliveryRejection, RegistrationStore};

#[cfg(test)]
mod tests;
mod transport;
use transport::Transport;

pub(crate) struct ReceivedSms {
    #[cfg_attr(not(windows), allow(dead_code))]
    pub sms: Sms,
    #[cfg_attr(not(windows), allow(dead_code))]
    pub sender_name: String,
    #[cfg(windows)]
    pub epoch: u64,
    #[cfg(windows)]
    pub source: PopupSource,
}

#[cfg(windows)]
pub(crate) struct PopupSource {
    pairing_id: String,
    inbox: std::sync::Weak<std::sync::Mutex<Inbox>>,
}

#[cfg(windows)]
impl PopupSource {
    pub(crate) fn is_active(&self) -> bool {
        self.inbox.upgrade().is_some_and(|inbox| {
            inbox
                .lock()
                .is_ok_and(|inbox| inbox.accepts(&self.pairing_id))
        })
    }
}

const WRITE_LIMIT: Duration = Duration::from_secs(5);
const HEARTBEAT_LIMIT: Duration = Duration::from_secs(20);
pub(super) const MAX_PENDING_DELIVERIES: usize = 32;
const MAX_HELD_DELIVERIES: usize = 32;

#[derive(Debug)]
pub(super) struct ConnectionLoss {
    pub status: DeliveryStatus,
    pub resume_failed: bool,
}

pub(super) struct IncomingDelivery {
    text: Utf8Bytes,
    arrival: Arrival,
    hold_capacity: Option<OwnedSemaphorePermit>,
    #[cfg(windows)]
    epoch: Option<u64>,
}

pub(super) struct QueuedDelivery {
    incoming: IncomingDelivery,
    held_capacity: bool,
    _capacity: OwnedSemaphorePermit,
}

pub(super) struct DeliveryQueue {
    send: mpsc::Sender<QueuedDelivery>,
    store: RegistrationStore,
    receiver_id: String,
    hold_capacity: Arc<Semaphore>,
}

impl DeliveryQueue {
    fn control(
        &self,
        kind: &str,
        text: &str,
        updates: &ProgressUpdates,
    ) -> Result<(), DeliveryStatus> {
        match kind {
            "pairing_removed" => {
                let event: PairingRemoved =
                    serde_json::from_str(text).map_err(|_| DeliveryStatus::Protocol)?;
                correlation(Some(&event.pairing_id)).map_err(|_| DeliveryStatus::Protocol)?;
                correlation(Some(&event.sender_id)).map_err(|_| DeliveryStatus::Protocol)?;
                updates
                    .inbox
                    .lock()
                    .map_err(|_| DeliveryStatus::Disconnected)?
                    .remove_pairing(event.pairing_id.clone());
                self.store.record_cleanup(Cleanup::PairingRemoved {
                    receiver_id: self.receiver_id.clone(),
                    pairing_id: event.pairing_id,
                    sender_id: event.sender_id,
                });
                updates.refresh_delivery();
                Ok(())
            }
            "device_revoked" => {
                let event: DeviceRevoked =
                    serde_json::from_str(text).map_err(|_| DeliveryStatus::Protocol)?;
                if event.receiver_id != self.receiver_id {
                    return Err(DeliveryStatus::Protocol);
                }
                updates
                    .inbox
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner)
                    .revoke();
                self.store.record_cleanup(Cleanup::Revoked {
                    receiver_id: event.receiver_id,
                });
                Err(DeliveryStatus::Revoked)
            }
            _ => Ok(()),
        }
    }

    fn reserve_hold(
        &self,
        text: &str,
        header: &EventHeader,
    ) -> Result<Option<OwnedSemaphorePermit>, TryAcquireError> {
        if let Some(pending) = self.store.current_pending_add()
            && let Some(sender) = header.sender_id.as_deref()
            && let Ok(address) = serde_json::from_str::<DeliveryAddress>(text)
            && address.receiver_id == self.receiver_id
            && pending.matches(&self.receiver_id, sender, &address.pairing_id)
        {
            return self.hold_capacity.clone().try_acquire_owned().map(Some);
        }
        Ok(None)
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct EventHeader {
    version: i64,
    #[serde(rename = "type")]
    kind: String,
    sender_id: Option<String>,
    message_id: Option<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct DeliveryAddress {
    receiver_id: String,
    pairing_id: String,
}

#[cfg(windows)]
pub(super) fn sender_label(
    store: &RegistrationStore,
    pairing_id: &str,
) -> std::io::Result<Option<String>> {
    store.sender_name(pairing_id)
}

#[cfg(all(test, windows))]
#[test]
fn delivery_resolves_the_latest_name_for_the_pairing_id() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("registration.json");
    std::fs::write(
        &path,
        r#"{"version":4,"active_pairings":{"pair-1":{"sender_id":"sender-1","protected_key":[],"name":"Old"}}}"#,
    )
    .unwrap();
    let store = RegistrationStore::new(path);
    assert_eq!(
        sender_label(&store, "pair-1").unwrap().as_deref(),
        Some("Old")
    );
    store
        .name_sender("pair-1", &"Office".encode_utf16().collect::<Vec<_>>())
        .unwrap();
    assert_eq!(
        sender_label(&store, "pair-1").unwrap().as_deref(),
        Some("Office")
    );
    assert_eq!(sender_label(&store, "sender-1").unwrap(), None);
}

pub(super) struct DeliveryConnection {
    socket: Socket,
}

impl DeliveryConnection {
    pub(super) async fn open(service: &Service, token: &str) -> Result<Self, PairingEventError> {
        service
            .websocket("/v1/receiver/events", token)
            .await
            .map(|socket| Self { socket })
    }

    pub(super) async fn run(
        self,
        updates: &ProgressUpdates,
        state: &mut watch::Receiver<Connectivity>,
        order: &mut u64,
        authenticate: &DeliveryQueue,
    ) -> ConnectionLoss {
        Transport::new(self.socket, updates, Some(authenticate))
            .run(state, order, &authenticate.send)
            .await
    }
}

pub(super) fn start_processing(
    store: &RegistrationStore,
    updates: &ProgressUpdates,
    receiver_id: String,
) -> DeliveryQueue {
    let (send, mut receive) =
        mpsc::channel::<QueuedDelivery>(MAX_PENDING_DELIVERIES + MAX_HELD_DELIVERIES);
    let queue = DeliveryQueue {
        send,
        store: store.clone(),
        receiver_id: receiver_id.clone(),
        hold_capacity: Arc::new(Semaphore::new(MAX_HELD_DELIVERIES)),
    };
    let store = store.clone();
    let inbox = updates.inbox.clone();
    let notify = updates.notify_sms.clone();
    let mut pending = store.watch_pending_adds();
    let runtime = tokio::runtime::Handle::current();
    tokio::task::spawn_blocking(move || {
        let mut held = VecDeque::new();
        let process = |queued: QueuedDelivery, delivery: Result<Delivery, DeliveryRejection>| {
            let incoming = queued.incoming;
            #[cfg(windows)]
            let source = delivery.as_ref().ok().map(|delivery| PopupSource {
                pairing_id: delivery.pairing_id.clone(),
                inbox: Arc::downgrade(&inbox),
            });
            let result = delivery.and_then(|delivery| {
                authenticate(&receiver_id, delivery, incoming.arrival, &store, &inbox)
            });
            match result {
                Ok((sms, sender_name)) => match &notify {
                    SmsReceiver::Observe(receive) => receive(sms),
                    SmsReceiver::Popup(notify) => {
                        #[cfg(windows)]
                        let Some(epoch) = incoming.epoch else {
                            return;
                        };
                        notify(ReceivedSms {
                            sms,
                            sender_name,
                            #[cfg(windows)]
                            epoch,
                            #[cfg(windows)]
                            source: source.expect("an authenticated Delivery has a Pairing"),
                        });
                    }
                },
                Err(category) => {
                    let _ = store.record_delivery_rejection(category);
                }
            }
        };
        runtime.block_on(async {
            loop {
                let queued = tokio::select! {
                    biased;
                    _ = pending.changed(), if !held.is_empty() => None,
                    queued = receive.recv() => match queued {
                        Some(queued) => Some(queued),
                        None => break,
                    },
                };
                let snapshot = store.pending_add(&pending.borrow_and_update()).cloned();
                let is_pending = |delivery: &Delivery| {
                    snapshot.as_ref().is_some_and(|entry| {
                        entry.matches(
                            &delivery.receiver_id,
                            &delivery.sender_id,
                            &delivery.pairing_id,
                        )
                    })
                };
                for _ in 0..held.len() {
                    let (queued, delivery) = held.pop_front().unwrap();
                    if is_pending(&delivery) {
                        held.push_back((queued, delivery));
                    } else {
                        process(queued, Ok(delivery));
                    }
                }
                let Some(queued) = queued else { continue };
                match serde_json::from_str::<Delivery>(&queued.incoming.text) {
                    Ok(delivery) if delivery.receiver_id != receiver_id => {
                        process(queued, Err(DeliveryRejection::ReceiverMismatch));
                    }
                    Ok(delivery) if queued.held_capacity && is_pending(&delivery) => {
                        held.push_back((queued, delivery));
                    }
                    Ok(delivery) => process(queued, Ok(delivery)),
                    Err(_) => process(queued, Err(DeliveryRejection::InvalidEnvelope)),
                }
            }
        });
    });
    queue
}

fn receipt(header: &EventHeader) -> Result<Option<String>, PairingEventError> {
    if header.version != 1 {
        return Err(PairingEventError::Protocol);
    }
    match header.kind.as_str() {
        "delivery" => {}
        "error" | "pairing_removed" | "device_revoked" => return Ok(None),
        _ => return Err(PairingEventError::Protocol),
    }
    let receipt = Receipt {
        sender_id: correlation(header.sender_id.as_deref())?.into(),
        message_id: correlation(header.message_id.as_deref())?.into(),
        reserved_type: "receipt".into(),
        version: 1,
    };
    serde_json::to_string(&receipt)
        .map(Some)
        .map_err(|_| PairingEventError::Protocol)
}

fn authenticate(
    receiver_id: &str,
    delivery: Delivery,
    arrival: Arrival,
    store: &RegistrationStore,
    inbox: &std::sync::Mutex<Inbox>,
) -> Result<(Sms, String), DeliveryRejection> {
    let target = store.delivery_target(receiver_id, &delivery.pairing_id, &delivery.sender_id);
    let (key, sender_name) = target.map_or_else(
        || (None, String::new()),
        |target| (Some(target.key), target.name),
    );
    let mut inbox = inbox
        .lock()
        .map_err(|_| DeliveryRejection::InboxUnavailable)?;
    let result = inbox.receive(
        arrival,
        Envelope {
            pairing_id: &delivery.pairing_id,
            crypto_version: delivery.crypto_version,
            nonce: &delivery.nonce,
            ciphertext: &delivery.ciphertext,
        },
        |_| key,
    );
    let sms = result.map_err(DeliveryRejection::Receive)?;
    Ok((sms, sender_name))
}

fn correlation(value: Option<&str>) -> Result<&str, PairingEventError> {
    value
        .filter(|id| !id.trim().is_empty())
        .ok_or(PairingEventError::Protocol)
}
