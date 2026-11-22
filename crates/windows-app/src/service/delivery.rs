use std::time::{Duration, SystemTime};

use futures_util::{Sink, SinkExt, StreamExt};
use pin_glide_wss::{Delivery, Receipt};
use receiver_core::delivery::{Arrival, Envelope, Sms};
use serde::Deserialize;
use tokio::{
    sync::watch,
    time::{Instant, timeout, timeout_at},
};
use tokio_tungstenite::tungstenite::{
    Error as SocketError, Message,
    error::ProtocolError,
    protocol::{CloseFrame, frame::coding::CloseCode},
};

use super::{
    DeliveryStatus, Service,
    pairing_events::{PairingEventError, Socket},
    pairing_operation::ProgressUpdates,
    reconnect::Connectivity,
};

use crate::storage::{DeliveryRejection, RegistrationStore};

#[cfg(test)]
mod tests;

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
    receiver_id: String,
}

impl DeliveryConnection {
    pub(super) async fn open(
        service: &Service,
        token: &str,
        receiver_id: &str,
    ) -> Result<Self, PairingEventError> {
        service
            .websocket("/v1/receiver/events", token)
            .await
            .map(|socket| Self {
                socket,
                receiver_id: receiver_id.into(),
            })
    }

    pub(super) async fn run(
        mut self,
        store: &RegistrationStore,
        updates: &ProgressUpdates,
        state: &mut watch::Receiver<Connectivity>,
        order: &mut u64,
    ) -> DeliveryStatus {
        let reason = self.read_frames(store, updates, state, order).await;
        if reason == DeliveryStatus::Protocol {
            let _ = timeout(
                WRITE_LIMIT,
                self.socket.close(Some(CloseFrame {
                    code: CloseCode::Protocol,
                    reason: "invalid_delivery_event".into(),
                })),
            )
            .await;
        }
        reason
    }

    async fn read_frames(
        &mut self,
        store: &RegistrationStore,
        updates: &ProgressUpdates,
        state: &mut watch::Receiver<Connectivity>,
        order: &mut u64,
    ) -> DeliveryStatus {
        let mut resumes = state.borrow().resumes;
        let mut heartbeat = Instant::now() + HEARTBEAT_LIMIT;
        let mut probe = None;
        loop {
            let power = *state.borrow_and_update();
            if power.suspended {
                updates.delivery_status(DeliveryStatus::Suspended);
                let _ = state.wait_for(|state| !state.suspended).await;
                continue;
            }
            if resumes != power.resumes {
                resumes = power.resumes;
                let payload = resumes.to_le_bytes();
                if !matches!(
                    timeout(
                        WRITE_LIMIT,
                        self.socket.send(Message::Ping(payload.to_vec().into()))
                    )
                    .await,
                    Ok(Ok(()))
                ) {
                    return DeliveryStatus::Disconnected;
                }
                heartbeat = Instant::now() + HEARTBEAT_LIMIT;
                probe = Some((payload, Instant::now() + WRITE_LIMIT));
                updates.delivery_status(DeliveryStatus::Connecting);
            }
            let deadline = probe.map_or(heartbeat, |(_, deadline)| deadline.min(heartbeat));
            if Instant::now() >= deadline {
                return DeliveryStatus::Heartbeat;
            }
            let frame = tokio::select! {
                biased;
                _ = state.wait_for(|state| state.suspended || state.resumes != resumes) => continue,
                frame = timeout_at(deadline, self.socket.next()) => frame,
            };
            let frame = match frame {
                Ok(Some(Ok(frame))) => frame,
                Ok(Some(Err(SocketError::Protocol(
                    ProtocolError::ResetWithoutClosingHandshake,
                )))) => {
                    return DeliveryStatus::Disconnected;
                }
                Ok(Some(Err(
                    SocketError::Protocol(_) | SocketError::Utf8(_) | SocketError::Capacity(_),
                ))) => {
                    return DeliveryStatus::Protocol;
                }
                Ok(_) => return DeliveryStatus::Disconnected,
                Err(_) => return DeliveryStatus::Heartbeat,
            };
            if frame.is_ping() {
                heartbeat = Instant::now() + HEARTBEAT_LIMIT;
            }
            if let Message::Pong(ref data) = frame
                && probe.is_some_and(|(expected, _)| data.as_ref() == expected)
            {
                probe = None;
                updates.delivery_status(DeliveryStatus::Ready);
            }
            if let Err(reason) = self.handle_frame(frame, store, updates, order).await {
                return reason;
            }
        }
    }

