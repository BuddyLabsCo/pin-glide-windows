#![allow(dead_code)]

use std::{
    io::{BufRead, BufReader, Write},
    net::{TcpListener, TcpStream},
    time::Duration,
};

use native_tls::{TlsAcceptor, TlsStream};
use serde_json::{Value, json};
use time::{OffsetDateTime, format_description::well_known::Rfc3339};
use tokio_tungstenite::tungstenite::{
    Message, WebSocket,
    protocol::{CloseFrame, frame::coding::CloseCode},
};
use windows_app::service::Service;

mod socket;
use socket::accept_socket;

#[path = "sender.rs"]
mod sender;
pub use sender::{decode_pixels, sender_handshake};
#[path = "tls.rs"]
mod tls;
pub use tls::acceptor;

pub type Connection = TlsStream<TcpStream>;
pub type Socket = WebSocket<Connection>;
pub const TOKEN: &str = "fixture-pairing-b";
pub const RECEIVER_TOKEN: &str = "fixture-receiver-r";

pub fn decode_qr(code: &qrcode::QrCode) -> Value {
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

pub fn listen() -> (TcpListener, Service) {
    let (listener, origin) = tls::listen("localhost");
    let root =
        reqwest::Certificate::from_pem(include_bytes!("../fixtures/service/ca.pem")).unwrap();
    let service = Service::with_trusted_root(&origin, root).unwrap();
    (listener, service)
}

pub fn accept(acceptor: &TlsAcceptor, listener: &TcpListener) -> Connection {
    acceptor.accept(tls::accept_loopback(listener)).unwrap()
}

pub fn read_request_parts(stream: &mut impl BufRead) -> (String, Vec<String>, Vec<u8>) {
    let mut request = String::new();
    stream.read_line(&mut request).unwrap();
    let mut size = 0;
    let mut headers = Vec::new();
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
        headers.push(header);
    }
    let mut body = vec![0; size];
    stream.read_exact(&mut body).unwrap();
    (request, headers, body)
}

pub fn assert_bearer(headers: &[String]) {
    assert!(headers.iter().any(|header| {
        header.split_once(':').is_some_and(|(name, value)| {
            name.eq_ignore_ascii_case("authorization") && value.trim() == format!("Bearer {TOKEN}")
        })
    }));
}

pub fn accept_completion(connection: Connection, session_id: &str) -> BufReader<Connection> {
    let mut stream = BufReader::new(connection);
    let (request, headers, body) = read_request_parts(&mut stream);
    assert_eq!(
        request,
        format!("POST /v1/pairing-sessions/{session_id}/complete HTTP/1.1\r\n")
    );
    assert_bearer(&headers);
    assert_eq!(
        serde_json::from_slice::<Value>(&body).unwrap(),
        json!({"keyConfirmation":"confirmed"})
    );
    stream
}

pub fn write_response(stream: &mut impl Write, status: &str, body: &Value) {
    let media = if status.starts_with('4') {
        "application/problem+json"
    } else {
        "application/json"
    };
    let body = body.to_string();
    write!(
        stream,
        "HTTP/1.1 {status}\r\nContent-Type: {media}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    ).unwrap();
    stream.flush().unwrap();
}

pub fn no_content(stream: &mut impl Write) {
    stream
        .write_all(b"HTTP/1.1 204 No Content\r\nConnection: close\r\n\r\n")
        .unwrap();
}

pub fn assert_delete(stream: Connection, session: &str) {
    let mut stream = BufReader::new(stream);
    let (request, headers, _) = read_request_parts(&mut stream);
    assert_eq!(
        request,
        format!("DELETE /v1/pairing-sessions/{session} HTTP/1.1\r\n")
    );
    assert_bearer(&headers);
    no_content(stream.get_mut());
}

pub fn accept_creation(connection: Connection, snapshot: &Value) {
    let mut stream = BufReader::new(connection);
    let (request, _, _) = read_request_parts(&mut stream);
    assert_eq!(request, "POST /v1/pairing-sessions HTTP/1.1\r\n");
    write_response(
        stream.get_mut(),
        "201 Created",
        &json!({
            "session": snapshot,
            "pairingToken": TOKEN,
            "pairingTokenExpiresAt": snapshot["expiresAt"],
        }),
    );
}

pub fn accept_events(connection: Connection, session: &str) -> Socket {
    accept_socket(
        connection,
        &format!("/v1/pairing-sessions/{session}/events"),
        TOKEN,
    )
}

pub fn accept_delivery(connection: Connection) -> Socket {
    accept_socket(connection, "/v1/receiver/events", RECEIVER_TOKEN)
}

pub fn snapshot(session: &str, state: &str, lifetime: u64) -> Value {
    let expires = OffsetDateTime::now_utc() + Duration::from_secs(lifetime);
    json!({
        "sessionId": session,
        "receiverId": "receiver-1",
        "mode": "new_receiver",
        "state": state,
        "expiresAt": expires.format(&Rfc3339).unwrap(),
        "senderConfirmed": false,
        "receiverConfirmed": false,
    })
}

pub fn claim(mut snapshot: Value) -> Value {
    snapshot["state"] = json!("awaiting_confirmation");
    snapshot["pairingId"] = json!("pairing-1");
    snapshot["senderId"] = json!("sender-1");
    snapshot
}

pub fn send_state(socket: &mut Socket, snapshot: impl std::borrow::Borrow<Value>) {
    send_event(
        socket,
        json!({"type":"pairing_state","version":1,"session":snapshot.borrow()}),
    );
}

pub fn send_event(socket: &mut Socket, event: Value) {
    socket.send(Message::text(event.to_string())).unwrap();
}

pub fn cancel(socket: &mut Socket, mut claimed: Value) {
    claimed["state"] = json!("canceled");
    claimed.as_object_mut().unwrap().remove("pairingId");
    send_state(socket, claimed);
    socket
        .close(Some(CloseFrame {
            code: CloseCode::Policy,
            reason: "pairing_session_closed".into(),
        }))
        .unwrap();
}

pub fn credential_ready(socket: &mut Socket, stream: &mut Connection, mut claimed: Value) {
    claimed["receiverConfirmed"] = json!(true);
    write_response(stream, "200 OK", &claimed);
    claimed["senderConfirmed"] = json!(true);
    claimed["state"] = json!("credential_ready");
    send_state(socket, claimed);
    assert_peer_left(socket);
}

pub fn assert_peer_left(socket: &mut Socket) {
    use tokio_tungstenite::tungstenite::{Error, error::ProtocolError};
    assert!(matches!(
        socket.read(),
        Ok(Message::Close(_))
            | Err(Error::ConnectionClosed
                | Error::Protocol(ProtocolError::ResetWithoutClosingHandshake))
    ));
}
