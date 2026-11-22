use std::time::Duration;

use pin_glide_http::{
    apis::{
        Error,
        configuration::Configuration,
        pairing_api::{
            PostPairingSessionCompleteSuccess, PostPairingSessionsError,
            PostPairingSessionsSuccess, delete_pairing_session, post_pairing_session_complete,
            post_pairing_sessions,
        },
    },
    models::{
        CompleteRequest, CreateRequest, complete_request, create_request, pairing_session_view,
    },
};
use reqwest::{Certificate, Client, ClientBuilder, StatusCode, Url};
use time::{OffsetDateTime, format_description::well_known::Rfc3339};

mod delivery;
mod enrollment;
mod pairing_events;
pub(crate) mod pairing_operation;
mod pairing_snapshot;
mod reconnect;
#[cfg(windows)]
pub(crate) use delivery::ReceivedSms;
pub use pairing_operation::{PairingOperation, PairingProgress, PairingStart};
pub use receiver_core::pairing::{PairingOutcome, SessionState};
pub use reconnect::DeliveryStatus;
#[cfg(windows)]
pub(crate) use reconnect::{connectivity_improved, power_changed};

#[cfg(test)]
pub(crate) mod tests;

#[derive(Clone)]
pub struct Service {
    config: Configuration,
}

pub struct NewReceiverSession {
    pub session_id: String,
    pub receiver_id: String,
    pub pairing_id: Option<String>,
    pub sender_id: Option<String>,
    pub state: SessionState,
    pub receiver_confirmed: bool,
    pub sender_confirmed: bool,
    pub expires_at: OffsetDateTime,
    pub pairing_token: String,
    pub pairing_token_expires_at: OffsetDateTime,
}

#[derive(Debug, thiserror::Error)]
pub enum CreateError {
    #[error("expected an HTTPS Service origin")]
    InvalidOrigin,
    #[error("Service transport failed")]
    Transport(#[from] reqwest::Error),
    #[error("invalid response to new Receiver creation")]
    InvalidResponse,
    #[error("Service returned HTTP {status}")]
    Http { status: StatusCode },
    #[error("Service returned HTTP {status} ({code}, request {request_id})")]
    Business {
        status: StatusCode,
        code: String,
        request_id: String,
        detail: String,
        errors: Option<Option<Vec<ValidationError>>>,
    },
}

#[derive(Debug, Eq, PartialEq)]
pub struct ValidationError {
    pub location: Option<String>,
    pub message: String,
}

impl Service {
    pub fn new(origin: &str) -> Result<Self, CreateError> {
        Self::with_builder(origin, client_builder())
    }

    pub fn with_trusted_root(origin: &str, root: Certificate) -> Result<Self, CreateError> {
        Self::with_builder(origin, client_builder().tls_certs_only([root]))
    }

    fn with_builder(origin: &str, builder: ClientBuilder) -> Result<Self, CreateError> {
        let origin = Url::parse(origin).map_err(|_| CreateError::InvalidOrigin)?;
        if origin.scheme() != "https"
            || !origin.username().is_empty()
            || origin.password().is_some()
            || origin.path() != "/"
            || origin.query().is_some()
            || origin.fragment().is_some()
        {
            return Err(CreateError::InvalidOrigin);
        }

        Ok(Self {
            config: Configuration {
                base_path: origin.origin().ascii_serialization(),
                client: builder.build()?,
                user_agent: None,
                basic_auth: None,
                oauth_access_token: None,
                bearer_access_token: None,
                api_key: None,
            },
        })
    }

    pub async fn new_receiver(&self) -> Result<NewReceiverSession, CreateError> {
        let response = post_pairing_sessions(
            &self.config,
            CreateRequest::new(create_request::Mode::NewReceiver),
        )
        .await
        .map_err(create_error)?;
        if response.status != StatusCode::CREATED {
            return Err(CreateError::Http {
                status: response.status,
            });
        }
        let Some(PostPairingSessionsSuccess::Status201(created)) = response.entity else {
            return Err(CreateError::InvalidResponse);
        };
        let session = created.session;
        if session.mode != pairing_session_view::Mode::NewReceiver
            || session.session_id.is_empty()
            || session.receiver_id.is_empty()
            || created.pairing_token.is_empty()
        {
            return Err(CreateError::InvalidResponse);
        }

        Ok(NewReceiverSession {
            session_id: session.session_id,
            receiver_id: session.receiver_id,
            pairing_id: session.pairing_id,
            sender_id: session.sender_id,
            state: session_state(session.state),
            receiver_confirmed: session.receiver_confirmed,
            sender_confirmed: session.sender_confirmed,
            expires_at: OffsetDateTime::parse(&session.expires_at, &Rfc3339)
                .map_err(|_| CreateError::InvalidResponse)?,
            pairing_token: created.pairing_token,
            pairing_token_expires_at: OffsetDateTime::parse(
                &created.pairing_token_expires_at,
                &Rfc3339,
            )
            .map_err(|_| CreateError::InvalidResponse)?,
        })
    }

