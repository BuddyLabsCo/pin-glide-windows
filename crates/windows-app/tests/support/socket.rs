use std::io::{BufRead, Read, Write};

use tokio_tungstenite::tungstenite::{
    WebSocket, accept_hdr,
    handshake::server::{Request, Response},
};

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

pub fn accept_socket<Stream: Read + Write>(
    stream: Stream,
    path: &str,
    token: &str,
) -> WebSocket<Stream> {
    #[expect(
        clippy::result_large_err,
        reason = "tungstenite fixes the callback error type"
    )]
    let socket = accept_hdr(stream, |request: &Request, mut response: Response| {
        assert_eq!(request.uri().path(), path);
        assert_eq!(
            request.headers()["authorization"],
            format!("Bearer {token}")
        );
        assert_eq!(request.headers()["sec-websocket-protocol"], "pin-glide.v1");
        response
            .headers_mut()
            .insert("sec-websocket-protocol", "pin-glide.v1".parse().unwrap());
        Ok(response)
    });
    socket.unwrap()
}
