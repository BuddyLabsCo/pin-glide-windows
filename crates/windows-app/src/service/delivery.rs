use std::{sync::Mutex, time::SystemTime};

use futures_util::{SinkExt, StreamExt};
use pin_glide_wss::{Delivery, Receipt};
use receiver_core::delivery::{Arrival, Envelope, Inbox, Sms};
use serde::Deserialize;
use tokio_tungstenite::tungstenite::{
    Message,
    protocol::{CloseFrame, frame::coding::CloseCode},
};

use super::{
    Service,
    pairing_events::{PairingEventError, Socket},
};

use crate::storage::RegistrationStore;

pub(crate) struct ReceivedSms {
    #[cfg_attr(not(windows), allow(dead_code))]
    pub sms: Sms,
    #[cfg_attr(not(windows), allow(dead_code))]
    pub sender_name: String,
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
        inbox: &Mutex<Inbox>,
        notify: fn(ReceivedSms),
    ) {
        let mut order = 0;
        while let Some(Ok(frame)) = self.socket.next().await {
            let time = SystemTime::now();
            let result = match frame {
                Message::Text(text) => {
                    order += 1;
                    self.handle_text(&text, Arrival { time, order }, store, inbox, notify)
                        .await
                }
                Message::Ping(_) | Message::Pong(_) => self
                    .socket
                    .flush()
                    .await
                    .map_err(|_| PairingEventError::Transport),
                Message::Close(_) => {
                    let _ = self.socket.flush().await;
                    break;
                }
                _ => Err(PairingEventError::Protocol),
            };
            if let Err(error) = result {
                if matches!(error, PairingEventError::Protocol) {
                    let _ = self
                        .socket
                        .close(Some(CloseFrame {
                            code: CloseCode::Protocol,
                            reason: "invalid_delivery_event".into(),
                        }))
                        .await;
                }
                break;
            }
        }
    }

    async fn handle_text(
        &mut self,
        text: &str,
        arrival: Arrival,
        store: &RegistrationStore,
        inbox: &Mutex<Inbox>,
        notify: fn(ReceivedSms),
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
        let sent = self.socket.send(Message::text(receipt)).await;
        if let Ok(delivery) = serde_json::from_str::<Delivery>(text)
            && delivery.receiver_id == self.receiver_id
            && let Some(message) = self.authenticate(&delivery, arrival, store, inbox)
        {
            notify(message);
        }
        sent.map_err(|_| PairingEventError::Transport)
    }

    fn authenticate(
        &self,
        delivery: &Delivery,
        arrival: Arrival,
        store: &RegistrationStore,
        inbox: &Mutex<Inbox>,
    ) -> Option<ReceivedSms> {
        let mut sender_name = String::new();
        let sms = inbox.lock().ok()?.receive(
            arrival,
            Envelope {
                pairing_id: &delivery.pairing_id,
                crypto_version: delivery.crypto_version,
                nonce: &delivery.nonce,
                ciphertext: &delivery.ciphertext,
            },
            |pairing_id| {
                let target =
                    store.delivery_target(&self.receiver_id, pairing_id, &delivery.sender_id)?;
                sender_name = target.name;
                Some(target.key)
            },
        )?;
        Some(ReceivedSms { sms, sender_name })
    }
}

fn correlation(value: Option<String>) -> Result<String, PairingEventError> {
    value
        .filter(|id| !id.trim().is_empty())
        .ok_or(PairingEventError::Protocol)
}
