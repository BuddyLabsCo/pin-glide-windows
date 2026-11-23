use std::{
    pin::Pin,
    task::{Context, Poll},
};

use futures_util::{Sink, SinkExt, Stream, StreamExt};
use tokio::io::{AsyncRead, AsyncWrite, DuplexStream, ReadBuf};
use tokio::time::{Instant, timeout};
use tokio_tungstenite::{
    WebSocketStream,
    tungstenite::{Error as SocketError, Message, protocol::Role},
};

use super::*;

#[tokio::test]
async fn attempts_the_receipt_before_handing_off_authentication() {
    let (updates, _) = crate::service::pairing_operation::tests::progress_channel();
    for fail in [false, true] {
        let (send, mut deliveries) = mpsc::channel::<QueuedDelivery>(1);
        let (attempted, receipt_attempt) = tokio::sync::oneshot::channel();
        let socket = ReceiptSocket {
            attempted: Some(attempted),
            fail,
            read: false,
        };
        let mut transport = Transport::new(socket, &updates);
        let (_power, mut state) = watch::channel(Connectivity::default());
        let mut order = 0;
        let verify = async {
            let mut receipt_attempt = receipt_attempt;
            let delivery = deliveries.recv().await.unwrap();
            assert_eq!(receipt_attempt.try_recv().unwrap(), "message");
            assert!(delivery.incoming.text.contains("\"messageId\":\"message\""));
        };
        timeout(WRITE_LIMIT, async {
            if fail {
                let (loss, ()) = tokio::join!(transport.run(&mut state, &mut order, &send), verify);
                assert_eq!(loss.status, DeliveryStatus::Disconnected);
            } else {
                tokio::select! {
                    loss = transport.run(&mut state, &mut order, &send) => panic!("connection ended: {loss:?}"),
                    () = verify => {}
                }
            }
        }).await.unwrap();
    }
}

struct ReceiptSocket {
    attempted: Option<tokio::sync::oneshot::Sender<String>>,
    fail: bool,
    read: bool,
}

impl Stream for ReceiptSocket {
    type Item = Result<Message, SocketError>;

    fn poll_next(mut self: Pin<&mut Self>, _: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        if std::mem::replace(&mut self.read, true) {
            return Poll::Pending;
        }
        Poll::Ready(Some(Ok(delivery_frame("message"))))
    }
}

impl Sink<Message> for ReceiptSocket {
    type Error = SocketError;

    fn poll_ready(self: Pin<&mut Self>, _: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        Poll::Ready(Ok(()))
    }

    fn start_send(mut self: Pin<&mut Self>, frame: Message) -> Result<(), Self::Error> {
        let receipt: Receipt = serde_json::from_str(frame.to_text().unwrap()).unwrap();
        assert_eq!(receipt.reserved_type, "receipt");
        assert_eq!(receipt.sender_id, "sender");
        self.attempted
            .take()
            .unwrap()
            .send(receipt.message_id)
            .unwrap();
        Ok(())
    }

    fn poll_flush(self: Pin<&mut Self>, _: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        Poll::Ready(if self.fail {
            Err(SocketError::ConnectionClosed)
        } else {
            Ok(())
        })
    }

    fn poll_close(self: Pin<&mut Self>, _: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        Poll::Ready(Ok(()))
    }
}

#[tokio::test(start_paused = true)]
async fn echoes_the_ping_on_the_wire_during_a_delayed_receipt() {
    let (updates, _) = crate::service::pairing_operation::tests::progress_channel();
    let (client, peer) = tokio::io::duplex(4096);
    let (blocked, receipt_blocked) = tokio::sync::oneshot::channel();
    let client = DelayedWrite {
        stream: client,
        blocked: Some(blocked),
        delay: Box::pin(tokio::time::sleep(Duration::from_secs(4))),
    };
    let socket = WebSocketStream::from_raw_socket(client, Role::Client, None).await;
    let mut peer = WebSocketStream::from_raw_socket(peer, Role::Server, None).await;
    let mut transport = Transport::new(socket, &updates);
    let (_power, mut state) = watch::channel(Connectivity::default());
    let (send, _deliveries) = mpsc::channel(1);
    let mut order = 0;
    let payload = vec![17, 0, 42, 255];
    tokio::select! {
        loss = transport.run(&mut state, &mut order, &send) => panic!("connection ended: {loss:?}"),
        () = async {
            peer.send(delivery_frame("delayed")).await.unwrap();
            receipt_blocked.await.unwrap();
            let ping_sent = Instant::now();
            peer.send(Message::Ping(payload.clone().into())).await.unwrap();
            timeout(WRITE_LIMIT, async {
                assert_receipt(peer.next().await.unwrap().unwrap(), "delayed");
                assert_eq!(peer.next().await.unwrap().unwrap(), Message::Pong(payload.into()));
            }).await.unwrap();
            assert_eq!(ping_sent.elapsed(), Duration::from_secs(4));
        } => {}
    }
}

