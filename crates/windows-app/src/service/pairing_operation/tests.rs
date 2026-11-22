use std::{
    io::{BufRead, BufReader, Read, Write},
    sync::{
        atomic::{AtomicU64, Ordering},
        mpsc,
    },
    time::{Duration, Instant as WallInstant, SystemTime, UNIX_EPOCH},
};

use serde_json::{Value, json};
use tokio_tungstenite::tungstenite::{
    Message, accept_hdr,
    handshake::server::{Request, Response},
    protocol::{CloseFrame, frame::coding::CloseCode},
};

use super::{PairingOperation, PairingOutcome, PairingProgress, Service, drive, oneshot, watch};
use crate::service::tests::{accept_loopback, acceptor, listen, local_service};

const TOKEN: &str = "fixture-pairing-b";
const EXPIRY: &str = "2030-01-01T00:01:00Z";
const START: u64 = 1_893_456_000;

enum Script {
    Events(mpsc::Receiver<Option<&'static str>>),
    Rejected(&'static str, &'static str),
}

fn script_service(script: Script) -> (Service, std::thread::JoinHandle<()>) {
    let (listener, origin) = listen("localhost");
    let service = local_service(&listener, &origin, true);
    let server = std::thread::spawn(move || {
        let acceptor = acceptor();
        let mut creation = BufReader::new(acceptor.accept(accept_loopback(&listener)).unwrap());
        assert_eq!(
            read_request(&mut creation),
            "POST /v1/pairing-sessions HTTP/1.1\r\n"
        );
        let created = json!({
            "session": snapshot("awaiting_authorization"),
            "pairingToken": TOKEN,
            "pairingTokenExpiresAt": EXPIRY,
        });
        write_response(creation.get_mut(), "201 Created", &created);
        drop(creation);

        let connection = acceptor.accept(accept_loopback(&listener)).unwrap();
        match script {
            Script::Events(frames) => send_events(connection, &frames),
            Script::Rejected(status, code) => {
                let mut request = BufReader::new(connection);
                assert_eq!(
                    read_request(&mut request),
                    "GET /v1/pairing-sessions/session-1/events HTTP/1.1\r\n"
                );
                let body = json!({"code": code, "detail": "test rejection"});
                write_response(request.get_mut(), status, &body);
            }
        }
    });
    (service, server)
}

fn read_request(stream: &mut impl BufRead) -> String {
    let mut request = String::new();
    stream.read_line(&mut request).unwrap();
    let mut size = 0;
    loop {
        let mut header = String::new();
        assert_ne!(stream.read_line(&mut header).unwrap(), 0);
        if header == "\r\n" {
            break;
        }
        let (name, value) = header.split_once(':').unwrap();
        if name.eq_ignore_ascii_case("content-length") {
            size = value.trim().parse().unwrap();
        }
    }
    stream.read_exact(&mut vec![0; size]).unwrap();
    request
}

fn write_response(stream: &mut impl Write, status: &str, body: &Value) {
    let body = body.to_string();
    write!(
        stream,
        "HTTP/1.1 {status}\r\nContent-Type: application/problem+json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        body.len(),
    )
    .unwrap();
    stream.flush().unwrap();
    std::thread::sleep(Duration::from_millis(20));
    stream.write_all(body.as_bytes()).unwrap();
}

fn snapshot(state: &str) -> Value {
    json!({
        "sessionId": "session-1",
        "receiverId": "receiver-1",
        "mode": "new_receiver",
        "state": state,
        "expiresAt": EXPIRY,
        "senderConfirmed": false,
        "receiverConfirmed": false,
    })
}

fn send_events(connection: impl Read + Write, frames: &mpsc::Receiver<Option<&str>>) {
    #[expect(
        clippy::result_large_err,
        reason = "tungstenite fixes the callback error type"
    )]
    let mut socket = accept_hdr(connection, |request: &Request, mut response: Response| {
        assert_eq!(
            request.uri().path(),
            "/v1/pairing-sessions/session-1/events"
        );
        assert_eq!(
            request.headers()["authorization"],
            format!("Bearer {TOKEN}")
        );
        assert_eq!(request.headers()["sec-websocket-protocol"], "pin-glide.v1");
        response
            .headers_mut()
            .insert("sec-websocket-protocol", "pin-glide.v1".parse().unwrap());
        Ok(response)
    })
    .unwrap();
    for state in frames {
        let Some(state) = state else {
            let _ = socket.close(Some(CloseFrame {
                code: CloseCode::Policy,
                reason: "fixture close".into(),
            }));
            return;
        };
        socket
            .send(Message::text(
                json!({
                    "type": "pairing_state",
                    "version": 1,
                    "session": snapshot(state),
                })
                .to_string(),
            ))
            .unwrap();
    }
}

