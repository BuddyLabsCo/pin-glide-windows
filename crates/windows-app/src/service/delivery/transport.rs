use std::{collections::VecDeque, future::poll_fn, pin::Pin, task::Poll, time::SystemTime};

use futures_util::{Sink, SinkExt, Stream};
use receiver_core::delivery::Arrival;
use tokio::{
    sync::{mpsc, watch},
    time::{Instant, timeout, timeout_at},
};
use tokio_tungstenite::tungstenite::{
    Error as SocketError, Message,
    error::ProtocolError,
    protocol::{CloseFrame, frame::coding::CloseCode},
};

use super::{
    ConnectionLoss, DeliveryStatus, HEARTBEAT_LIMIT, IncomingDelivery, MAX_PENDING_DELIVERIES,
    ProgressUpdates, RegistrationStore, WRITE_LIMIT, receipt,
};
use crate::service::reconnect::{Connectivity, connected_status};

pub(super) struct Transport<'a, Socket> {
    socket: Socket,
    store: &'a RegistrationStore,
    updates: &'a ProgressUpdates,
    outbound: VecDeque<(Message, Option<IncomingDelivery>)>,
    flushing: bool,
    write_deadline: Option<Instant>,
    heartbeat: Instant,
    probe: Option<([u8; 8], Instant)>,
}

enum SocketEvent {
    Incoming(Option<Result<Message, SocketError>>),
    Outgoing(Result<(), SocketError>, Option<IncomingDelivery>),
}