#[tokio::test(start_paused = true)]
async fn stalled_authentication_defers_receipts_without_blocking_pong() {
    let (updates, _) = crate::service::pairing_operation::tests::progress_channel();
    let (client, peer) = tokio::io::duplex(16 * 1024);
    let socket = WebSocketStream::from_raw_socket(client, Role::Client, None).await;
    let mut peer = WebSocketStream::from_raw_socket(peer, Role::Server, None).await;
    let mut transport = Transport::new(socket, &updates);
    let (_power, mut state) = watch::channel(Connectivity::default());
    let (send, mut deliveries) = mpsc::channel(MAX_PENDING_DELIVERIES);
    let mut order = 0;
    let burst = MAX_PENDING_DELIVERIES + 8;
    let payload = vec![20, 27];
    tokio::select! {
        loss = transport.run(&mut state, &mut order, &send) => panic!("connection ended: {loss:?}"),
        () = async {
            for sequence in 0..burst {
                peer.send(delivery_frame(&sequence.to_string())).await.unwrap();
            }
            for sequence in 0..MAX_PENDING_DELIVERIES {
                assert_receipt(peer.next().await.unwrap().unwrap(), &sequence.to_string());
            }
            peer.send(Message::Ping(payload.clone().into())).await.unwrap();
            assert_eq!(
                timeout(WRITE_LIMIT, peer.next()).await.unwrap().unwrap().unwrap(),
                Message::Pong(payload.into()),
            );
            for sequence in 0..burst {
                let delivery = deliveries.recv().await.unwrap();
                let header: EventHeader = serde_json::from_str(&delivery.incoming.text).unwrap();
                assert_eq!(header.message_id.unwrap(), sequence.to_string());
                drop(delivery);
                if sequence + MAX_PENDING_DELIVERIES < burst {
                    let next = sequence + MAX_PENDING_DELIVERIES;
                    assert_receipt(peer.next().await.unwrap().unwrap(), &next.to_string());
                }
            }
        } => {}
    }
}

fn delivery_frame(id: &str) -> Message {
    Message::text(
        serde_json::json!({
            "type": "delivery", "version": 1, "senderId": "sender", "messageId": id,
        })
        .to_string(),
    )
}

fn assert_receipt(frame: Message, id: &str) {
    let receipt: Receipt = serde_json::from_str(frame.to_text().unwrap()).unwrap();
    assert_eq!(receipt.reserved_type, "receipt");
    assert_eq!(receipt.message_id, id);
}

struct DelayedWrite {
    stream: DuplexStream,
    blocked: Option<tokio::sync::oneshot::Sender<()>>,
    delay: Pin<Box<tokio::time::Sleep>>,
}

impl AsyncRead for DelayedWrite {
    fn poll_read(
        mut self: Pin<&mut Self>,
        context: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<std::io::Result<()>> {
        Pin::new(&mut self.stream).poll_read(context, buf)
    }
}

impl AsyncWrite for DelayedWrite {
    fn poll_write(
        mut self: Pin<&mut Self>,
        context: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<std::io::Result<usize>> {
        if self.delay.as_mut().poll(context).is_pending() {
            if let Some(blocked) = self.blocked.take() {
                blocked.send(()).unwrap();
            }
            return Poll::Pending;
        }
        Pin::new(&mut self.stream).poll_write(context, buf)
    }

    fn poll_flush(
        mut self: Pin<&mut Self>,
        context: &mut Context<'_>,
    ) -> Poll<std::io::Result<()>> {
        Pin::new(&mut self.stream).poll_flush(context)
    }

    fn poll_shutdown(
        mut self: Pin<&mut Self>,
        context: &mut Context<'_>,
    ) -> Poll<std::io::Result<()>> {
        Pin::new(&mut self.stream).poll_shutdown(context)
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
    let connection = DeliveryConnection::open(&service, TOKEN).await.unwrap();
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
    let authenticate = start_processing(&store, &updates, "fixture".into());
    let run = async {
        let mut order = 0;
        connection
            .run(&updates, &mut state, &mut order, &authenticate)
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
