use std::io::{Read, Write};

use tokio_tungstenite::tungstenite::{
    WebSocket, accept_hdr,
    handshake::server::{Request, Response},
};

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
