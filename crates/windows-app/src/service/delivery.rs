use std::time::Duration;

use pin_glide_wss::{Delivery, Receipt};
use receiver_core::delivery::{Arrival, Envelope, Sms};
use serde::Deserialize;
use tokio::sync::{mpsc, watch};
use tokio_tungstenite::tungstenite::Utf8Bytes;

use super::{
    DeliveryStatus, Service,
    pairing_events::{PairingEventError, Socket},
    pairing_operation::ProgressUpdates,
    reconnect::Connectivity,
};

use crate::storage::{DeliveryRejection, RegistrationStore};

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
}

const WRITE_LIMIT: Duration = Duration::from_secs(5);
const HEARTBEAT_LIMIT: Duration = Duration::from_secs(20);
const MAX_PENDING_DELIVERIES: usize = 32;

#[derive(Debug)]
pub(super) struct ConnectionLoss {
    pub status: DeliveryStatus,
    pub resume_failed: bool,
}

pub(super) struct IncomingDelivery {
    text: Utf8Bytes,
    arrival: Arrival,
    #[cfg(windows)]
    epoch: Option<u64>,
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
        store: &RegistrationStore,
        updates: &ProgressUpdates,
        state: &mut watch::Receiver<Connectivity>,
        order: &mut u64,
        authenticate: &mpsc::Sender<IncomingDelivery>,
    ) -> ConnectionLoss {
        Transport::new(self.socket, store, updates)
            .run(state, order, authenticate)
            .await
    }
}

pub(super) fn start_processing(
    store: &RegistrationStore,
    updates: &ProgressUpdates,
    receiver_id: String,
) -> mpsc::Sender<IncomingDelivery> {
    let (send, mut receive) = mpsc::channel::<IncomingDelivery>(MAX_PENDING_DELIVERIES);
    let store = store.clone();
    let inbox = updates.inbox.clone();
    let notify = updates.notify_sms;
    tokio::task::spawn_blocking(move || {
        while let Some(incoming) = receive.blocking_recv() {
            let result = match serde_json::from_str::<Delivery>(&incoming.text) {
                Ok(delivery) if delivery.receiver_id == receiver_id => {
                    authenticate(&receiver_id, delivery, incoming.arrival, &store, &inbox)
                }
                Ok(_) => Err(DeliveryRejection::ReceiverMismatch),
                Err(_) => Err(DeliveryRejection::InvalidEnvelope),
            };
            match result {
                Ok((sms, sender_name)) => {
                    #[cfg(windows)]
                    let Some(epoch) = incoming.epoch else {
                        continue;
                    };
                    notify(ReceivedSms {
                        sms,
                        sender_name,
                        #[cfg(windows)]
                        epoch,
                    });
                }
                Err(category) => {
                    let _ = store.record_delivery_rejection(category);
                }
            }
        }
    });
    send
}

fn receipt(text: &str) -> Result<Option<String>, PairingEventError> {
    let header: EventHeader =
        serde_json::from_str(text).map_err(|_| PairingEventError::Protocol)?;
    if header.version != 1 {
        return Err(PairingEventError::Protocol);
    }
    match header.kind.as_str() {
        "delivery" => {}
        "error" | "pairing_removed" | "device_revoked" => return Ok(None),
        _ => return Err(PairingEventError::Protocol),
    }
    let receipt = Receipt {
        sender_id: correlation(header.sender_id)?,
        message_id: correlation(header.message_id)?,
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
    inbox: &std::sync::Mutex<receiver_core::delivery::Inbox>,
) -> Result<(Sms, String), DeliveryRejection> {
    let mut sender_name = String::new();
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
        |pairing_id| {
            let target = store.delivery_target(receiver_id, pairing_id, &delivery.sender_id)?;
            sender_name = target.name;
            Some(target.key)
        },
    );
    let sms = result.map_err(DeliveryRejection::Receive)?;
    Ok((sms, sender_name))
}

fn correlation(value: Option<String>) -> Result<String, PairingEventError> {
    value
        .filter(|id| !id.trim().is_empty())
        .ok_or(PairingEventError::Protocol)
}
