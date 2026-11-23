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
#[path = "support/add_sender_desktop.rs"]
mod add_sender_desktop;
#[path = "support/add_sender_service.rs"]
mod add_sender_service;
#[path = "support/completion_frames.rs"]
mod completion_frames;
#[path = "support/main_capture.rs"]
mod main_capture;
#[path = "support/name_clipboard.rs"]
mod name_clipboard;
#[path = "support/window_size.rs"]
mod window_size;

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
        for mode in ["visible", "hidden", "reduced", "duplicate", "motion"] {
            let child = Command::new(std::env::current_exe().unwrap())
                .args([
                    "--exact",
                    "pairing_window_tracks_claim_terminal_states_and_credential_readiness",
                    "--ignored",
                ])
                .env(CHILD, mode)
                .status()
                .unwrap();
            assert!(child.success(), "{mode}");
        }
        return;
    };
    let hidden = mode == "hidden";
    let reduced = mode == "reduced";
    let duplicate = mode == "duplicate";
    let saved_name_evidence = mode == "name-saved";
    select_backend();
    if mode == "motion" {
        let evidence = std::env::var_os("PIN_GLIDE_PAIRING_EVIDENCE");
        completion_frames::verify_transition(evidence.as_deref().map(Path::new));
        return;
    }
    let (listener, service) = listen();
    let (advance, release) = mpsc::channel();
    let (cancelled, may_refresh) = mpsc::channel();
    let (send_qr, scanned) = mpsc::channel();
    let (pong, delivered) = mpsc::channel();
    let fixture = issue_sessions(listener, release, cancelled, scanned, pong);
    let directory = tempfile::tempdir().unwrap();
    let registration = directory.path().join("registration.json");
    main_capture::seed_sender_names(&registration, duplicate, hidden);
    let stored = registration.clone();
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
    let evidence = (mode == "visible" || duplicate || saved_name_evidence)
        .then(|| std::env::var_os("PIN_GLIDE_PAIRING_EVIDENCE"))
        .flatten();
    let final_name = if saved_name_evidence {
        "办公室手机"
    } else {
        "我的设备"
    };
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
    let mut main_expected_at: Option<Instant> = None;
    let mut main_label: Option<mpsc::Receiver<bool>> = None;
    let expected = if hidden {
        "未命名设备-3"
    } else {
        final_name
    };
    timer.start(TimerMode::Repeated, Duration::from_millis(12), move || {
        assert!(started.elapsed() < Duration::from_secs(18));
        if let Some(opened_at) = main_expected_at {
            main_capture::check_main(
                opened_at,
                &mut main_label,
                expected,
                evidence.as_deref().map(Path::new),
            );
            return;
        }
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
            if hidden {
                assert!(!window.window().is_visible());
                if delivered.try_recv().is_err() {
                    return;
                }
                assert!(main_capture::main_window().is_none());
                ui::open();
                assert!(!window.window().is_visible());
                assert_eq!(saved_name(&stored), "未命名设备-3");
                main_expected_at = Some(Instant::now());
                return;
            }
            let completed_at = completed_at.get_or_insert_with(|| {
                assert!(window.window().is_visible());
                window.set_sender_name("我的设备".into());
                Instant::now()
            });
            if !reduced && completed_at.elapsed() < Duration::from_millis(210) {
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
                        capture(window.window(), path);
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
        } else if let Some(path) = &evidence_path
            && !duplicate
        {
            capture(window.window(), path);
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
            Step::Finish => {
                completion_frames::verify_done_pointer_states(&window);
                verify_pending_name_route(&window);
                verify_naming(
                    &window,
                    &stored,
                    duplicate,
                    final_name,
                    evidence.as_deref().map(Path::new),
                );
                if duplicate {
                    slint::quit_event_loop().unwrap();
                } else {
                    main_expected_at = Some(Instant::now());
                }
            }
            Step::WaitForCancellation | Step::Refresh => window.invoke_refresh(),
            Step::Scan => unreachable!(),
        }
    });
    slint::run_event_loop_until_quit().unwrap();
    timer.stop();
    ui::close();
    fixture.join().unwrap();
}

