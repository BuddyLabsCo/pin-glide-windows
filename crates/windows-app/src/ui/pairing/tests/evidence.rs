use std::{fs, path::Path};

use super::*;

#[test]
#[ignore = "requires an interactive Windows desktop"]
fn renders_key_check_failure_cancellation_and_expiry() {
    const CHILD: &str = "PIN_GLIDE_PAIRING_EVIDENCE_CHILD";
    if std::env::var_os(CHILD).is_none() {
        let child = Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "ui::pairing::tests::evidence::renders_key_check_failure_cancellation_and_expiry",
                "--ignored",
            ])
            .env(CHILD, "1")
            .env("SLINT_SCALE_FACTOR", "1")
            .status()
            .unwrap();
        assert!(child.success());
        return;
    }
    select_test_backend().unwrap();
    let (listener, origin) = listen("localhost");
    SERVICES.with_borrow_mut(|services| {
        services.extend((0..3).map(|_| local_service(&listener, &origin, true)));
    });
    let (advance, release) = mpsc::channel();
    let (cancelled, may_refresh) = mpsc::channel();
    let fixture = issue_sessions(listener, release, cancelled);
    super::super::open(true).unwrap();
    let window = PAIRING.with_borrow(|pairing| pairing.as_ref().unwrap().window.clone_strong());
    window.set_motion_enabled(false);
    let evidence = std::env::var_os("PIN_GLIDE_PAIRING_EVIDENCE");
    let mut states = [
        (PairingStage::Confirming, "pair-key-check"),
        (PairingStage::Failed, "pair-failed"),
        (PairingStage::Canceled, "pair-canceled"),
        (PairingStage::Expired, "pair-expired"),
    ]
    .into_iter()
    .peekable();
    let timer = Timer::default();
    let started = Instant::now();
    let mut frame = None;
    timer.start(TimerMode::Repeated, Duration::from_millis(60), move || {
        assert!(started.elapsed() < Duration::from_secs(18));
        let Some(&(stage, name)) = states.peek() else {
            return;
        };
        if window.get_stage() != stage {
            return;
        }
        if stage == PairingStage::Failed && may_refresh.try_recv().is_err() {
            return;
        }
        assert!(window.window().is_visible());
        assert!(window.get_qr().to_rgb8().is_none());
        if stage == PairingStage::Confirming {
            let current = window.window().take_snapshot().unwrap();
            if let Some(previous) = frame.replace(current.clone()) {
                if !window.get_motion_enabled() {
                    assert!(previous.as_slice() == current.as_slice());
                    if let Some(directory) = &evidence {
                        capture(&window, &Path::new(directory).join(format!("{name}.ppm")));
                    }
                    window.set_motion_enabled(true);
                    return;
                }
                if previous.as_slice() == current.as_slice() {
                    return;
                }
            } else {
                return;
            }
        }
        if stage != PairingStage::Confirming
            && let Some(directory) = &evidence
        {
            capture(&window, &Path::new(directory).join(format!("{name}.ppm")));
        }
        states.next();
        match stage {
            PairingStage::Confirming => advance.send(()).unwrap(),
            PairingStage::Expired => slint::quit_event_loop().unwrap(),
            _ => window.invoke_refresh(),
        }
    });
    slint::run_event_loop_until_quit().unwrap();
    timer.stop();
    ui::close();
    fixture.join().unwrap();
}

fn capture(window: &PairingWindow, path: &Path) {
    let pixels = window.window().take_snapshot().unwrap();
    assert_eq!((pixels.width(), pixels.height()), (440, 560));
    let mut file = fs::File::create(path).unwrap();
    write!(file, "P6\n{} {}\n255\n", pixels.width(), pixels.height()).unwrap();
    let rgb: Vec<_> = pixels
        .as_slice()
        .iter()
        .flat_map(|pixel| [pixel.r, pixel.g, pixel.b])
        .collect();
    file.write_all(&rgb).unwrap();
}

fn issue_sessions(
    listener: TcpListener,
    release: mpsc::Receiver<()>,
    cancelled: mpsc::Sender<()>,
) -> JoinHandle<()> {
    let acceptor = acceptor();
    thread::spawn(move || {
        for (index, lifetime) in [30, 30, 2].into_iter().enumerate() {
            let mut creation = BufReader::new(acceptor.accept(accept_loopback(&listener)).unwrap());
            assert_eq!(
                read_request(&mut creation),
                "POST /v1/pairing-sessions HTTP/1.1\r\n"
            );
            let session_id = format!("fixture-{}", index + 1);
            let expiry = OffsetDateTime::now_utc() + Duration::from_secs(lifetime);
            let mut state = snapshot("awaiting_authorization");
            state["sessionId"] = json!(session_id);
            state["expiresAt"] = json!(expiry.format(&Rfc3339).unwrap());
            write_response(
                creation.get_mut(),
                "201 Created",
                &json!({
                    "session": state,
                    "pairingToken": TOKEN,
                    "pairingTokenExpiresAt": state["expiresAt"],
                }),
            );
            drop(creation);
            let mut socket = accept_events(
                acceptor.accept(accept_loopback(&listener)).unwrap(),
                &format!("/v1/pairing-sessions/{session_id}/events"),
            );
            match index {
                0 => {
                    state["state"] = json!("awaiting_confirmation");
                    state["pairingId"] = json!("fixture-pairing");
                    state["senderId"] = json!("fixture-sender");
                    send_state(&mut socket, state);
                    release.recv_timeout(Duration::from_secs(10)).unwrap();
                    socket
                        .send(tokio_tungstenite::tungstenite::Message::text(
                            json!({
                                "version":1, "type":"pairing_relay", "sessionId":session_id,
                                "side":"sender", "payload":"invalid",
                            })
                            .to_string(),
                        ))
                        .unwrap();
                    let mut delete =
                        BufReader::new(acceptor.accept(accept_loopback(&listener)).unwrap());
                    assert_eq!(
                        read_request(&mut delete),
                        "DELETE /v1/pairing-sessions/fixture-1 HTTP/1.1\r\n"
                    );
                    delete
                        .get_mut()
                        .write_all(b"HTTP/1.1 204 No Content\r\nConnection: close\r\n\r\n")
                        .unwrap();
                    cancelled.send(()).unwrap();
                }
                1 => {
                    state["state"] = json!("canceled");
                    send_state(&mut socket, state);
                    let _ = socket.close(Some(tokio_tungstenite::tungstenite::protocol::CloseFrame {
                        code: tokio_tungstenite::tungstenite::protocol::frame::coding::CloseCode::Policy,
                        reason: "fixture canceled".into(),
                    }));
                }
                _ => {
                    send_state(&mut socket, state);
                    let _ = socket.read();
                }
            }
        }
    })
}
