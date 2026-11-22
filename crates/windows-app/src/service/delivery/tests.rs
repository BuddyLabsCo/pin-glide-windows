use std::{
    pin::Pin,
    task::{Context, Poll},
};

use futures_util::{Sink, Stream};
use tokio::time::{Instant, timeout};
use tokio_tungstenite::tungstenite::{Error as SocketError, Message};

use super::*;

#[tokio::test(start_paused = true)]
async fn reads_and_flushes_ping_while_a_receipt_is_backpressured() {
    let dir = tempfile::tempdir().unwrap();
    let store = RegistrationStore::new(dir.path().join("registration.json"));
    let (updates, _) = crate::service::pairing_operation::tests::progress_channel();
    let (pong, flushed) = tokio::sync::oneshot::channel();
    let (send, mut deliveries) = mpsc::channel(1);
    let socket = BackpressuredSocket {
        step: 0,
        pong: Some(pong),
        ping: Box::pin(tokio::time::sleep(Duration::from_secs(4))),
    };
    let mut transport = Transport::new(socket, &store, &updates);
    let (_power, mut state) = watch::channel(Connectivity::default());
    let mut order = 0;
    let started = Instant::now();
    tokio::select! {
        loss = transport.run(&mut state, &mut order, send) => panic!("connection ended: {loss:?}"),
        () = async {
            let delivery = deliveries.recv().await.unwrap();
            assert!(delivery.text.contains("\"messageId\":\"message\""));
            timeout(WRITE_LIMIT, flushed).await.unwrap().unwrap();
            assert_eq!(started.elapsed(), Duration::from_secs(4));
        } => {}
    }
}

struct BackpressuredSocket {
    step: u8,
    pong: Option<tokio::sync::oneshot::Sender<()>>,
    ping: Pin<Box<tokio::time::Sleep>>,
}

impl Stream for BackpressuredSocket {
    type Item = Result<Message, SocketError>;

    fn poll_next(mut self: Pin<&mut Self>, context: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        match self.step {
            0 => {
                self.step = 1;
                Poll::Ready(Some(Ok(Message::text(
                    r#"{"type":"delivery","version":1,"senderId":"sender","messageId":"message"}"#,
                ))))
            }
            2 => {
                if self.ping.as_mut().poll(context).is_pending() {
                    return Poll::Pending;
                }
                self.step = 3;
                Poll::Ready(Some(Ok(Message::Ping(vec![17].into()))))
            }
            _ => Poll::Pending,
        }
    }
}

impl Sink<Message> for BackpressuredSocket {
    type Error = SocketError;

    fn poll_ready(self: Pin<&mut Self>, _: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        Poll::Ready(Ok(()))
    }

    fn start_send(mut self: Pin<&mut Self>, frame: Message) -> Result<(), Self::Error> {
        let receipt: Receipt = serde_json::from_str(frame.to_text().unwrap()).unwrap();
        assert_eq!(receipt.reserved_type, "receipt");
        assert_eq!(receipt.sender_id, "sender");
        assert_eq!(receipt.message_id, "message");
        self.step = 2;
        Ok(())
    }

    fn poll_flush(mut self: Pin<&mut Self>, _: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        if self.step == 2 {
            return Poll::Pending;
        }
        if let Some(pong) = self.pong.take() {
            pong.send(()).unwrap();
        }
        Poll::Ready(Ok(()))
    }

    fn poll_close(self: Pin<&mut Self>, _: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        Poll::Ready(Ok(()))
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
    let path = dir.path().join("registration.json");
    std::fs::write(
        &path,
        r#"{"version":4,"active_pairings":{"fixture":{"sender_id":"sender","protected_key":[]}}}"#,
    )
    .unwrap();
    let store = RegistrationStore::new(path);
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
            loss = run => panic!("socket ended before the resume check: {loss:?}"),
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
