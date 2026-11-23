use std::{io::BufReader, net::TcpListener, sync::mpsc::Receiver, time::Duration};

use serde_json::{Value, json};
use tokio_tungstenite::tungstenite::Message;

use super::fixture::*;

pub(super) fn serve(listener: TcpListener, release: Receiver<()>, qr: Receiver<Value>) {
    let tls = acceptor();
    let mut delivery = accept_delivery(accept(&tls, &listener));
    let mut rejected = BufReader::new(accept(&tls, &listener));
    assert_eq!(
        read_request_parts(&mut rejected).0,
        "POST /v1/pairing-sessions HTTP/1.1\r\n"
    );
    release.recv_timeout(Duration::from_secs(8)).unwrap();
    write_response(rejected.get_mut(), "503 Service Unavailable", &json!({}));
    for (name, seconds) in [("expired", 2), ("canceled", 30), ("complete", 272)] {
        let mut initial = snapshot(name, "awaiting_authorization", seconds);
        initial["mode"] = json!("add_sender");
        accept_creation(accept(&tls, &listener), &initial);
        let mut events = accept_events(accept(&tls, &listener), name);
        send_state(&mut events, &initial);
        if name == "expired" {
            assert_peer_left(&mut events);
            continue;
        }
        release.recv_timeout(Duration::from_secs(8)).unwrap();
        let mut claimed = claim(initial);
        if name == "canceled" {
            cancel(&mut events, claimed);
            continue;
        }
        send_state(&mut events, &claimed);
        sender_handshake(
            &mut events,
            &qr.recv_timeout(Duration::from_secs(8)).unwrap(),
        );
        let mut report = accept_completion(accept(&tls, &listener), name);
        claimed["receiverConfirmed"] = json!(true);
        write_response(report.get_mut(), "200 OK", &claimed);
        release.recv_timeout(Duration::from_secs(8)).unwrap();
        claimed["senderConfirmed"] = json!(true);
        claimed["state"] = json!("completed");
        send_state(&mut events, claimed);
    }
    delivery.send(Message::Ping(vec![8].into())).unwrap();
    assert!(delivery.read().unwrap().is_pong());
    assert_peer_left(&mut delivery);
}
