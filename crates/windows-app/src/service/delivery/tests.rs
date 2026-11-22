use std::{cell::Cell, future::ready};

use super::*;

#[tokio::test]
async fn attempts_the_receipt_before_authentication_even_when_sending_fails() {
    let dir = tempfile::tempdir().unwrap();
    let store = RegistrationStore::new(dir.path().join("registration.json"));
    let delivery = r#"{"type":"delivery","version":1,"cryptoVersion":1,"senderId":"sender",
        "messageId":"message","receiverId":"receiver","pairingId":"pairing","nonce":"","ciphertext":""}"#;
    for fails in [false, true] {
        let attempted = Cell::new(false);
        let authenticated = Cell::new(false);
        let mut socket = futures_util::sink::drain()
            .sink_map_err(|never| -> SocketError { match never {} })
            .with(|frame: Message| {
                let receipt: Receipt = serde_json::from_str(frame.to_text().unwrap()).unwrap();
                assert_eq!(receipt.reserved_type, "receipt");
                assert_eq!(receipt.sender_id, "sender");
                assert_eq!(receipt.message_id, "message");
                attempted.set(true);
                ready(if fails {
                    Err(SocketError::ConnectionClosed)
                } else {
                    Ok(frame)
                })
            });
        let result = handle_text(&mut socket, delivery, "receiver", &store, |_| {
            assert!(
                attempted.get(),
                "authenticate only after attempting the Receipt"
            );
            authenticated.set(true);
            Ok(())
        })
        .await;
        assert!(authenticated.get());
        assert_eq!(result.is_err(), fails);
    }
}

#[tokio::test]
async fn checks_the_existing_socket_after_resume() {
    use std::sync::mpsc;
    use tokio::sync::oneshot;

    use crate::service::{
        pairing_operation::tests::progress_channel,
        tests::{accept_loopback, acceptor, listen, local_service, socket},
    };

    const TOKEN: &str = "fixture-r";
    let (listener, origin) = listen("localhost");
    let service = local_service(&listener, &origin, true);
    let (acknowledged, pong) = oneshot::channel();
    let (stop, stopped) = mpsc::channel();
    let server = std::thread::spawn(move || {
        let stream = acceptor().accept(accept_loopback(&listener)).unwrap();
        let mut socket = socket::accept_socket(stream, "/v1/receiver/events", TOKEN);
        assert!(matches!(socket.read().unwrap(), Message::Ping(_)));
        socket.flush().unwrap();
        acknowledged.send(()).unwrap();
        stopped.recv_timeout(Duration::from_secs(5)).unwrap();
    });
    let connection = DeliveryConnection::open(&service, TOKEN, "fixture")
        .await
        .unwrap();
    let dir = tempfile::tempdir().unwrap();
    let store = RegistrationStore::new(dir.path().join("registration.json"));
    let (power, mut state) = watch::channel(Connectivity::default());
    let (updates, _progress) = progress_channel();
    let mut status = updates.delivery.subscribe();
    let run = async {
        let mut order = 0;
        connection
            .run(&store, &updates, &mut state, &mut order)
            .await
    };
    timeout(Duration::from_secs(5), async {
        tokio::select! {
            reason = run => panic!("socket ended before the resume check: {reason:?}"),
            () = async {
                power.send_modify(|state| state.suspended = true);
                status.wait_for(|state| *state == DeliveryStatus::Suspended).await.unwrap();
                power.send_modify(|state| {
                    state.suspended = false;
                    state.resumes += 1;
                });
                pong.await.unwrap();
                status.wait_for(|state| *state == DeliveryStatus::Ready).await.unwrap();
            } => {}
        }
    })
    .await
    .unwrap();
    stop.send(()).unwrap();
    server.join().unwrap();
}
