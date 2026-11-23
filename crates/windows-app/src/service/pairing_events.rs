use futures_util::{SinkExt, StreamExt};
use pin_glide_wss::{
    AnonymousSchema26, AnonymousSchema33, CredentialReady, PairingRelayIn, PairingRelayOut,
    PairingState, WssError,
};
use reqwest::{Response, StatusCode, Version, header};
use serde_json::Value;
use tokio_tungstenite::{
    WebSocketStream,
    tungstenite::{
        Message,
        handshake::{client::generate_key, derive_accept_key},
        protocol::{CloseFrame, Role, WebSocketConfig, frame::coding::CloseCode},
    },
};

use super::{
    NewReceiverSession, Service, SessionState, pairing_session_view, pairing_snapshot::Snapshot,
};

const PROTOCOL: &str = "pin-glide.v1";
pub(super) type Socket = WebSocketStream<reqwest::Upgraded>;

pub(super) struct PairingEvents {
    socket: Socket,
    session_id: String,
    receiver_id: String,
    mode: pairing_session_view::Mode,
}

pub(super) enum PairingEvent {
    State(Snapshot),
    Relay(String),
    CredentialReady,
    Error,
}

#[derive(Debug, thiserror::Error)]
pub(super) enum PairingEventError {
    #[error("pairing event transport failed")]
    Transport,
    #[error("invalid pairing event protocol")]
    Protocol,
    #[error("Service rejected pairing subscription with HTTP {status}")]
    Rejected {
        status: StatusCode,
        code: Option<String>,
    },
}

impl Service {
    pub(super) async fn pairing_events(
        &self,
        session: &NewReceiverSession,
    ) -> Result<PairingEvents, PairingEventError> {
        let session_id = pin_glide_http::apis::urlencode(&session.session_id);
        let socket = self
            .websocket(
                &format!("/v1/pairing-sessions/{session_id}/events"),
                &session.pairing_token,
            )
            .await?;
        Ok(PairingEvents {
            socket,
            session_id: session.session_id.clone(),
            receiver_id: session.receiver_id.clone(),
            mode: session.mode,
        })
    }

    pub(super) async fn websocket(
        &self,
        route: &str,
        token: &str,
    ) -> Result<Socket, PairingEventError> {
        let key = generate_key();
        let response = self
            .config
            .client
            .get(format!("{}{route}", self.config.base_path))
            .version(Version::HTTP_11)
            .bearer_auth(token)
            .header(header::CONNECTION, "Upgrade")
            .header(header::UPGRADE, "websocket")
            .header(header::SEC_WEBSOCKET_VERSION, "13")
            .header(header::SEC_WEBSOCKET_KEY, &key)
            .header(header::SEC_WEBSOCKET_PROTOCOL, PROTOCOL)
            .send()
            .await
            .map_err(|_| PairingEventError::Transport)?;
        if response.status() != StatusCode::SWITCHING_PROTOCOLS {
            return Err(subscription_error(response).await);
        }
        verify_upgrade(&response, &key)?;
        let stream = response
            .upgrade()
            .await
            .map_err(|_| PairingEventError::Transport)?;
        let config = WebSocketConfig::default()
            .max_message_size(Some(64 * 1024))
            .max_frame_size(Some(64 * 1024));
        Ok(WebSocketStream::from_raw_socket(stream, Role::Client, Some(config)).await)
    }
}

fn verify_upgrade(response: &Response, key: &str) -> Result<(), PairingEventError> {
    let headers = response.headers();
    let has_token = |name, token: &str| {
        headers
            .get_all(name)
            .iter()
            .filter_map(|value| value.to_str().ok())
            .any(|value| {
                value
                    .split(',')
                    .any(|part| part.trim().eq_ignore_ascii_case(token))
            })
    };
    let accept = derive_accept_key(key.as_bytes());
    if !has_token(header::CONNECTION, "upgrade")
        || !has_token(header::UPGRADE, "websocket")
        || !headers
            .get(header::SEC_WEBSOCKET_ACCEPT)
            .is_some_and(|value| value == accept.as_str())
        || !headers
            .get(header::SEC_WEBSOCKET_PROTOCOL)
            .is_some_and(|value| value == PROTOCOL)
        || headers.contains_key(header::SEC_WEBSOCKET_EXTENSIONS)
    {
        return Err(PairingEventError::Protocol);
    }
    Ok(())
}

