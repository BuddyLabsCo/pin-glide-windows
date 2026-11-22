#![cfg(windows)]

#[path = "support/dpapi.rs"]
mod dpapi;
#[path = "support/pairing.rs"]
mod fixture;

use std::{fs, io::BufReader, path::PathBuf, sync::mpsc, time::Duration};

use serde_json::{Value, json};
use tempfile::tempdir;
use tokio_tungstenite::tungstenite::protocol::{CloseFrame, frame::coding::CloseCode};
use windows_app::service::{
    PairingOperation, PairingOutcome, PairingProgress, PairingStart, SessionState,
};

use fixture::*;

#[derive(Clone, Copy)]
enum Scenario {
    Ready,
    WriteRejected,
    ClosedBeforeCancellation,
    ClosedWithoutTerminalState,
    UnexpectedCloseDuringReport,
    MissingHttpSenderConfirmation,
    MissingWssSenderConfirmation,
    ResumeUncertain,
    RejectRedemption,
    RejectUpgrade,
}

impl Scenario {
    fn definitive_rejection(self) -> bool {
        matches!(self, Self::RejectRedemption | Self::RejectUpgrade)
    }
}

#[test]
fn completes_only_after_the_exported_key_is_committed() {
    exercise(
        Scenario::Ready,
        PairingOutcome::KeyConfirmed(SessionState::Completed),
    );
    exercise(Scenario::WriteRejected, PairingOutcome::Failed);
}

#[test]
fn resumes_the_uncertain_redemption_once_after_restart() {
    exercise(
        Scenario::ResumeUncertain,
        PairingOutcome::KeyConfirmed(SessionState::Completed),
    );
}

#[test]
fn rejects_definitive_failures_during_enrollment() {
    for scenario in [Scenario::RejectRedemption, Scenario::RejectUpgrade] {
        exercise(scenario, PairingOutcome::Failed);
    }
}

#[test]
fn post_claim_cancellation_overrides_an_earlier_http_410() {
    exercise(Scenario::ClosedBeforeCancellation, PairingOutcome::Canceled);
}

#[test]
fn expires_at_the_session_deadline_without_a_terminal_state() {
    exercise(
        Scenario::ClosedWithoutTerminalState,
        PairingOutcome::Expired,
    );
}

#[test]
fn fails_when_wss_closes_without_a_terminal_state_during_completion() {
    exercise(
        Scenario::UnexpectedCloseDuringReport,
        PairingOutcome::Failed,
    );
}

#[test]
fn an_activated_snapshot_requires_both_confirmations_on_each_transport() {
    exercise(
        Scenario::MissingHttpSenderConfirmation,
        PairingOutcome::Failed,
    );
    exercise(
        Scenario::MissingWssSenderConfirmation,
        PairingOutcome::Failed,
    );
}

