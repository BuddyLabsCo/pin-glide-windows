use std::{
    io::{BufRead, BufReader, Read, Write},
    net::TcpListener,
    thread::{self, JoinHandle},
    time::Duration,
};

use native_tls::{Identity, TlsAcceptor};
use reqwest::StatusCode;
use serde_json::{Value, json};

use super::{CreateError, Service, SessionState, client_builder};

const TEST_CA: &[u8] = include_bytes!("../../tests/fixtures/service/ca.pem");

fn listen() -> (TcpListener, String) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let origin = format!(
        "https://localhost:{}",
        listener.local_addr().unwrap().port()
    );
    (listener, origin)
}

fn acceptor() -> TlsAcceptor {
    let identity = Identity::from_pkcs12(
        include_bytes!("../../tests/fixtures/service/server.p12"),
        "pin-glide-test",
    )
    .unwrap();
    TlsAcceptor::new(identity).unwrap()
}

fn respond(
    status: &'static str,
    media_type: &'static str,
    body: Value,
) -> (Service, JoinHandle<()>) {
    let (listener, origin) = listen();
    let mut service = Service::new(&origin).unwrap();
    service.config.client = client_builder()
        .no_proxy()
        .tls_certs_only([reqwest::Certificate::from_pem(TEST_CA).unwrap()])
        .build()
        .unwrap();
    let server = thread::spawn(move || {
        let (stream, _) = listener.accept().unwrap();
        stream
            .set_read_timeout(Some(Duration::from_secs(5)))
            .unwrap();
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
    let (listener, origin) = listen();
    let service = Service::new(&origin).unwrap();
    let server = thread::spawn(move || {
        let (stream, _) = listener.accept().unwrap();
        stream
            .set_read_timeout(Some(Duration::from_secs(5)))
            .unwrap();
        if let Ok(mut stream) = acceptor().accept(stream) {
            assert!(!matches!(stream.read(&mut [0]), Ok(1)));
        }
    });
    assert!(
        matches!(service.new_receiver().await, Err(CreateError::Transport(error)) if error.is_connect())
    );
    server.join().unwrap();
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