async fn wait_for(operation: &PairingOperation, matches: impl Fn(&PairingProgress) -> bool) {
    let until = WallInstant::now() + Duration::from_secs(5);
    while !matches(&operation.current()) {
        assert!(WallInstant::now() < until, "expected operation progress");
        tokio::task::yield_now().await;
    }
}

fn clock(seconds: u64) -> SystemTime {
    UNIX_EPOCH + Duration::from_secs(seconds)
}

#[tokio::test]
async fn reopening_reads_the_terminal_snapshot_even_when_followed_by_policy_close() {
    for (state, expected) in [
        ("canceled", PairingOutcome::Canceled),
        ("expired", PairingOutcome::Expired),
        ("awaiting_authorization", PairingOutcome::Failed),
    ] {
        let (send, frames) = mpsc::channel();
        let (service, server) = script_service(Script::Events(frames));
        let (updates, progress) = watch::channel(PairingProgress::Preparing);
        let (_stop, stopping) = oneshot::channel();
        let operation = PairingOperation {
            progress,
            worker: None,
        };
        let worker =
            tokio::spawn(async move { drive(&service, &updates, stopping, || clock(START)).await });
        send.send(Some("awaiting_authorization")).unwrap();
        wait_for(&operation, |state| {
            matches!(state, PairingProgress::Waiting { .. })
        })
        .await;
        let PairingProgress::Waiting { code: first, .. } = operation.current() else {
            unreachable!()
        };
        let PairingProgress::Waiting { code: reopened, .. } = operation.current() else {
            unreachable!()
        };
        assert!(std::sync::Arc::ptr_eq(&first, &reopened));
        send.send(Some(state)).unwrap();
        send.send(None).unwrap();
        wait_for(&operation, |state| {
            matches!(state, PairingProgress::Finished(_))
        })
        .await;
        assert!(
            matches!(operation.current(), PairingProgress::Finished(outcome) if outcome == expected)
        );
        worker.await.unwrap();
        server.join().unwrap();
    }
}

#[tokio::test]
async fn upgrade_rejection_distinguishes_expiry_from_other_failures() {
    for (status, code, expected) in [
        (
            "410 Gone",
            "pairing_session_expired",
            PairingOutcome::Expired,
        ),
        (
            "401 Unauthorized",
            "credential_expired",
            PairingOutcome::Expired,
        ),
        (
            "410 Gone",
            "pairing_session_closed",
            PairingOutcome::CreateFailed,
        ),
        (
            "401 Unauthorized",
            "credential_invalid",
            PairingOutcome::CreateFailed,
        ),
    ] {
        let (service, server) = script_service(Script::Rejected(status, code));
        let (updates, progress) = watch::channel(PairingProgress::Preparing);
        let (_stop, stopping) = oneshot::channel();
        drive(&service, &updates, stopping, || clock(START)).await;
        assert!(
            matches!(*progress.borrow(), PairingProgress::Finished(outcome) if outcome == expected)
        );
        server.join().unwrap();
    }
}

#[tokio::test]
async fn a_wall_clock_jump_cannot_expire_the_display_ahead_of_the_operation() {
    let (send, frames) = mpsc::channel();
    let (service, server) = script_service(Script::Events(frames));
    let (updates, progress) = watch::channel(PairingProgress::Preparing);
    let (_stop, stopping) = oneshot::channel();
    let operation = PairingOperation {
        progress,
        worker: None,
    };
    let wall = std::sync::Arc::new(AtomicU64::new(START));
    let worker_wall = wall.clone();
    let worker = tokio::spawn(async move {
        drive(&service, &updates, stopping, || {
            clock(worker_wall.load(Ordering::Relaxed))
        })
        .await;
    });
    send.send(Some("awaiting_authorization")).unwrap();
    wait_for(&operation, |state| {
        matches!(state, PairingProgress::Waiting { .. })
    })
    .await;
    tokio::time::pause();
    wall.store(START + 3600, Ordering::Relaxed);
    assert!(matches!(
        operation.current(),
        PairingProgress::Waiting { seconds: 60, .. }
    ));
    tokio::time::advance(Duration::from_secs(59)).await;
    wait_for(&operation, |state| {
        matches!(state, PairingProgress::Waiting { seconds: 1, .. })
    })
    .await;
    tokio::time::advance(Duration::from_secs(1)).await;
    worker.await.unwrap();
    assert!(matches!(
        operation.current(),
        PairingProgress::Finished(PairingOutcome::Expired)
    ));
    send.send(None).unwrap();
    server.join().unwrap();
}
