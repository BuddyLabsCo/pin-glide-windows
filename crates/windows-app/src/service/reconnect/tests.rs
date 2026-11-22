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