fn verify_pending_name_route(window: &PairingWindow) {
    window.set_sender_name("Draft".into());
    window.hide().unwrap();
    ui::open_settings();
    assert!(window.window().is_visible());
    assert_eq!(window.get_sender_name(), "Draft");
    ui::open();
    assert!(main_capture::main_window().is_none());
}

fn saved_name(path: &Path) -> String {
    let stored: Value = serde_json::from_slice(&fs::read(path).unwrap()).unwrap();
    stored["active_pairings"]["pairing-1"]["name"]
        .as_str()
        .unwrap()
        .to_owned()
}

fn verify_naming(
    window: &PairingWindow,
    store: &Path,
    duplicate: bool,
    final_name: &str,
    evidence: Option<&Path>,
) {
    let too_long = "我的备用设备".repeat(7);
    let cases = [
        ("empty", "", "请填写设备名称。"),
        ("too-long", &too_long, "设备名称最多 40 个字符。"),
        ("duplicate", "我的设备", "已有一台同名设备，请换一个名称。"),
    ];
    for (state, input, error) in cases {
        if (state == "duplicate") != duplicate {
            continue;
        }
        window.set_sender_name(input.into());
        window.set_name_error("".into());
        if let Some(directory) = evidence {
            capture(
                window.window(),
                &directory.join(format!("pair-done-input-{state}.ppm")),
            );
        }
        window.invoke_finish_naming();
        assert_eq!(window.get_name_error(), error);
        assert!(window.window().is_visible());
        assert_eq!(saved_name(store), "未命名设备");
        if let Some(directory) = evidence {
            capture(
                window.window(),
                &directory.join(format!("pair-done-error-{state}.ppm")),
            );
        }
    }
    if !duplicate {
        window.set_sender_name("".into());
        name_clipboard::verify_paste(window);
        window.invoke_finish_naming();
        assert_eq!(window.get_name_error(), "名称包含无效字符，请重新输入。");
        assert_eq!(saved_name(store), "未命名设备");
        if let Some(directory) = evidence {
            capture(
                window.window(),
                &directory.join("pair-done-error-invalid-character.ppm"),
            );
        }
        name_clipboard::type_text(window, "X");
        assert_eq!(window.get_name_error(), "");
        window.set_sender_name(format!("　{final_name}\t").into());
        window.invoke_finish_naming();
        assert!(!window.window().is_visible());
        assert_eq!(saved_name(store), final_name);
    }
}

fn capture(window: &slint::Window, path: &Path) {
    let pixels = window.take_snapshot().unwrap();
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
    pong: mpsc::Sender<()>,
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
                    let mut complete = accept_completion(accept(&acceptor, &listener), &session_id);
                    credential_ready(&mut socket, complete.get_mut(), claimed);
                    issue_credential(
                        &acceptor,
                        &listener,
                        &release,
                        &session_id,
                        index == 3,
                        &pong,
                    );
                }
            }
        }
    })
}

fn select_backend() {
    let mut builder = EventLoop::with_user_event();
    builder.with_any_thread(true);
    slint::BackendSelector::new()
        .backend_name("winit".into())
        .renderer_name("software".into())
        .with_winit_event_loop_builder(builder)
        .select()
        .unwrap();
}

fn issue_credential(
    acceptor: &native_tls::TlsAcceptor,
    listener: &TcpListener,
    release: &mpsc::Receiver<()>,
    session_id: &str,
    reject_upgrade: bool,
    pong: &mpsc::Sender<()>,
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
            "401 Unauthorized",
            &json!({"code":"credential_invalid"}),
        );
        return;
    }
    use tokio_tungstenite::tungstenite::Message;
    let mut delivery = accept_delivery(connection);
    delivery.send(Message::Ping(vec![29].into())).unwrap();
    assert_eq!(delivery.read().unwrap(), Message::Pong(vec![29].into()));
    pong.send(()).unwrap();
    assert_peer_left(&mut delivery);
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
