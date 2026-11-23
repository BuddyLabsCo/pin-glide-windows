use std::{
    cell::RefCell,
    io::{BufReader, Write},
    net::TcpListener,
    process::Command,
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
        mpsc,
    },
    thread::{self, JoinHandle},
    time::{Duration, Instant},
};

use serde_json::json;
use slint::{
    ComponentHandle, Rgb8Pixel, Timer, TimerMode,
    winit_030::{
        WinitWindowAccessor,
        winit::raw_window_handle::{HasWindowHandle, RawWindowHandle},
    },
};
use time::{OffsetDateTime, format_description::well_known::Rfc3339};
use windows_sys::Win32::UI::WindowsAndMessaging::{PostMessageW, WM_CLOSE};

use super::{PAIRING, PairingStage, PairingWindow, Service};
use crate::{
    service::{
        pairing_operation::tests::{
            TOKEN, accept_events, read_request, send_state, snapshot, write_response,
        },
        tests::{accept_loopback, acceptor, listen, local_service},
    },
    startup_tests::select_test_backend,
    ui::{self, WELCOME},
};

mod add_sender;

thread_local! {
    static SERVICES: RefCell<Vec<Service>> = const { RefCell::new(Vec::new()) };
}

pub(super) fn connect() -> Option<Service> {
    SERVICES.with_borrow_mut(Vec::pop)
}

enum Step {
    FirstQr,
    Hidden {
        pixels: Vec<Rgb8Pixel>,
        countdown: String,
    },
    Expiry(Vec<Rgb8Pixel>),
    Refreshed(Vec<Rgb8Pixel>),
}

#[test]
#[ignore = "requires an interactive Windows desktop"]
fn closes_reopens_and_refreshes_the_pairing_view() {
    const CHILD: &str = "PIN_GLIDE_PAIRING_TEST_CHILD";
    if std::env::var_os(CHILD).is_none() {
        let child = Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "ui::pairing::tests::closes_reopens_and_refreshes_the_pairing_view",
                "--ignored",
            ])
            .env(CHILD, "1")
            .status()
            .unwrap();
        assert!(
            child.success(),
            "pairing desktop test failed in its child process"
        );
        return;
    }
    select_test_backend().unwrap();
    let (listener, origin) = listen("localhost");
    SERVICES.with_borrow_mut(|services| {
        services.extend((0..2).map(|_| local_service(&listener, &origin, true)));
    });
    let created = Arc::new(AtomicUsize::new(0));
    let (stop, stopped) = mpsc::channel();
    let fixture = desktop_sessions(listener, Arc::clone(&created), stopped);
    ui::open();
    WELCOME.with_borrow(|welcome| welcome.as_ref().unwrap().invoke_start_pairing());
    let window = PAIRING.with_borrow(|pairing| pairing.as_ref().unwrap().window.clone_strong());
    let timer = Timer::default();
    let started = Instant::now();
    let mut step = Some(Step::FirstQr);
    timer.start(TimerMode::Repeated, Duration::from_millis(30), move || {
        assert!(
            started.elapsed() < Duration::from_secs(12),
            "pairing view did not finish the flow"
        );
        step = advance(step.take().unwrap(), &window, &created);
        if step.is_none() {
            slint::quit_event_loop().unwrap();
        }
    });
    slint::run_event_loop_until_quit().unwrap();
    timer.stop();
    ui::close();
    stop.send(()).unwrap();
    fixture.join().unwrap();
}

fn advance(step: Step, window: &PairingWindow, created: &AtomicUsize) -> Option<Step> {
    let stage = window.get_stage();
    Some(match step {
        Step::FirstQr if stage == PairingStage::Waiting => {
            assert!(window.window().is_visible());
            close_window(window);
            Step::Hidden {
                pixels: qr_pixels(window),
                countdown: window.get_remaining().to_string(),
            }
        }
        Step::Hidden { pixels, countdown }
            if !window.window().is_visible()
                && window.get_remaining().as_str() < countdown.as_str() =>
        {
            assert_eq!(stage, PairingStage::Waiting);
            let current_countdown = window.get_remaining();
            ui::open();
            assert!(window.window().is_visible());
            assert_eq!(qr_pixels(window), pixels);
            assert!(window.get_remaining() <= current_countdown);
            assert_eq!(created.load(Ordering::SeqCst), 1);
            Step::Expiry(pixels)
        }
        Step::Expiry(pixels) if stage == PairingStage::Expired => {
            assert!(window.get_qr().to_rgb8().is_none());
            window.invoke_refresh();
            window.invoke_refresh();
            Step::Refreshed(pixels)
        }
        Step::Refreshed(first) if stage == PairingStage::Waiting => {
            assert_ne!(qr_pixels(window), first);
            assert_eq!(created.load(Ordering::SeqCst), 2);
            return None;
        }
        step => step,
    })
}

fn close_window(window: &PairingWindow) {
    window
        .window()
        .with_winit_window(|native| {
            let RawWindowHandle::Win32(handle) = native.window_handle().unwrap().as_raw() else {
                panic!("expected a Windows pairing window");
            };
            // SAFETY: the HWND is live for the duration of the native window callback.
            let posted = unsafe { PostMessageW(handle.hwnd.get() as _, WM_CLOSE, 0, 0) };
            assert_ne!(posted, 0);
        })
        .unwrap();
}

fn qr_pixels(window: &PairingWindow) -> Vec<Rgb8Pixel> {
    window.get_qr().to_rgb8().unwrap().as_slice().to_vec()
}

fn desktop_sessions(
    listener: TcpListener,
    created: Arc<AtomicUsize>,
    stop: mpsc::Receiver<()>,
) -> JoinHandle<()> {
    thread::spawn(move || {
        let acceptor = acceptor();
        for (session, lifetime) in [("session-1", 5), ("session-2", 30)] {
            let mut creation = BufReader::new(acceptor.accept(accept_loopback(&listener)).unwrap());
            assert_eq!(
                read_request(&mut creation),
                "POST /v1/pairing-sessions HTTP/1.1\r\n"
            );
            created.fetch_add(1, Ordering::SeqCst);
            let expiry = OffsetDateTime::now_utc() + Duration::from_secs(lifetime);
            let mut state = snapshot("awaiting_authorization");
            state["sessionId"] = json!(session);
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
                &format!("/v1/pairing-sessions/{session}/events"),
            );
            send_state(&mut socket, state);
            let _ = socket.read();
            if session == "session-2" {
                let mut request =
                    BufReader::new(acceptor.accept(accept_loopback(&listener)).unwrap());
                assert_eq!(
                    read_request(&mut request),
                    "DELETE /v1/pairing-sessions/session-2 HTTP/1.1\r\n"
                );
                request
                    .get_mut()
                    .write_all(b"HTTP/1.1 204 No Content\r\nConnection: close\r\n\r\n")
                    .unwrap();
            }
        }
        stop.recv_timeout(Duration::from_secs(5)).unwrap();
        listener.set_nonblocking(true).unwrap();
        assert!(
            matches!(listener.accept(), Err(error) if error.kind() == std::io::ErrorKind::WouldBlock)
        );
    })
}