impl<'a, Socket> Transport<'a, Socket>
where
    Socket:
        Sink<Message, Error = SocketError> + Stream<Item = Result<Message, SocketError>> + Unpin,
{
    pub(super) fn new(
        socket: Socket,
        store: &'a RegistrationStore,
        updates: &'a ProgressUpdates,
    ) -> Self {
        Self {
            socket,
            store,
            updates,
            outbound: VecDeque::new(),
            flushing: false,
            write_deadline: None,
            heartbeat: Instant::now() + HEARTBEAT_LIMIT,
            probe: None,
        }
    }

    pub(super) async fn run(
        &mut self,
        state: &mut watch::Receiver<Connectivity>,
        order: &mut u64,
        authenticate: &mpsc::Sender<IncomingDelivery>,
    ) -> ConnectionLoss {
        let status = self.drive(state, order, authenticate).await;
        let resume_failed = self.probe.is_some();
        if status == DeliveryStatus::Protocol && !resume_failed {
            let _ = timeout(
                WRITE_LIMIT,
                self.socket.send(Message::Close(Some(CloseFrame {
                    code: CloseCode::Protocol,
                    reason: "invalid_delivery_event".into(),
                }))),
            )
            .await;
        }
        ConnectionLoss {
            status,
            resume_failed,
        }
    }

    async fn drive(
        &mut self,
        state: &mut watch::Receiver<Connectivity>,
        order: &mut u64,
        authenticate: &mpsc::Sender<IncomingDelivery>,
    ) -> DeliveryStatus {
        let mut resumes = state.borrow().resumes;
        let mut capacity = None;
        loop {
            let power = *state.borrow_and_update();
            if power.suspended {
                self.updates.delivery_status(DeliveryStatus::Suspended);
                let _ = state.wait_for(|power| !power.suspended).await;
                continue;
            }
            if resumes != power.resumes {
                resumes = power.resumes;
                self.start_probe(resumes);
            }
            let deadline = [
                self.write_deadline,
                self.probe.map(|(_, deadline)| deadline),
            ]
            .into_iter()
            .flatten()
            .fold(self.heartbeat, Instant::min);
            if Instant::now() >= deadline {
                return DeliveryStatus::Heartbeat;
            }
            let event = tokio::select! {
                biased;
                _ = state.wait_for(|power| power.suspended || power.resumes != resumes) => continue,
                permit = authenticate.reserve(), if capacity.is_none() && self.receipt_pending() => {
                    match permit {
                        Ok(permit) => capacity = Some(permit),
                        Err(_) => return DeliveryStatus::Disconnected,
                    }
                    continue;
                }
                event = timeout_at(deadline, self.next_event(capacity.is_some())) => event,
            };
            let result = match event {
                Ok(SocketEvent::Incoming(frame)) => self.handle_frame(frame, order).await,
                Ok(SocketEvent::Outgoing(result, incoming)) => {
                    if let Some(incoming) = incoming {
                        capacity
                            .take()
                            .expect("Receipt requires capacity")
                            .send(incoming);
                    }
                    result.map_err(|_| DeliveryStatus::Disconnected)
                }
                Err(_) => Err(DeliveryStatus::Heartbeat),
            };
            if let Err(status) = result {
                return status;
            }
        }
    }

    fn receipt_pending(&self) -> bool {
        !self.flushing && matches!(self.outbound.front(), Some((_, Some(_))))
    }

    fn start_probe(&mut self, resumes: u64) {
        let now = Instant::now();
        let payload = resumes.to_le_bytes();
        self.heartbeat = now + HEARTBEAT_LIMIT;
        self.write_deadline = self.write_deadline.map(|_| now + WRITE_LIMIT);
        self.probe = Some((payload, now + WRITE_LIMIT));
        self.outbound.retain(|(_, incoming)| incoming.is_some());
        self.outbound
            .push_front((Message::Ping(payload.to_vec().into()), None));
        self.updates.delivery_status(DeliveryStatus::Connecting);
    }

    async fn handle_frame(
        &mut self,
        frame: Option<Result<Message, SocketError>>,
        order: &mut u64,
    ) -> Result<(), DeliveryStatus> {
        match frame {
            Some(Ok(Message::Text(text))) => {
                let arrival = Arrival {
                    time: SystemTime::now(),
                    order: *order,
                };
                *order += 1;
                if let Some(reply) = receipt(&text).map_err(|_| DeliveryStatus::Protocol)? {
                    if self.outbound.len() >= MAX_PENDING_DELIVERIES {
                        return Err(DeliveryStatus::Disconnected);
                    }
                    let incoming = IncomingDelivery {
                        text,
                        arrival,
                        #[cfg(windows)]
                        epoch: crate::platform::popup_epoch(),
                    };
                    self.outbound
                        .push_back((Message::text(reply), Some(incoming)));
                }
            }
            Some(Ok(Message::Ping(_))) => {
                self.heartbeat = Instant::now() + HEARTBEAT_LIMIT;
                self.flushing = true;
                self.set_write_deadline();
            }
            Some(Ok(Message::Pong(payload))) => {
                if self
                    .probe
                    .is_some_and(|(expected, _)| payload.as_ref() == expected)
                {
                    self.probe = None;
                    self.updates.delivery_status(connected_status(self.store));
                }
            }
            Some(Ok(Message::Close(_))) => {
                if self.probe.is_none() {
                    let _ = timeout(WRITE_LIMIT, self.socket.flush()).await;
                }
                return Err(DeliveryStatus::Disconnected);
            }
            Some(Err(SocketError::Protocol(ProtocolError::ResetWithoutClosingHandshake))) => {
                return Err(DeliveryStatus::Disconnected);
            }
            Some(
                Ok(_)
                | Err(SocketError::Protocol(_) | SocketError::Utf8(_) | SocketError::Capacity(_)),
            ) => {
                return Err(DeliveryStatus::Protocol);
            }
            _ => return Err(DeliveryStatus::Disconnected),
        }
        Ok(())
    }

    fn set_write_deadline(&mut self) {
        self.write_deadline
            .get_or_insert_with(|| Instant::now() + WRITE_LIMIT);
    }

    async fn next_event(&mut self, has_capacity: bool) -> SocketEvent {
        poll_fn(|context| {
            let can_write = self
                .outbound
                .front()
                .is_some_and(|(_, incoming)| incoming.is_none() || has_capacity);
            let mut socket = Pin::new(&mut self.socket);
            if self.flushing {
                match socket.as_mut().poll_flush(context) {
                    Poll::Ready(Ok(())) => {
                        self.flushing = false;
                        self.write_deadline = None;
                        return Poll::Ready(SocketEvent::Outgoing(Ok(()), None));
                    }
                    Poll::Ready(Err(error)) => {
                        return Poll::Ready(SocketEvent::Outgoing(Err(error), None));
                    }
                    Poll::Pending => {}
                }
            }
            if !self.flushing && can_write {
                if self.write_deadline.is_none() {
                    self.set_write_deadline();
                    return Poll::Ready(SocketEvent::Outgoing(Ok(()), None));
                }
                let Poll::Ready(ready) = socket.as_mut().poll_ready(context) else {
                    return socket.poll_next(context).map(SocketEvent::Incoming);
                };
                let (message, incoming) = self.outbound.pop_front().unwrap();
                let sent = ready
                    .and_then(|()| socket.as_mut().start_send(message))
                    .and_then(|()| match socket.as_mut().poll_flush(context) {
                        Poll::Ready(result) => result,
                        Poll::Pending => {
                            self.flushing = true;
                            Ok(())
                        }
                    });
                if !self.flushing {
                    self.write_deadline = None;
                }
                return Poll::Ready(SocketEvent::Outgoing(sent, incoming));
            }
            socket.poll_next(context).map(SocketEvent::Incoming)
        })
        .await
    }
}
