use std::sync::mpsc;

use tokio::sync::oneshot;

use super::*;
use crate::service::{
    pairing_operation::tests::progress_channel,
    tests::{accept_loopback_with_timeout, acceptor, listen, local_service, socket},
};

#[tokio::test]
async fn an_unanswered_resume_probe_replaces_the_socket_without_backoff() {
    const TOKEN: &str = "fixture-r";
    let (listener, origin) = listen("localhost");
    let service = local_service(&listener, &origin, true);
    let (replacement, replaced) = oneshot::channel();
    let (stop, stopped) = mpsc::channel();
    let server = std::thread::spawn(move || {
        let acceptor = acceptor();
        let accept = || {
            let stream = acceptor
                .accept(accept_loopback_with_timeout(
                    &listener,
                    Duration::from_secs(8),
                ))
                .unwrap();
            socket::accept_socket(stream, "/v1/receiver/events", TOKEN)
        };
        let mut unresponsive = accept();
        let probe = unresponsive.read().unwrap();
        assert!(probe.is_ping());
        let started = Instant::now();
        let _replacement = accept();
        replacement.send(started.elapsed()).unwrap();
        stopped.recv_timeout(Duration::from_secs(5)).unwrap();
    });
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("registration.json");
    std::fs::write(
        &path,
        r#"{"version":4,"active_pairings":{"fixture":{"sender_id":"sender","protected_key":[]}}}"#,
    )
    .unwrap();
    let store = RegistrationStore::new(path);
    let (updates, _) = progress_channel();
    let mut status = updates.delivery.subscribe();
    let (power, state) = watch::channel(Connectivity::default());
    let mut reconnect = Reconnect::new(&service, TOKEN, "fixture", &updates);
    reconnect.state = state;
    let connection = reconnect.open(false).await.unwrap();
    timeout(Duration::from_secs(8), async {
        tokio::select! {
            () = reconnect.receive(&store, connection) => panic!("reconnect owner exited"),
            () = async {
                status.wait_for(|status| *status == DeliveryStatus::Ready).await.unwrap();
                power.send_modify(|state| state.suspended = true);
                status.wait_for(|status| *status == DeliveryStatus::Suspended).await.unwrap();
                power.send_modify(|state| {
                    state.suspended = false;
                    state.resumes += 1;
                });
                let elapsed = replaced.await.unwrap();
                assert!(elapsed < Duration::from_millis(5700), "replacement took {elapsed:?}");
                status.wait_for(|status| *status == DeliveryStatus::Ready).await.unwrap();
            } => {}
        }
    })
    .await
    .unwrap();
    stop.send(()).unwrap();
    server.join().unwrap();
}

