#![cfg(windows)]

#[path = "support/dpapi.rs"]
mod dpapi;
#[path = "support/pairing.rs"]
mod fixture;

use std::{fs, io::BufReader, path::PathBuf, sync::mpsc, time::Duration};

use serde_json::{Value, json};
use tempfile::tempdir;
use windows_app::service::{PairingOperation, PairingOutcome, PairingProgress, SessionState};

use fixture::*;

#[derive(Clone, Copy)]
enum Scenario {
    Ready,
    WriteRejected,
    ClosedBeforeCancellation,
    ClosedWithoutTerminalState,
    MissingHttpSenderConfirmation,
    MissingWssSenderConfirmation,
}

#[test]
fn completes_only_after_the_exported_key_is_committed() {
    exercise(
        Scenario::Ready,
        PairingOutcome::KeyConfirmed(SessionState::CredentialReady),
    );
    exercise(Scenario::WriteRejected, PairingOutcome::Failed);
}

#[test]
fn post_claim_cancellation_overrides_an_earlier_http_410() {
    exercise(Scenario::ClosedBeforeCancellation, PairingOutcome::Canceled);
}

#[test]
fn reports_failure_when_the_service_never_publishes_a_terminal_state() {
    exercise(Scenario::ClosedWithoutTerminalState, PairingOutcome::Failed);
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
    if matches!(scenario, Scenario::WriteRejected) {
        fs::write(&path, br#"{"version":99}"#).unwrap();
    }
    let (send_qr, qr) = mpsc::channel();
    let (service, server) = confirmation_service(qr, path.clone(), scenario);
    let operation =
        PairingOperation::start_with_registration(move || Some(service), || {}, path.clone());
    let until = std::time::Instant::now() + Duration::from_secs(10);
    let mut send_qr = Some(send_qr);
    let actual = loop {
        match operation.current() {
            PairingProgress::Waiting { code, .. } if send_qr.is_some() => {
                send_qr.take().unwrap().send(decode_qr(&code)).unwrap();
            }
            PairingProgress::Finished(outcome) => break outcome,
            _ => {}
        }
        assert!(std::time::Instant::now() < until, "pairing did not finish");
        std::thread::sleep(Duration::from_millis(10));
    };
    assert_eq!(actual, expected);
    if matches!(scenario, Scenario::WriteRejected) {
        assert_eq!(fs::read(&path).unwrap(), br#"{"version":99}"#);
    }
    operation.shutdown();
    server.join().unwrap();
}

fn confirmation_service(
    qr: mpsc::Receiver<Value>,
    path: PathBuf,
    scenario: Scenario,
) -> (windows_app::service::Service, std::thread::JoinHandle<()>) {
    let (listener, service) = listen();
    let server = std::thread::spawn(move || {
        let acceptor = acceptor();
        let waiting = snapshot("session-1", "awaiting_authorization", 30);
        accept_creation(accept(&acceptor, &listener), &waiting);
        let mut socket = accept_events(accept(&acceptor, &listener), "session-1");
        send_state(&mut socket, &waiting);
        let qr = qr.recv_timeout(Duration::from_secs(5)).unwrap();
        let claimed = claim(waiting);
        send_state(&mut socket, claimed.clone());
        let key = sender_handshake(&mut socket, &qr);
        let mut complete = BufReader::new(accept(&acceptor, &listener));
        let (request, headers, body) = read_request_parts(&mut complete);
        assert_bearer(&headers);
        if matches!(scenario, Scenario::WriteRejected) {
            assert_eq!(
                request,
                "DELETE /v1/pairing-sessions/session-1 HTTP/1.1\r\n"
            );
            no_content(complete.get_mut());
            return;
        }
        assert_eq!(
            request,
            "POST /v1/pairing-sessions/session-1/complete HTTP/1.1\r\n"
        );
        assert_eq!(
            serde_json::from_slice::<Value>(&body).unwrap(),
            json!({"keyConfirmation":"confirmed"})
        );
        let stored: Value = serde_json::from_slice(&fs::read(path).unwrap()).unwrap();
        let pending = &stored["provisional_pairing"];
        assert_eq!(pending["session_id"], "session-1");
        assert_eq!(pending["pairing_id"], "pairing-1");
        assert_eq!(pending["sender_id"], "sender-1");
        assert_eq!(pending["receiver_id"], "receiver-1");
        assert!(dpapi::unprotect(&pending["protected_key"]) == key);
        finish_scenario(
            scenario,
            &acceptor,
            &listener,
            complete.into_inner(),
            socket,
            claimed,
        );
    });
    (service, server)
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
        Scenario::Ready => {
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
                assert_delete(accept(acceptor, listener), "session-1");
            }
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
        Scenario::WriteRejected => unreachable!(),
    }
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
