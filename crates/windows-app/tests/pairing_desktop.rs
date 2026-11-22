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

#[derive(Clone, Copy)]
enum Step {
    Advance,
    VerifyKeyCheckMotion,
    WaitForCancellation,
    Scan,
    Refresh,
    ConnectForCompletion,
    Finish,
}

#[test]
#[ignore = "requires an interactive Windows desktop"]
fn pairing_window_tracks_claim_terminal_states_and_credential_readiness() {
    const CHILD: &str = "PIN_GLIDE_PAIRING_EVIDENCE_CHILD";
    let Ok(mode) = std::env::var(CHILD) else {
        for mode in ["visible", "hidden", "reduced"] {
            let child = Command::new(std::env::current_exe().unwrap())
                .args([
                    "--exact",
                    "pairing_window_tracks_claim_terminal_states_and_credential_readiness",
                    "--ignored",
                ])
                .env(CHILD, mode)
                .env("SLINT_SCALE_FACTOR", "1")
                .status()
                .unwrap();
            assert!(child.success(), "{mode}");
        }
        return;
    };
    let hidden = mode == "hidden";
    let reduced = mode == "reduced";
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
    let window = ui::open_pairing(true, move |notify, start| {
        let service = service.clone();
        PairingOperation::start_with_registration(
            move || Some(service),
            notify,
            registration.clone(),
            start,
        )
    })
    .unwrap();
    window.set_motion_enabled(false);
    let evidence = (mode == "visible")
        .then(|| std::env::var_os("PIN_GLIDE_PAIRING_EVIDENCE"))
        .flatten();
    let mut states = [
        (PairingStage::Waiting, Step::Advance, None),
        (
            PairingStage::Confirming,
            Step::VerifyKeyCheckMotion,
            Some("pair-key-check"),
        ),
        (
            PairingStage::Failed,
            Step::WaitForCancellation,
            Some("pair-failed"),
        ),
        (PairingStage::Waiting, Step::Advance, None),
        (PairingStage::Confirming, Step::Advance, None),
        (PairingStage::Canceled, Step::Refresh, Some("pair-canceled")),
        (PairingStage::Expired, Step::Refresh, Some("pair-expired")),
        (PairingStage::Waiting, Step::Scan, None),
        (
            PairingStage::CredentialReady,
            Step::Advance,
            Some("pair-credential-ready"),
        ),
        (
            PairingStage::Checking,
            Step::Advance,
            Some("pair-uncertain"),
        ),
        (PairingStage::Connecting, Step::Advance, Some("pair-issued")),
        (PairingStage::Failed, Step::Refresh, Some("upgrade-failed")),
        (PairingStage::Waiting, Step::Scan, None),
        (PairingStage::Connecting, Step::ConnectForCompletion, None),
        (PairingStage::Completed, Step::Finish, Some("pair-done")),
    ]
    .into_iter()
    .peekable();
    let timer = Timer::default();
    let started = Instant::now();
    let mut frame = None;
    let mut completed_at = None;
    let mut completion_frames = CompletionFrames::default();
    timer.start(TimerMode::Repeated, Duration::from_millis(20), move || {
        assert!(started.elapsed() < Duration::from_secs(18));
        let Some(&(stage, step, capture_name)) = states.peek() else {
            return;
        };
        if window.get_stage() != stage {
            return;
        }
        if matches!(step, Step::WaitForCancellation) && may_refresh.try_recv().is_err() {
            return;
        }
        if matches!(step, Step::Finish) {
            let completed_at = completed_at.get_or_insert_with(|| {
                assert_eq!(window.window().is_visible(), !hidden);
                if hidden {
                    ui::open();
                    assert!(window.window().is_visible());
                    assert_eq!(window.get_stage(), PairingStage::Completed);
                }
                Instant::now()
            });
            if !completion_frames.observe(
                &window,
                completed_at.elapsed(),
                !hidden && !reduced,
                evidence.as_deref().map(Path::new),
            ) {
                return;
            }
        }
        assert!(window.window().is_visible());
        if matches!(step, Step::Scan) {
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
        assert_eq!(
            window.get_qr().to_rgb8().is_some(),
            stage == PairingStage::Waiting
        );
        let evidence_path = evidence
            .as_deref()
            .zip(capture_name)
            .map(|(directory, name)| Path::new(directory).join(format!("{name}.ppm")));
        if matches!(step, Step::VerifyKeyCheckMotion) {
            let current = window.window().take_snapshot().unwrap();
            if let Some(previous) = frame.replace(current.clone()) {
                if !window.get_motion_enabled() {
                    assert!(previous.as_slice() == current.as_slice());
                    if let Some(path) = &evidence_path {
                        capture(&window, path);
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
        } else if let Some(path) = &evidence_path {
            capture(&window, path);
        }
        states.next();
        match step {
            Step::Advance | Step::VerifyKeyCheckMotion => advance.send(()).unwrap(),
            Step::ConnectForCompletion => {
                window.set_motion_enabled(!reduced);
                if hidden {
                    window.window().hide().unwrap();
                }
                advance.send(()).unwrap();
            }
            Step::Finish => slint::quit_event_loop().unwrap(),
            Step::WaitForCancellation | Step::Refresh => window.invoke_refresh(),
            Step::Scan => unreachable!(),
        }
    });
    slint::run_event_loop_until_quit().unwrap();
    timer.stop();
    ui::close();
    fixture.join().unwrap();
}

#[derive(Default)]
struct CompletionFrames {
    first: Option<slint::SharedPixelBuffer<slint::Rgba8Pixel>>,
    transition: Option<slint::SharedPixelBuffer<slint::Rgba8Pixel>>,
}

impl CompletionFrames {
    fn observe(
        &mut self,
        window: &PairingWindow,
        elapsed: Duration,
        moving: bool,
        evidence: Option<&Path>,
    ) -> bool {
        let frame = window.window().take_snapshot().unwrap();
        let first = self.first.get_or_insert_with(|| frame.clone());
        if moving
            && self.transition.is_none()
            && elapsed >= Duration::from_millis(55)
            && elapsed < Duration::from_millis(120)
        {
            self.transition = Some(frame.clone());
            if let Some(directory) = evidence {
                capture(window, &directory.join("pair-done-transition.ppm"));
            }
        }
        if elapsed < Duration::from_millis(200) {
            return false;
        }
        let sample = |pixels: &slint::SharedPixelBuffer<slint::Rgba8Pixel>| {
            pixels.as_slice()[160 * pixels.width() as usize + 220]
        };
        if moving {
            let first = sample(first);
            let during = sample(self.transition.as_ref().expect("completion fade frame"));
            let settled = sample(&frame);
            assert!(first.r > during.r && during.r > settled.r);
        } else {
            assert!(first.as_slice() == frame.as_slice());
        }
        true
    }
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
        for (index, lifetime) in [30, 30, 2, 30, 30].into_iter().enumerate() {
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
                    issue_credential(&acceptor, &listener, &release, &session_id, index == 3);
                }
            }
        }
    })
}

