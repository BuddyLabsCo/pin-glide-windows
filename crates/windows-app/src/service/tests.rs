use std::{
    io::{BufRead, BufReader, Read, Write},
    net::TcpListener,
    thread::{self, JoinHandle},
    time::Duration,
};

use reqwest::StatusCode;
use serde_json::{Value, json};

use super::{CreateError, NewReceiverSession, Service, SessionState, client_builder};

const TEST_CA: &[u8] = include_bytes!("../../tests/fixtures/service/ca.pem");

#[path = "../../tests/support/socket.rs"]
pub(super) mod socket;

#[path = "../../tests/support/tls.rs"]
mod tls;
pub(crate) use tls::{accept_loopback, acceptor, listen};

pub(crate) fn local_service(
    listener: &TcpListener,
    origin: &str,
    trust_fixture_ca: bool,
) -> Service {
    let mut service = Service::new(origin).unwrap();
    let mut client = client_builder()
        .no_proxy()
        .resolve("localhost", listener.local_addr().unwrap());
    if trust_fixture_ca {
        client = client.tls_certs_only([reqwest::Certificate::from_pem(TEST_CA).unwrap()]);
    }
    service.config.client = client.build().unwrap();
    service
}

fn respond(
    status: &'static str,
    media_type: &'static str,
    body: Value,
) -> (Service, JoinHandle<()>) {
    let (listener, origin) = listen("localhost");
    let service = local_service(&listener, &origin, true);
    let server = thread::spawn(move || {
        let stream = accept_loopback(&listener);
        let mut stream = BufReader::new(acceptor().accept(stream).unwrap());
        let mut request_line = String::new();
        stream.read_line(&mut request_line).unwrap();
        assert_eq!(request_line, "POST /v1/pairing-sessions HTTP/1.1\r\n");
        let mut content_length = None;
        loop {
            let mut header = String::new();
            assert_ne!(stream.read_line(&mut header).unwrap(), 0);
            if header == "\r\n" {
                break;
            }
            let (name, value) = header.split_once(':').unwrap();
            assert!(!name.eq_ignore_ascii_case("authorization"));
            if name.eq_ignore_ascii_case("content-length") {
                content_length = Some(value.trim().parse::<usize>().unwrap());
            }
        }
        let mut request = vec![0; content_length.unwrap()];
        stream.read_exact(&mut request).unwrap();
        assert_eq!(
            serde_json::from_slice::<Value>(&request).unwrap(),
            json!({"mode": "new_receiver"}),
        );

        let body = body.to_string();
        write!(
            stream.get_mut(),
            "HTTP/1.1 {status}\r\nContent-Type: {media_type}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
            body.len(),
        )
        .unwrap();
    });
    (service, server)
}

#[tokio::test]
async fn creates_a_receiver_anonymously_and_decodes_the_session() {
    let (service, server) = respond(
        "201 Created",
        "application/json",
        json!({
            "session": {
                "sessionId": "session-1",
                "receiverId": "receiver-1",
                "mode": "new_receiver",
                "state": "awaiting_authorization",
                "expiresAt": "2026-01-01T00:05:00Z",
                "senderConfirmed": false,
                "receiverConfirmed": false
            },
            "pairingToken": "restricted-test-token",
            "pairingTokenExpiresAt": "2026-01-01T00:05:00Z"
        }),
    );
    let session = service.new_receiver().await.unwrap();
    server.join().unwrap();
    assert_eq!(session.session_id, "session-1");
    assert_eq!(session.receiver_id, "receiver-1");
    assert_eq!(session.state, SessionState::AwaitingAuthorization);
    assert_eq!(session.pairing_id, None);
    assert_eq!(session.sender_id, None);
    assert_eq!(session.pairing_token, "restricted-test-token");
    assert_eq!(session.expires_at.unix_timestamp(), 1_767_225_900);
    assert_eq!(session.pairing_token_expires_at, session.expires_at);
    assert!(!session.sender_confirmed && !session.receiver_confirmed);
}

