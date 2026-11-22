use futures_util::{SinkExt, StreamExt};
use tokio_tungstenite::tungstenite::Message;

use super::{
    Service,
    pairing_events::{PairingEventError, Socket},
};

#[cfg(windows)]
use crate::storage::RegistrationStore;

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

pub(super) struct DeliveryConnection(Socket);

impl DeliveryConnection {
    pub(super) async fn open(service: &Service, token: &str) -> Result<Self, PairingEventError> {
        service
            .websocket("/v1/receiver/events", token)
            .await
            .map(Self)
    }

    pub(super) async fn run(mut self) {
        while let Some(Ok(frame)) = self.0.next().await {
            if matches!(frame, Message::Ping(_) | Message::Close(_))
                && (self.0.flush().await.is_err() || frame.is_close())
            {
                break;
            }
        }
    }
}