#[cfg(windows)]
#[tokio::test]
async fn a_resume_close_reconnects_before_the_authentication_backlog_drains() {
    use crate::storage::Enrollment;
    use serde_json::json;
    use tokio_tungstenite::tungstenite::Message;

    const TOKEN: &str = "fixture-r";
    let (listener, origin) = listen("localhost");
    let service = local_service(&listener, &origin, true);
    let (replacement, replaced) = oneshot::channel();
    let (receipt_sent, receipt_received) = oneshot::channel();
    let (continue_sending, may_send) = mpsc::channel();
    let (stop, stopped) = mpsc::channel();
    let server = std::thread::spawn(move || {
        let acceptor = acceptor();
        let accept = || {
            let stream = acceptor
                .accept(accept_loopback_with_timeout(
                    &listener,
                    Duration::from_secs(5),
                ))
                .unwrap();
            socket::accept_socket(stream, "/v1/receiver/events", TOKEN)
        };
        let mut first = accept();
        send_sms(&mut first, 1, "first");
        receipt_sent.send(()).unwrap();
        assert!(first.read().unwrap().is_ping());
        first.send(Message::Close(None)).unwrap();
        let mut second = accept();
        replacement.send(()).unwrap();
        may_send.recv_timeout(Duration::from_secs(5)).unwrap();
        send_sms(&mut second, 2, "second");
        stopped.recv_timeout(Duration::from_secs(5)).unwrap();
    });
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("registration.json");
    std::fs::write(&path, json!({
        "version": 4,
        "receiver_id": "fixture",
        "active_pairings": {
            "pairing": {
                "sender_id": "sender",
                "name": "Fixture",
                "protected_key": Enrollment::protect_token(std::str::from_utf8(&[b'K'; 32]).unwrap()).unwrap(),
            }
        }
    }).to_string()).unwrap();
    let store = RegistrationStore::new(path);
    let (updates, _) = progress_channel();
    let inbox = updates.inbox.clone();
    let (blocked, held) = oneshot::channel();
    let (release, released) = mpsc::channel();
    let block_authentication = std::thread::spawn(move || {
        let guard = inbox.lock().unwrap();
        blocked.send(()).unwrap();
        let _ = released.recv();
        drop(guard);
    });
    held.await.unwrap();
    let mut status = updates.delivery.subscribe();
    let (power, state) = watch::channel(Connectivity::default());
    let mut reconnect = Reconnect::new(&service, TOKEN, "fixture", &updates);
    reconnect.state = state;
    let connection = reconnect.open(false).await.unwrap();
    timeout(Duration::from_secs(4), async {
        tokio::select! {
            () = reconnect.receive(&store, connection) => panic!("reconnect owner exited"),
            () = async {
                receipt_received.await.unwrap();
                status.wait_for(|status| *status == DeliveryStatus::Ready).await.unwrap();
                power.send_modify(|state| state.suspended = true);
                status.wait_for(|status| *status == DeliveryStatus::Suspended).await.unwrap();
                power.send_modify(|state| {
                    state.suspended = false;
                    state.resumes += 1;
                });
                replaced.await.unwrap();
                release.send(()).unwrap();
                for expected in ["first", "second"] {
                    while updates.inbox.lock().unwrap().latest() != Some(expected) {
                        tokio::time::sleep(Duration::from_millis(5)).await;
                    }
                    if expected == "first" {
                        continue_sending.send(()).unwrap();
                    }
                }
            } => {}
        }
    })
    .await
    .unwrap();
    block_authentication.join().unwrap();
    stop.send(()).unwrap();
    server.join().unwrap();
}

#[cfg(windows)]
fn send_sms(
    socket: &mut tokio_tungstenite::tungstenite::WebSocket<
        native_tls::TlsStream<std::net::TcpStream>,
    >,
    sequence: u8,
    otp: &str,
) {
    use aes_gcm::{Aes256Gcm, KeyInit, Nonce, aead::Aead};
    use base64::{Engine, engine::general_purpose::STANDARD};
    use serde_json::{Value, json};
    use tokio_tungstenite::tungstenite::Message;

    let nonce = [sequence; 12];
    let cipher = Aes256Gcm::new_from_slice(&[b'K'; 32]).unwrap();
    let sms = json!({
        "type": "verification_message", "smsText": "Fixture SMS",
        "otp": otp, "receivedAt": "2026-11-23T00:00:00Z",
    });
    let ciphertext = cipher
        .encrypt(&Nonce::from(nonce), sms.to_string().as_bytes())
        .unwrap();
    let message_id = sequence.to_string();
    socket.send(Message::text(json!({
        "type": "delivery", "version": 1, "cryptoVersion": 1, "nonce": STANDARD.encode(nonce),
        "ciphertext": STANDARD.encode(ciphertext), "messageId": message_id,
        "receiverId": "fixture", "pairingId": "pairing", "senderId": "sender",
    }).to_string())).unwrap();
    let frame = socket.read().unwrap();
    let receipt: Value = serde_json::from_str(frame.to_text().unwrap()).unwrap();
    assert_eq!(
        receipt,
        json!({
            "type": "receipt", "version": 1, "senderId": "sender", "messageId": message_id,
        })
    );
}