#[tokio::test]
async fn cancels_the_receiver_session_using_its_restricted_token() {
    let deadline = time::OffsetDateTime::now_utc() + Duration::from_secs(300);
    let session = NewReceiverSession {
        session_id: "session-1".into(),
        receiver_id: "receiver-1".into(),
        pairing_id: None,
        sender_id: None,
        state: SessionState::AwaitingAuthorization,
        receiver_confirmed: false,
        sender_confirmed: false,
        expires_at: deadline,
        pairing_token: "restricted-test-token".into(),
        pairing_token_expires_at: deadline,
    };
    let (listener, origin) = listen("localhost");
    let service = local_service(&listener, &origin, true);
    let server = thread::spawn(move || {
        let stream = accept_loopback(&listener);
        let mut stream = BufReader::new(acceptor().accept(stream).unwrap());
        let mut request = String::new();
        stream.read_line(&mut request).unwrap();
        assert_eq!(
            request,
            "DELETE /v1/pairing-sessions/session-1 HTTP/1.1\r\n"
        );
        let mut authorization = None;
        loop {
            let mut header = String::new();
            assert_ne!(stream.read_line(&mut header).unwrap(), 0);
            if header == "\r\n" {
                break;
            }
            let (name, value) = header.split_once(':').unwrap();
            if name.eq_ignore_ascii_case("authorization") {
                authorization = Some(value.trim().to_owned());
            }
        }
        assert_eq!(
            authorization.as_deref(),
            Some("Bearer restricted-test-token")
        );
        stream
            .get_mut()
            .write_all(b"HTTP/1.1 204 No Content\r\nConnection: close\r\n\r\n")
            .unwrap();
    });
    assert!(service.cancel_receiver(&session).await);
    server.join().unwrap();
}

#[tokio::test]
async fn decodes_a_business_problem_with_its_http_status_and_validation_items() {
    let (service, server) = respond(
        "422 Unprocessable Entity",
        "application/problem+json",
        json!({
            "type": "/problems/validation_failed",
            "title": "Validation failed",
            "status": 422,
            "detail": "The request was rejected.",
            "code": "validation_failed",
            "requestId": "request-1",
            "errors": [{"location": "body.mode", "message": "Unsupported mode"}]
        }),
    );
    let outcome = service.new_receiver().await;
    server.join().unwrap();
    let Err(CreateError::Business {
        status,
        code,
        request_id,
        detail,
        errors,
    }) = outcome
    else {
        panic!("expected a Service business problem");
    };
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(code, "validation_failed");
    assert_eq!(request_id, "request-1");
    assert_eq!(detail, "The request was rejected.");
    assert_eq!(
        errors,
        Some(Some(vec![super::ValidationError {
            location: Some("body.mode".into()),
            message: "Unsupported mode".into(),
        }]))
    );
}

#[tokio::test]
async fn distinguishes_an_unstructured_failure_from_a_business_problem() {
    let (service, server) = respond(
        "503 Service Unavailable",
        "application/json",
        json!({"proxy": "unavailable"}),
    );
    let outcome = service.new_receiver().await;
    server.join().unwrap();
    assert!(matches!(
        outcome,
        Err(CreateError::Http {
            status: StatusCode::SERVICE_UNAVAILABLE
        })
    ));
}

#[tokio::test]
async fn rejects_an_untrusted_service_certificate() {
    assert_tls_rejected("localhost", false).await;
}

#[tokio::test]
async fn rejects_a_trusted_service_certificate_for_a_different_hostname() {
    assert_tls_rejected("127.0.0.1", true).await;
}

async fn assert_tls_rejected(host: &str, trust_fixture_ca: bool) {
    let (listener, origin) = listen(host);
    let service = local_service(&listener, &origin, trust_fixture_ca);
    let server = thread::spawn(move || {
        let stream = accept_loopback(&listener);
        if let Ok(mut stream) = acceptor().accept(stream) {
            assert!(
                !matches!(stream.read(&mut [0]), Ok(1)),
                "client sent HTTP over rejected TLS"
            );
        }
    });
    let outcome = service.new_receiver().await;
    server.join().unwrap();
    assert!(matches!(outcome, Err(CreateError::Transport(error)) if error.is_connect()));
}

#[test]
fn wss_pairing_state_preserves_recovery_mode_and_absent_pairing() {
    let frame = json!({
        "type": "pairing_state",
        "version": 1,
        "session": {
            "mode": "recover_receiver",
            "state": "credential_issued",
            "sessionId": "recovery-1",
            "receiverId": "receiver-1",
            "senderId": "sender-1",
            "expiresAt": "2026-01-01T00:05:00Z",
            "senderConfirmed": true,
            "receiverConfirmed": true
        }
    });
    let model: pin_glide_wss::PairingState = serde_json::from_value(frame.clone()).unwrap();
    assert_eq!(model.session.pairing_id, None);
    assert_eq!(model.session.sender_id.as_deref(), Some("sender-1"));
    assert_eq!(serde_json::to_value(model).unwrap(), frame);
}

#[test]
fn wss_business_error_round_trips_absent_null_and_array_errors() {
    for errors in [
        None,
        Some(Value::Null),
        Some(json!([{"location": "payload", "message": "Required"}])),
    ] {
        let mut frame = json!({
            "type": "error",
            "version": 1,
            "requestId": "request-1",
            "error": {"code": "invalid_request", "detail": "Invalid frame"}
        });
        if let Some(errors) = errors {
            frame["error"]["errors"] = errors;
        }
        let model: pin_glide_wss::WssError = serde_json::from_value(frame.clone()).unwrap();
        assert_eq!(serde_json::to_value(model).unwrap(), frame);
    }
}
