use futures_util::{SinkExt, StreamExt};
use pin_glide_wss::{AnonymousSchema26, AnonymousSchema33, PairingState};
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

use super::{NewReceiverSession, Service, SessionState};

const PROTOCOL: &str = "pin-glide.v1";

pub(super) struct PairingEvents {
    socket: WebSocketStream<reqwest::Upgraded>,
    session_id: String,
    receiver_id: String,
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
        let path = &self.config.base_path;
        let session_id = pin_glide_http::apis::urlencode(&session.session_id);
        let key = generate_key();
        let response = self
            .config
            .client
            .get(format!("{path}/v1/pairing-sessions/{session_id}/events"))
            .version(Version::HTTP_11)
            .bearer_auth(&session.pairing_token)
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
        Ok(PairingEvents {
            socket: WebSocketStream::from_raw_socket(stream, Role::Client, Some(config)).await,
            session_id: session.session_id.clone(),
            receiver_id: session.receiver_id.clone(),
        })
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
