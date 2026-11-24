use std::{io::BufReader, net::TcpListener, sync::mpsc, time::Duration};

use i_slint_core::animations::{Instant as AnimationTime, current_tick, update_animations};
use serde_json::{Value, json};
use slint::{
    ComponentHandle, LogicalPosition, Window,
    platform::{Key, PointerEventButton, WindowEvent},
};
use windows_app::{PairingStage, PairingWindow, set_animation_preference_override, ui};

use super::{evidence, pixel, region, stage};
use crate::{assert_redeem, fixture::*};

pub(super) fn verify_pair_button(window: &Window) {
    let center = LogicalPosition::new(200.0, 188.0);
    let outside = LogicalPosition::new(200.0, 236.0);
    let sample = || pixel(&window.take_snapshot().unwrap(), 52, 180);
    let rest = sample();
    evidence(window, "main-local-r");
    window.dispatch_event(WindowEvent::PointerMoved { position: center });
    let hover = sample();
    assert_ne!(rest, hover);
    evidence(window, "pair-button-hover");
    window.dispatch_event(WindowEvent::PointerPressed {
        position: center,
        button: PointerEventButton::Left,
    });
    assert_eq!(sample(), hover);
    evidence(window, "pair-button-pressed");
    window.dispatch_event(WindowEvent::PointerMoved { position: outside });
    window.dispatch_event(WindowEvent::PointerReleased {
        position: outside,
        button: PointerEventButton::Left,
    });
    window.dispatch_event(WindowEvent::PointerExited);
    for _ in 0..3 {
        window.dispatch_event(WindowEvent::KeyPressed {
            text: Key::Tab.into(),
        });
        window.dispatch_event(WindowEvent::KeyReleased {
            text: Key::Tab.into(),
        });
        let focus = pixel(&window.take_snapshot().unwrap(), 200, 165);
        if (focus.r, focus.g, focus.b) == (40, 78, 121) {
            evidence(window, "pair-button-focus");
            return;
        }
    }
    panic!("the keyboard did not focus Pair a new device");
}

pub(super) fn verify_preparation(window: &PairingWindow) {
    evidence(window.window(), "pair-preparing");
    set_animation_preference_override(Some(true));
    ui::update_motion();
    let cycle = (current_tick().as_millis() / 1800 + 1) * 1800;
    update_animations(AnimationTime::from_millis(cycle + 100));
    let first = window.window().take_snapshot().unwrap();
    evidence(window.window(), "pair-pulse-100");
    update_animations(AnimationTime::from_millis(cycle + 500));
    let later = window.window().take_snapshot().unwrap();
    assert!(
        region(&first, 132, 88, 176, 176) != region(&later, 132, 88, 176, 176),
        "the QR loading card should change during its pulse"
    );
    evidence(window.window(), "pair-pulse-500");
    set_animation_preference_override(Some(false));
    ui::update_motion();
}

pub(super) async fn capture_waiting(window: &PairingWindow) {
    stage(window, PairingStage::Waiting).await;
    assert_eq!(window.get_remaining(), "04:32");
    evidence(window.window(), "pair-waiting-live");
}

pub(super) fn issue_replacement(
    tls: &native_tls::TlsAcceptor,
    listener: &TcpListener,
    session: &str,
    lifetime: u64,
    scan: &mpsc::Receiver<Value>,
) {
    let initial = snapshot(session, "awaiting_authorization", lifetime);
    accept_creation(accept(tls, listener), &initial);
    let mut events = accept_events(accept(tls, listener), session);
    send_state(&mut events, &initial);
    let qr = scan.recv_timeout(Duration::from_secs(8)).unwrap();
    let claimed = claim(initial);
    send_state(&mut events, &claimed);
    sender_handshake(&mut events, &qr);
    let mut complete = accept_completion(accept(tls, listener), session);
    credential_ready(&mut events, complete.get_mut(), claimed);
    let mut redemption = BufReader::new(accept(tls, listener));
    assert_redeem(&mut redemption, session, 0);
    write_response(
        redemption.get_mut(),
        "200 OK",
        &json!({"receiverId":"receiver-1","receiverToken":RECEIVER_TOKEN}),
    );
}

pub(super) fn reject_first_upgrade(connection: Connection) {
    let mut connection = BufReader::new(connection);
    let (request, headers, _) = read_request_parts(&mut connection);
    assert_eq!(request, "GET /v1/receiver/events HTTP/1.1\r\n");
    assert!(headers.iter().any(|header| {
        header
            .trim()
            .eq_ignore_ascii_case(&format!("Authorization: Bearer {RECEIVER_TOKEN}"))
    }));
    write_response(
        connection.get_mut(),
        "401 Unauthorized",
        &json!({"code":"credential_invalid"}),
    );
}