async fn subscription_error(mut response: Response) -> PairingEventError {
    let status = response.status();
    let mut body = Vec::new();
    let code = loop {
        match response.chunk().await {
            Ok(Some(chunk)) if body.len() + chunk.len() <= 16 * 1024 => {
                body.extend_from_slice(&chunk)
            }
            Ok(None) => {
                break serde_json::from_slice::<Value>(&body)
                    .ok()
                    .and_then(|problem| problem["code"].as_str().map(str::to_owned));
            }
            _ => break None,
        }
    };
    PairingEventError::Rejected { status, code }
}

impl PairingEvents {
    pub async fn next_event(&mut self) -> Result<PairingEvent, PairingEventError> {
        while let Some(frame) = self.socket.next().await {
            match frame.map_err(|_| PairingEventError::Transport)? {
                Message::Text(text) => match self.parse(&text) {
                    Ok(event) => return Ok(event),
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

    pub async fn send_relay(&mut self, payload: String) -> Result<(), PairingEventError> {
        let frame = PairingRelayIn {
            version: 1,
            reserved_type: "pairing_relay".into(),
            session_id: self.session_id.clone(),
            payload,
        };
        let text = serde_json::to_string(&frame).map_err(|_| PairingEventError::Protocol)?;
        self.socket
            .send(Message::text(text))
            .await
            .map_err(|_| PairingEventError::Transport)
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

    fn parse(&self, text: &str) -> Result<PairingEvent, PairingEventError> {
        let event: Value = serde_json::from_str(text).map_err(|_| PairingEventError::Protocol)?;
        if event["version"].as_i64() != Some(1) {
            return Err(PairingEventError::Protocol);
        }
        match event["type"].as_str() {
            Some("pairing_state") => self.parse_state(event),
            Some("pairing_relay") => {
                let relay: PairingRelayOut =
                    serde_json::from_value(event).map_err(|_| PairingEventError::Protocol)?;
                if relay.session_id != self.session_id || relay.side != "sender" {
                    return Err(PairingEventError::Protocol);
                }
                Ok(PairingEvent::Relay(relay.payload))
            }
            Some("credential_ready") => {
                let ready: CredentialReady =
                    serde_json::from_value(event).map_err(|_| PairingEventError::Protocol)?;
                if self.mode != pairing_session_view::Mode::NewReceiver
                    || ready.session_id != self.session_id
                    || ready.receiver_id != self.receiver_id
                {
                    return Err(PairingEventError::Protocol);
                }
                Ok(PairingEvent::CredentialReady)
            }
            Some("error") => {
                let error: WssError =
                    serde_json::from_value(event).map_err(|_| PairingEventError::Protocol)?;
                if error.session_id.is_some_and(|id| id != self.session_id) {
                    return Err(PairingEventError::Protocol);
                }
                Ok(PairingEvent::Error)
            }
            _ => Err(PairingEventError::Protocol),
        }
    }

    fn parse_state(&self, event: Value) -> Result<PairingEvent, PairingEventError> {
        let state: PairingState =
            serde_json::from_value(event).map_err(|_| PairingEventError::Protocol)?;
        let session = state.session;
        if !matches!(
            (self.mode, *session.mode),
            (
                pairing_session_view::Mode::NewReceiver,
                AnonymousSchema26::NewReceiver
            ) | (
                pairing_session_view::Mode::AddSender,
                AnonymousSchema26::AddSender
            )
        ) {
            return Err(PairingEventError::Protocol);
        }
        let phase = match *session.state {
            AnonymousSchema33::AwaitingAuthorization => SessionState::AwaitingAuthorization,
            AnonymousSchema33::AwaitingConfirmation => SessionState::AwaitingConfirmation,
            AnonymousSchema33::CredentialReady => SessionState::CredentialReady,
            AnonymousSchema33::CredentialIssued => SessionState::CredentialIssued,
            AnonymousSchema33::Completed => SessionState::Completed,
            AnonymousSchema33::Canceled => SessionState::Canceled,
            AnonymousSchema33::Expired => SessionState::Expired,
            AnonymousSchema33::Superseded => SessionState::Superseded,
        };
        Snapshot {
            session_id: session.session_id,
            receiver_id: session.receiver_id,
            pairing_id: session.pairing_id,
            sender_id: session.sender_id,
            state: phase,
            receiver_confirmed: session.receiver_confirmed,
            sender_confirmed: session.sender_confirmed,
        }
        .validated(&self.session_id, &self.receiver_id)
        .map(PairingEvent::State)
        .ok_or(PairingEventError::Protocol)
    }
}