fn issue_credential(
    acceptor: &native_tls::TlsAcceptor,
    listener: &TcpListener,
    release: &mpsc::Receiver<()>,
    session_id: &str,
    reject_upgrade: bool,
) {
    let mut original = BufReader::new(accept(acceptor, listener));
    assert_redeem(&mut original, session_id, 0);
    let mut redemption = if reject_upgrade {
        release.recv_timeout(Duration::from_secs(5)).unwrap();
        drop(original);
        let mut retry = BufReader::new(accept(acceptor, listener));
        assert_redeem(&mut retry, session_id, 1);
        release.recv_timeout(Duration::from_secs(5)).unwrap();
        retry
    } else {
        original
    };
    write_response(
        redemption.get_mut(),
        "200 OK",
        &json!({"receiverId": "receiver-1", "receiverToken": RECEIVER_TOKEN}),
    );
    let connection = accept(acceptor, listener);
    release.recv_timeout(Duration::from_secs(5)).unwrap();
    if reject_upgrade {
        let mut upgrade = BufReader::new(connection);
        assert_eq!(
            read_request_parts(&mut upgrade).0,
            "GET /v1/receiver/events HTTP/1.1\r\n"
        );
        write_response(
            upgrade.get_mut(),
            "409 Conflict",
            &json!({"code":"receiver_already_connected"}),
        );
        return;
    }
    assert_peer_left(&mut accept_delivery(connection));
}

fn assert_redeem(connection: &mut BufReader<Connection>, session_id: &str, attempt: i32) {
    let (request, headers, body) = read_request_parts(connection);
    assert_eq!(
        request,
        format!("POST /v1/pairing-sessions/{session_id}/receiver-token HTTP/1.1\r\n")
    );
    assert_bearer(&headers);
    let body: Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(body["attempt"], attempt);
}
