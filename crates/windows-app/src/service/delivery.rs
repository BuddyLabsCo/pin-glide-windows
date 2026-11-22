use futures_util::{SinkExt, StreamExt};
use tokio_tungstenite::tungstenite::Message;

use super::{
    Service,
    pairing_events::{PairingEventError, Socket},
};

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