fn exercise(scenario: Scenario, expected: PairingOutcome) {
    let directory = tempdir().unwrap();
    let path = directory.path().join("registration.json");
    let active = json!({
        "version": 4,
        "receiver_id": "previous-receiver",
        "protected_receiver_token": [66, 77],
        "active_pairings": {
            "previous-pairing": {"sender_id": "previous-sender", "protected_key": [55, 66], "name": "Previous"}
        }
    });
    if matches!(scenario, Scenario::WriteRejected) {
        fs::write(&path, br#"{"version":99}"#).unwrap();
    } else if scenario.definitive_rejection() {
        fs::write(&path, serde_json::to_vec(&active).unwrap()).unwrap();
    }
    let (send_qr, qr) = mpsc::channel();
    let (attempt_sent, first_attempt) = mpsc::channel();
    let (finished, terminal) = mpsc::channel();
    let (service, server) =
        confirmation_service(qr, path.clone(), scenario, attempt_sent, terminal);
    let start = || {
        let service = service.clone();
        PairingOperation::start_with_registration(
            move || Some(service),
            || {},
            path.clone(),
            PairingStart::ResumePending,
        )
    };
    let mut operation = start();
    let until = std::time::Instant::now() + Duration::from_secs(10);
    let mut send_qr = Some(send_qr);
    let actual = loop {
        match operation.current() {
            PairingProgress::Waiting { code, .. } if send_qr.is_some() => {
                send_qr.take().unwrap().send(decode_qr(&code)).unwrap();
            }
            PairingProgress::Finished(outcome) => break outcome,
            PairingProgress::Completed { pairing_id } => break completed_pairing(&pairing_id),
            _ => {}
        }
        if first_attempt.try_recv().is_ok() {
            operation.shutdown();
            operation = start();
        }
        assert!(std::time::Instant::now() < until, "pairing did not finish");
        std::thread::sleep(Duration::from_millis(10));
    };
    let _ = finished.send(());
    assert_eq!(actual, expected);
    if matches!(scenario, Scenario::WriteRejected) {
        assert_eq!(fs::read(&path).unwrap(), br#"{"version":99}"#);
    } else if scenario.definitive_rejection() {
        assert_rejection_preserves_registration(&path, &active);
    }
    server.join().unwrap();
    operation.shutdown();
}

fn completed_pairing(pairing_id: &str) -> PairingOutcome {
    assert_eq!(pairing_id, "pairing-1");
    PairingOutcome::KeyConfirmed(SessionState::Completed)
}

fn assert_rejection_preserves_registration(path: &std::path::Path, active: &Value) {
    let stored = read_registration(path);
    assert!(stored["enrollment"].is_null());
    assert!(stored["provisional_pairing"].is_null());
    for field in ["receiver_id", "protected_receiver_token", "active_pairings"] {
        assert_eq!(stored[field], active[field]);
    }
}

fn confirmation_service(
    qr: mpsc::Receiver<Value>,
    path: PathBuf,
    scenario: Scenario,
    attempt_sent: mpsc::Sender<()>,
    terminal: mpsc::Receiver<()>,
) -> (windows_app::service::Service, std::thread::JoinHandle<()>) {
    let (listener, service) = listen();
    let server = std::thread::spawn(move || {
        let acceptor = acceptor();
        let lifetime = if matches!(scenario, Scenario::ClosedWithoutTerminalState) {
            4
        } else {
            30
        };
        let waiting = snapshot("session-1", "awaiting_authorization", lifetime);
        accept_creation(accept(&acceptor, &listener), &waiting);
        let mut socket = accept_events(accept(&acceptor, &listener), "session-1");
        send_state(&mut socket, &waiting);
        let qr = qr.recv_timeout(Duration::from_secs(5)).unwrap();
        let claimed = claim(waiting);
        send_state(&mut socket, claimed.clone());
        let key = sender_handshake(&mut socket, &qr);
        let connection = accept(&acceptor, &listener);
        if matches!(scenario, Scenario::WriteRejected) {
            assert_delete(connection, "session-1");
            return;
        }
        let complete = accept_completion(connection, &path, &key);
        finish_scenario(scenario, &acceptor, &listener, complete, socket, claimed);
        if matches!(
            scenario,
            Scenario::Ready
                | Scenario::ResumeUncertain
                | Scenario::RejectRedemption
                | Scenario::RejectUpgrade
        ) {
            exercise_enrollment(&acceptor, &listener, &path, scenario, attempt_sent);
            if scenario.definitive_rejection() {
                assert_no_further_requests(&listener, terminal);
            }
        }
    });
    (service, server)
}

fn assert_no_further_requests(listener: &std::net::TcpListener, terminal: mpsc::Receiver<()>) {
    let deadline = std::time::Instant::now() + Duration::from_secs(5);
    loop {
        let finished = terminal.try_recv().is_ok();
        match listener.accept() {
            Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {}
            _ => panic!("unexpected request after a definitive enrollment rejection"),
        }
        if finished {
            return;
        }
        assert!(
            std::time::Instant::now() < deadline,
            "client did not finish enrollment"
        );
        std::thread::sleep(Duration::from_millis(10));
    }
}

fn accept_completion(connection: Connection, path: &std::path::Path, key: &[u8; 32]) -> Connection {
    let mut complete = BufReader::new(connection);
    let (request, headers, body) = read_request_parts(&mut complete);
    assert_bearer(&headers);
    assert_eq!(
        request,
        "POST /v1/pairing-sessions/session-1/complete HTTP/1.1\r\n"
    );
    assert_eq!(
        serde_json::from_slice::<Value>(&body).unwrap(),
        json!({"keyConfirmation":"confirmed"})
    );
    let stored = read_registration(path);
    let pending = &stored["provisional_pairing"];
    assert_eq!(pending["session_id"], "session-1");
    assert_eq!(pending["pairing_id"], "pairing-1");
    assert_eq!(pending["sender_id"], "sender-1");
    assert_eq!(pending["receiver_id"], "receiver-1");
    assert!(dpapi::unprotect(&pending["protected_key"]) == key);
    complete.into_inner()
}

fn finish_scenario(
    scenario: Scenario,
    acceptor: &native_tls::TlsAcceptor,
    listener: &std::net::TcpListener,
    mut complete: Connection,
    mut socket: Socket,
    mut claimed: Value,
) {
    claimed["receiverConfirmed"] = json!(true);
    match scenario {
        Scenario::Ready
        | Scenario::ResumeUncertain
        | Scenario::RejectRedemption
        | Scenario::RejectUpgrade => {
            credential_ready(&mut socket, &mut complete, claimed);
        }
        Scenario::ClosedBeforeCancellation | Scenario::ClosedWithoutTerminalState => {
            write_response(
                &mut complete,
                "410 Gone",
                &json!({
                    "type":"/problems/pairing_session_closed", "title":"Session closed",
                    "status":410, "detail":"The session is closed.",
                    "code":"pairing_session_closed", "requestId":"fixture-close"
                }),
            );
            drop(complete);
            if matches!(scenario, Scenario::ClosedBeforeCancellation) {
                std::thread::sleep(Duration::from_millis(250));
                cancel(&mut socket, claimed);
            } else {
                assert_peer_left(&mut socket);
            }
        }
        Scenario::UnexpectedCloseDuringReport => {
            socket
                .close(Some(CloseFrame {
                    code: CloseCode::Policy,
                    reason: "pairing_session_closed".into(),
                }))
                .unwrap();
            assert_delete(accept(acceptor, listener), "session-1");
        }
        Scenario::MissingHttpSenderConfirmation => {
            claimed["state"] = json!("credential_ready");
            write_response(&mut complete, "200 OK", &claimed);
            assert_delete(accept(acceptor, listener), "session-1");
        }
        Scenario::MissingWssSenderConfirmation => {
            write_response(&mut complete, "200 OK", &claimed);
            claimed["state"] = json!("credential_ready");
            send_state(&mut socket, claimed);
            assert_delete(accept(acceptor, listener), "session-1");
        }
        _ => unreachable!(),
    }
}

fn read_registration(path: &std::path::Path) -> Value {
    serde_json::from_slice(&fs::read(path).unwrap()).unwrap()
}

fn accept_redemption(
    acceptor: &native_tls::TlsAcceptor,
    listener: &std::net::TcpListener,
    path: &std::path::Path,
    attempt: i32,
) -> (Connection, Value) {
    let mut redemption = BufReader::new(accept(acceptor, listener));
    let (request, headers, body) = read_request_parts(&mut redemption);
    assert_eq!(
        request,
        "POST /v1/pairing-sessions/session-1/receiver-token HTTP/1.1\r\n"
    );
    assert_bearer(&headers);
    let body: Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(body["attempt"], attempt);
    assert!(!body["operationId"].as_str().unwrap().is_empty());
    let stored = read_registration(path);
    let enrollment = &stored["enrollment"];
    assert_eq!(enrollment["operation_id"], body["operationId"]);
    assert_eq!(
        dpapi::unprotect(&enrollment["phase"][format!("Attempt{attempt}")]),
        TOKEN.as_bytes()
    );
    (redemption.into_inner(), body["operationId"].clone())
}

fn exercise_enrollment(
    acceptor: &native_tls::TlsAcceptor,
    listener: &std::net::TcpListener,
    path: &std::path::Path,
    scenario: Scenario,
    attempt_sent: mpsc::Sender<()>,
) {
    let (mut redemption, operation) = accept_redemption(acceptor, listener, path, 0);
    if matches!(scenario, Scenario::ResumeUncertain) {
        attempt_sent.send(()).unwrap();
        let (retry, same_operation) = accept_redemption(acceptor, listener, path, 1);
        assert_eq!(same_operation, operation);
        redemption = retry;
    }
    if matches!(scenario, Scenario::RejectRedemption) {
        use std::io::Write;
        redemption
            .write_all(b"HTTP/1.1 409 Conflict\r\nContent-Length: 50\r\nConnection: close\r\n\r\n")
            .unwrap();
        redemption.flush().unwrap();
        return;
    }
    write_response(
        &mut redemption,
        "200 OK",
        &json!({
            "receiverId":"receiver-1", "receiverToken": RECEIVER_TOKEN,
        }),
    );
    let connection = accept(acceptor, listener);
    let stored = read_registration(path);
    if matches!(scenario, Scenario::RejectUpgrade) {
        assert_eq!(stored["receiver_id"], "previous-receiver");
    } else {
        assert!(stored["receiver_id"].is_null());
    }
    assert_eq!(
        dpapi::unprotect(&stored["enrollment"]["phase"]["Issued"]),
        RECEIVER_TOKEN.as_bytes()
    );
    if matches!(scenario, Scenario::RejectUpgrade) {
        let mut connection = BufReader::new(connection);
        assert_eq!(
            read_request_parts(&mut connection).0,
            "GET /v1/receiver/events HTTP/1.1\r\n"
        );
        write_response(
            connection.get_mut(),
            "401 Unauthorized",
            &json!({"code":"credential_invalid"}),
        );
        return;
    }
    let mut delivery = accept_delivery(connection);
    use tokio_tungstenite::tungstenite::Message;
    delivery.send(Message::Ping(vec![13].into())).unwrap();
    assert_eq!(delivery.read().unwrap(), Message::Pong(vec![13].into()));
    let stored = read_registration(path);
    assert!(stored["enrollment"].is_null());
    assert!(stored["provisional_pairing"].is_null());
    assert_eq!(stored["receiver_id"], "receiver-1");
    assert_eq!(
        dpapi::unprotect(&stored["protected_receiver_token"]),
        RECEIVER_TOKEN.as_bytes()
    );
    assert_eq!(
        stored["active_pairings"]["pairing-1"]["sender_id"],
        "sender-1"
    );
    assert_eq!(stored["active_pairings"]["pairing-1"]["name"], "未命名设备");
}

fn decode_qr(code: &qrcode::QrCode) -> Value {
    let width = (code.width() + 8) * 4;
    let mut pixels = vec![255; width * width];
    for row in 0..code.width() {
        for column in 0..code.width() {
            if code[(column, row)] == qrcode::Color::Dark {
                for offset in 0..4 {
                    let left = ((row + 4) * 4 + offset) * width + (column + 4) * 4;
                    pixels[left..left + 4].fill(0);
                }
            }
        }
    }
    decode_pixels(width, width, &pixels)
}
