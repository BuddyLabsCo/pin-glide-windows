use futures_util::{SinkExt, StreamExt};
use pin_glide_wss::{AnonymousSchema26, AnonymousSchema33, PairingState};
use serde_json::Value;
use tokio_tungstenite::{
    MaybeTlsStream, WebSocketStream, connect_async_with_config,
    tungstenite::{
        Message,
        client::IntoClientRequest,
        http::{HeaderValue, header},
        protocol::{CloseFrame, WebSocketConfig, frame::coding::CloseCode},
    },
};

use super::{NewReceiverSession, Service, SessionState};

const PROTOCOL: &str = "pin-glide.v1";

pub struct PairingEvents {
    socket: WebSocketStream<MaybeTlsStream<tokio::net::TcpStream>>,
    session_id: String,
    receiver_id: String,
}

#[derive(Debug, thiserror::Error)]
pub enum PairingEventError {
    #[error("pairing event transport failed")]
    Transport,
    #[error("invalid pairing event protocol")]
    Protocol,
}

impl Service {
    pub async fn pairing_events(
        &self,
        session: &NewReceiverSession,
    ) -> Result<PairingEvents, PairingEventError> {
        let path = self.config.base_path.replacen("https://", "wss://", 1);
        let session_id = pin_glide_http::apis::urlencode(&session.session_id);
        let mut request = format!("{path}/v1/pairing-sessions/{session_id}/events")
            .into_client_request()
            .map_err(|_| PairingEventError::Protocol)?;
        let mut authorization = HeaderValue::from_str(&format!("Bearer {}", session.pairing_token))
            .map_err(|_| PairingEventError::Protocol)?;
        authorization.set_sensitive(true);
        request
            .headers_mut()
            .insert(header::AUTHORIZATION, authorization);
        request.headers_mut().insert(
            header::SEC_WEBSOCKET_PROTOCOL,
            HeaderValue::from_static(PROTOCOL),
        );
        let config = WebSocketConfig::default()
            .max_message_size(Some(64 * 1024))
            .max_frame_size(Some(64 * 1024));
        let (socket, _) = connect_async_with_config(request, Some(config), false)
            .await
            .map_err(|_| PairingEventError::Transport)?;
        Ok(PairingEvents {
            socket,
            session_id: session.session_id.clone(),
            receiver_id: session.receiver_id.clone(),
        })
    }
}

impl PairingEvents {
    pub async fn next_state(&mut self) -> Result<SessionState, PairingEventError> {
        while let Some(frame) = self.socket.next().await {
            match frame.map_err(|_| PairingEventError::Transport)? {
                Message::Text(text) => match self.parse(&text) {
                    Ok(Some(state)) => return Ok(state),
                    Ok(None) => {}
                    Err(error) => {
                        self.protocol_close().await;
                        return Err(error);
                    }
                },
                Message::Ping(_) | Message::Pong(_) => {
                    self.socket
                        .flush()
                        .await
                        .map_err(|_| PairingEventError::Transport)?;
                }
                Message::Close(_) => {
                    let _ = self.socket.flush().await;
                    break;
                }
                _ => {
                    self.protocol_close().await;
                    return Err(PairingEventError::Protocol);
                }
            }
        }
        Err(PairingEventError::Transport)
    }

    async fn protocol_close(&mut self) {
        let _ = self
            .socket
            .close(Some(CloseFrame {
                code: CloseCode::Protocol,
                reason: "invalid_pairing_event".into(),
            }))
            .await;
    }

    fn parse(&self, text: &str) -> Result<Option<SessionState>, PairingEventError> {
        let event: Value = serde_json::from_str(text).map_err(|_| PairingEventError::Protocol)?;
        if event["version"].as_i64() != Some(1) {
            return Err(PairingEventError::Protocol);
        }
        match event["type"].as_str() {
            Some("pairing_state") => {
                let state: PairingState =
                    serde_json::from_value(event).map_err(|_| PairingEventError::Protocol)?;
                if state.session.session_id != self.session_id
                    || state.session.receiver_id != self.receiver_id
                    || *state.session.mode != AnonymousSchema26::NewReceiver
                {
                    return Err(PairingEventError::Protocol);
                }
                Ok(Some(match *state.session.state {
                    AnonymousSchema33::AwaitingAuthorization => SessionState::AwaitingAuthorization,
                    AnonymousSchema33::AwaitingConfirmation => SessionState::AwaitingConfirmation,
                    AnonymousSchema33::CredentialReady => SessionState::CredentialReady,
                    AnonymousSchema33::CredentialIssued => SessionState::CredentialIssued,
                    AnonymousSchema33::Completed => SessionState::Completed,
                    AnonymousSchema33::Canceled => SessionState::Canceled,
                    AnonymousSchema33::Expired => SessionState::Expired,
                    AnonymousSchema33::Superseded => SessionState::Superseded,
                }))
            }
            Some("pairing_relay" | "credential_ready")
                if event["sessionId"].as_str() == Some(self.session_id.as_str()) =>
            {
                Ok(None)
            }
            Some("error")
                if event.get("sessionId").is_none()
                    || event["sessionId"].as_str() == Some(self.session_id.as_str()) =>
            {
                Ok(None)
            }
            _ => Err(PairingEventError::Protocol),
        }
    }
}
