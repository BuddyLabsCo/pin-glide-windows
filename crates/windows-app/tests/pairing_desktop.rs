#![cfg(windows)]

#[path = "support/pairing.rs"]
mod fixture;

use serde_json::{Value, json};
use slint::{
    ComponentHandle, Timer, TimerMode,
    winit_030::winit::{event_loop::EventLoop, platform::windows::EventLoopBuilderExtWindows},
};
use std::{
    fs,
    io::{BufReader, Write},
    net::TcpListener,
    path::Path,
    process::Command,
    sync::mpsc,
    thread::{self, JoinHandle},
    time::{Duration, Instant},
};
use windows_app::{PairingStage, PairingWindow, service::PairingOperation, ui};

use fixture::*;

#[test]
#[ignore = "requires an interactive Windows desktop"]
fn pairing_window_tracks_claim_terminal_states_and_credential_readiness() {
    const CHILD: &str = "PIN_GLIDE_PAIRING_EVIDENCE_CHILD";
    if std::env::var_os(CHILD).is_none() {
        let child = Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "pairing_window_tracks_claim_terminal_states_and_credential_readiness",
                "--ignored",
            ])
            .env(CHILD, "1")
            .env("SLINT_SCALE_FACTOR", "1")
            .status()
            .unwrap();
        assert!(child.success());
        return;
    }
    let mut builder = EventLoop::with_user_event();
    builder.with_any_thread(true);
    slint::BackendSelector::new()
        .backend_name("winit".into())
        .renderer_name("software".into())
        .with_winit_event_loop_builder(builder)
        .select()
        .unwrap();
    let (listener, service) = listen();
    let (advance, release) = mpsc::channel();
    let (cancelled, may_refresh) = mpsc::channel();
    let (send_qr, scanned) = mpsc::channel();
    let fixture = issue_sessions(listener, release, cancelled, scanned);
    let directory = tempfile::tempdir().unwrap();
    let registration = directory.path().join("registration.json");
    let window = ui::open_pairing(true, move |notify| {
        let service = service.clone();
        PairingOperation::start_with_registration(
            move || Some(service),
            notify,
            registration.clone(),
        )
    })
    .unwrap();
    window.set_motion_enabled(false);
    let evidence = std::env::var_os("PIN_GLIDE_PAIRING_EVIDENCE");
    let mut states = [
        (PairingStage::Waiting, ""),
        (PairingStage::Confirming, "pair-key-check"),
        (PairingStage::Failed, "pair-failed"),
        (PairingStage::Waiting, ""),
        (PairingStage::Confirming, ""),
        (PairingStage::Canceled, "pair-canceled"),
        (PairingStage::Expired, "pair-expired"),
        (PairingStage::Waiting, "scan-to-prepare"),
        (PairingStage::CredentialReady, "pair-credential-ready"),
        (PairingStage::Checking, "pair-uncertain"),
        (PairingStage::Connecting, "pair-issued"),
        (PairingStage::Completed, "pair-done"),
    ]
    .into_iter()
    .peekable();
    let timer = Timer::default();
    let started = Instant::now();
    let mut frame = None;
    let mut reopened = None;
    let mut captured_transition = false;
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
        if stage == PairingStage::Completed {
            let reopened = reopened.get_or_insert_with(|| {
                assert!(!window.window().is_visible());
                ui::open();
                assert_eq!(window.get_stage(), PairingStage::Completed);
                Instant::now()
            });
            if !captured_transition
                && reopened.elapsed() >= Duration::from_millis(50)
                && let Some(directory) = &evidence
            {
                capture(
                    &window,
                    &Path::new(directory).join("pair-done-transition.ppm"),
                );
                captured_transition = true;
            }
            if reopened.elapsed() < Duration::from_millis(200) {
                return;
            }
        }
        assert!(window.window().is_visible());
        if name == "scan-to-prepare" {
            let image = window.get_qr().to_rgb8().unwrap();
            let pixels: Vec<_> = image.as_slice().iter().map(|pixel| pixel.r).collect();
            send_qr
                .send(decode_pixels(
                    image.width() as usize,
                    image.height() as usize,
                    &pixels,
                ))
                .unwrap();
            states.next();
            return;
        }
        if stage == PairingStage::Waiting || (stage == PairingStage::Confirming && name.is_empty())
        {
            assert_eq!(
                window.get_qr().to_rgb8().is_some(),
                stage == PairingStage::Waiting
            );
            advance.send(()).unwrap();
            states.next();
            return;
        }
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
            if stage == PairingStage::CredentialReady {
                window.set_motion_enabled(false);
            }
            capture(&window, &Path::new(directory).join(format!("{name}.ppm")));
        }
        states.next();
        match stage {
            PairingStage::Confirming => advance.send(()).unwrap(),
            PairingStage::CredentialReady | PairingStage::Checking => advance.send(()).unwrap(),
            PairingStage::Connecting => {
                window.window().hide().unwrap();
                advance.send(()).unwrap();
            }
            PairingStage::Completed => slint::quit_event_loop().unwrap(),
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
    scanned: mpsc::Receiver<Value>,
) -> JoinHandle<()> {
    thread::spawn(move || {
        let acceptor = acceptor();
        for (index, lifetime) in [30, 30, 2, 30].into_iter().enumerate() {
            let session_id = format!("fixture-{}", index + 1);
            let state = snapshot(&session_id, "awaiting_authorization", lifetime);
            accept_creation(accept(&acceptor, &listener), &state);
            let mut socket = accept_events(accept(&acceptor, &listener), &session_id);
            send_state(&mut socket, &state);
            if index < 2 {
                release.recv_timeout(Duration::from_secs(10)).unwrap();
                send_state(&mut socket, claim(state.clone()));
                release.recv_timeout(Duration::from_secs(10)).unwrap();
            }
            match index {
                0 => {
                    send_event(
                        &mut socket,
                        json!({
                                    "version":1, "type":"pairing_relay", "sessionId":session_id,
                                    "side":"sender", "payload":"invalid",
                        }),
                    );
                    assert_delete(accept(&acceptor, &listener), &session_id);
                    cancelled.send(()).unwrap();
                }
                1 => {
                    cancel(&mut socket, claim(state));
                }
                2 => {
                    let _ = socket.read();
                }
                _ => {
                    let qr = scanned.recv_timeout(Duration::from_secs(5)).unwrap();
                    let claimed = claim(state);
                    send_state(&mut socket, &claimed);
                    sender_handshake(&mut socket, &qr);
                    let mut complete = BufReader::new(accept(&acceptor, &listener));
                    let (request, headers, _) = read_request_parts(&mut complete);
                    assert_eq!(
                        request,
                        format!("POST /v1/pairing-sessions/{session_id}/complete HTTP/1.1\r\n")
                    );
                    assert_bearer(&headers);
                    credential_ready(&mut socket, complete.get_mut(), claimed);
                    issue_credential(&acceptor, &listener, &release);
                }
            }
        }
    })
}

fn issue_credential(
    acceptor: &native_tls::TlsAcceptor,
    listener: &TcpListener,
    release: &mpsc::Receiver<()>,
) {
    let mut original = BufReader::new(accept(acceptor, listener));
    assert_redeem(&mut original, 0);
    release.recv_timeout(Duration::from_secs(5)).unwrap();
    drop(original);
    let mut retry = BufReader::new(accept(acceptor, listener));
    assert_redeem(&mut retry, 1);
    release.recv_timeout(Duration::from_secs(5)).unwrap();
    write_response(
        retry.get_mut(),
        "200 OK",
        &json!({"receiverId": "receiver-1", "receiverToken": RECEIVER_TOKEN}),
    );
    let connection = accept(acceptor, listener);
    release.recv_timeout(Duration::from_secs(5)).unwrap();
    assert_peer_left(&mut accept_delivery(connection));
}

fn assert_redeem(connection: &mut BufReader<Connection>, attempt: i32) {
    let (request, headers, body) = read_request_parts(connection);
    assert_eq!(
        request,
        "POST /v1/pairing-sessions/fixture-4/receiver-token HTTP/1.1\r\n"
    );
    assert_bearer(&headers);
    let body: Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(body["attempt"], attempt);
}
