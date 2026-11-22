use std::{
    io,
    net::{TcpListener, TcpStream},
    thread,
    time::{Duration, Instant},
};

use native_tls::{Identity, TlsAcceptor};

pub fn listen(host: &str) -> (TcpListener, String) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let origin = format!("https://{host}:{}", listener.local_addr().unwrap().port());
    (listener, origin)
}

pub fn accept_loopback(listener: &TcpListener) -> TcpStream {
    let timeout = Duration::from_secs(5);
    let deadline = Instant::now() + timeout;
    listener.set_nonblocking(true).unwrap();
    let stream = loop {
        match listener.accept() {
            Ok((stream, _)) => break stream,
            Err(error) if error.kind() == io::ErrorKind::WouldBlock => {
                assert!(
                    Instant::now() < deadline,
                    "client did not reach the TLS fixture"
                );
                thread::sleep(Duration::from_millis(10));
            }
            Err(error) => panic!("TLS fixture accept failed: {error}"),
        }
    };
    stream.set_nonblocking(false).unwrap();
    stream.set_read_timeout(Some(timeout)).unwrap();
    stream.set_write_timeout(Some(timeout)).unwrap();
    stream
}

pub fn acceptor() -> TlsAcceptor {
    let identity = Identity::from_pkcs12(
        include_bytes!("../fixtures/service/server.p12"),
        "pin-glide-test",
    )
    .unwrap();
    TlsAcceptor::new(identity).unwrap()
}