    pub async fn cancel_receiver(&self, session: &NewReceiverSession) -> bool {
        let mut config = self.config.clone();
        config.bearer_access_token = Some(session.pairing_token.clone());
        matches!(
            delete_pairing_session(&config, &session.session_id).await,
            Ok(response) if response.status == StatusCode::NO_CONTENT
        )
    }

    async fn confirm_receiver(
        &self,
        session: &NewReceiverSession,
    ) -> Result<pairing_snapshot::Snapshot, CompletionError> {
        let mut config = self.config.clone();
        config.bearer_access_token = Some(session.pairing_token.clone());
        let report = CompleteRequest::new(complete_request::KeyConfirmation::Confirmed);
        let result = post_pairing_session_complete(&config, &session.session_id, report)
            .await
            .map_err(completion_error)?;
        let Some(PostPairingSessionCompleteSuccess::Status200(snapshot)) = result.entity else {
            return Err(CompletionError::InvalidResponse);
        };
        if result.status != StatusCode::OK
            || snapshot.mode != pairing_session_view::Mode::NewReceiver
        {
            return Err(CompletionError::InvalidResponse);
        }
        pairing_snapshot::Snapshot {
            session_id: snapshot.session_id,
            receiver_id: snapshot.receiver_id,
            pairing_id: snapshot.pairing_id,
            sender_id: snapshot.sender_id,
            state: session_state(snapshot.state),
            receiver_confirmed: snapshot.receiver_confirmed,
            sender_confirmed: snapshot.sender_confirmed,
        }
        .validated(&session.session_id, &session.receiver_id)
        .ok_or(CompletionError::InvalidResponse)
    }
}

enum CompletionError {
    Transport,
    InvalidResponse,
    Rejected {
        status: StatusCode,
        code: Option<String>,
    },
}

fn completion_error(
    error: Error<pin_glide_http::apis::pairing_api::PostPairingSessionCompleteError>,
) -> CompletionError {
    use pin_glide_http::apis::pairing_api::PostPairingSessionCompleteError::*;

    match error {
        Error::Reqwest(_) => CompletionError::Transport,
        Error::ResponseError(response) => {
            let code = match response.entity {
                Some(
                    Status401(problem) | Status404(problem) | Status409(problem)
                    | Status410(problem) | Status422(problem) | Status500(problem)
                    | Status503(problem),
                ) if problem.status == i64::from(response.status.as_u16()) => Some(problem.code),
                _ => None,
            };
            CompletionError::Rejected {
                status: response.status,
                code,
            }
        }
        _ => CompletionError::InvalidResponse,
    }
}

fn session_state(state: pairing_session_view::State) -> SessionState {
    use pairing_session_view::State;

    match state {
        State::AwaitingAuthorization => SessionState::AwaitingAuthorization,
        State::AwaitingConfirmation => SessionState::AwaitingConfirmation,
        State::CredentialReady => SessionState::CredentialReady,
        State::CredentialIssued => SessionState::CredentialIssued,
        State::Completed => SessionState::Completed,
        State::Canceled => SessionState::Canceled,
        State::Expired => SessionState::Expired,
        State::Superseded => SessionState::Superseded,
    }
}

fn create_error(error: Error<PostPairingSessionsError>) -> CreateError {
    use PostPairingSessionsError::{Status401, Status404, Status422, Status500, Status503};

    let response = match error {
        Error::Reqwest(error) => return CreateError::Transport(error),
        Error::ResponseError(response) => response,
        _ => return CreateError::InvalidResponse,
    };
    let Some(
        Status401(problem) | Status404(problem) | Status422(problem) | Status500(problem)
        | Status503(problem),
    ) = response.entity
    else {
        return CreateError::Http {
            status: response.status,
        };
    };
    CreateError::Business {
        status: response.status,
        code: problem.code,
        request_id: problem.request_id,
        detail: problem.detail,
        errors: problem.errors.map(|errors| {
            errors.map(|items| {
                items
                    .into_iter()
                    .map(|item| ValidationError {
                        location: item.location,
                        message: item.message,
                    })
                    .collect()
            })
        }),
    }
}

fn client_builder() -> ClientBuilder {
    Client::builder()
        .tls_backend_native()
        .https_only(true)
        .redirect(reqwest::redirect::Policy::none())
        .retry(reqwest::retry::never())
        .timeout(Duration::from_secs(15))
}