    async fn handle_frame(
        &mut self,
        frame: Message,
        store: &RegistrationStore,
        updates: &ProgressUpdates,
        order: &mut u64,
    ) -> Result<(), DeliveryStatus> {
        let result = match frame {
            Message::Text(text) => {
                *order += 1;
                let arrival = Arrival {
                    time: SystemTime::now(),
                    order: *order,
                };
                #[cfg(windows)]
                let epoch = crate::platform::popup_epoch();
                handle_text(
                    &mut self.socket,
                    &text,
                    &self.receiver_id,
                    store,
                    |delivery| {
                        authenticate(
                            &self.receiver_id,
                            delivery,
                            arrival,
                            store,
                            updates,
                            |sms, sender_name| {
                                #[cfg(windows)]
                                let Some(epoch) = epoch else { return };
                                (updates.notify_sms)(ReceivedSms {
                                    sms,
                                    sender_name,
                                    #[cfg(windows)]
                                    epoch,
                                });
                            },
                        )
                    },
                )
                .await
            }
            Message::Ping(_) | Message::Pong(_) => {
                match timeout(WRITE_LIMIT, self.socket.flush()).await {
                    Ok(Ok(())) => Ok(()),
                    _ => return Err(DeliveryStatus::Heartbeat),
                }
            }
            Message::Close(_) => {
                let _ = timeout(WRITE_LIMIT, self.socket.flush()).await;
                return Err(DeliveryStatus::Disconnected);
            }
            _ => return Err(DeliveryStatus::Protocol),
        };
        result.map_err(|error| match error {
            PairingEventError::Protocol => DeliveryStatus::Protocol,
            _ => DeliveryStatus::Disconnected,
        })
    }
}

async fn handle_text(
    socket: &mut (impl Sink<Message, Error = SocketError> + Unpin),
    text: &str,
    receiver_id: &str,
    store: &RegistrationStore,
    authenticate: impl FnOnce(Delivery) -> Result<(), DeliveryRejection>,
) -> Result<(), PairingEventError> {
    let header: EventHeader =
        serde_json::from_str(text).map_err(|_| PairingEventError::Protocol)?;
    if header.version != 1 {
        return Err(PairingEventError::Protocol);
    }
    match header.kind.as_str() {
        "delivery" => {}
        "error" | "pairing_removed" | "device_revoked" => return Ok(()),
        _ => return Err(PairingEventError::Protocol),
    }
    let receipt = Receipt {
        sender_id: correlation(header.sender_id)?,
        message_id: correlation(header.message_id)?,
        reserved_type: "receipt".into(),
        version: 1,
    };
    let receipt = serde_json::to_string(&receipt).map_err(|_| PairingEventError::Protocol)?;
    let sent = timeout(WRITE_LIMIT, socket.send(Message::text(receipt))).await;
    let authenticated = match serde_json::from_str::<Delivery>(text) {
        Ok(delivery) if delivery.receiver_id == receiver_id => authenticate(delivery),
        Ok(_) => Err(DeliveryRejection::ReceiverMismatch),
        Err(_) => Err(DeliveryRejection::InvalidEnvelope),
    };
    if let Err(category) = authenticated {
        let _ = store.record_delivery_rejection(category);
    }
    match sent {
        Ok(Ok(())) => Ok(()),
        _ => Err(PairingEventError::Transport),
    }
}

fn authenticate(
    receiver_id: &str,
    delivery: Delivery,
    arrival: Arrival,
    store: &RegistrationStore,
    updates: &ProgressUpdates,
    notify: impl FnOnce(Sms, String),
) -> Result<(), DeliveryRejection> {
    let mut sender_name = String::new();
    let mut inbox = updates
        .inbox
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
    drop(inbox);
    let sms = result.map_err(DeliveryRejection::Receive)?;
    notify(sms, sender_name);
    Ok(())
}

fn correlation(value: Option<String>) -> Result<String, PairingEventError> {
    value
        .filter(|id| !id.trim().is_empty())
        .ok_or(PairingEventError::Protocol)
}
