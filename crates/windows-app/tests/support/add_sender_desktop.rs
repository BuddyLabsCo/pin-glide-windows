use std::{
    fs,
    io::BufReader,
    path::{Path, PathBuf},
    process::Command,
    sync::mpsc,
    thread,
    time::{Duration, Instant},
};

use i_slint_core::animations::{current_tick, update_animations};
use serde_json::{Value, json};
use slint::{
    ComponentHandle, LogicalPosition, Model, ModelRc, Rgba8Pixel, SharedPixelBuffer, Timer,
    VecModel, Window, platform::WindowEvent,
};
use windows_app::{
    PairingStage, PairingWindow, service::DeliveryStatus, set_animation_preference_override, ui,
};

use super::{assert_redeem, capture, click, fixture::*, select_backend};

#[path = "dpapi.rs"]
pub(super) mod dpapi;

const CHILD: &str = "PIN_GLIDE_ADD_DESKTOP_CHILD";

#[test]
#[ignore = "requires an interactive Windows desktop"]
fn add_sender_uses_live_windows_and_preserves_receiver_state() {
    let Ok(mode) = std::env::var(CHILD) else {
        for mode in ["visible", "reduced", "hidden", "replace", "scroll"] {
            assert!(
                Command::new(std::env::current_exe().unwrap())
                    .args(["add_sender_desktop::add_sender_uses_live_windows_and_preserves_receiver_state", "--ignored", "--exact"])
                    .env(CHILD, mode)
                    .status()
                    .unwrap()
                    .success(),
                "{mode}"
            );
        }
        return;
    };
    select_backend();
    slint::spawn_local(async move {
        match mode.as_str() {
            "replace" => re_enroll().await,
            "scroll" => scroll(),
            _ => add_sender(&mode).await,
        }
        ui::close();
        slint::quit_event_loop().unwrap();
    })
    .unwrap();
    slint::run_event_loop_until_quit().unwrap();
}

async fn add_sender(mode: &str) {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("registration.json");
    let hidden = mode == "hidden";
    let reduced = mode == "reduced";
    set_animation_preference_override(Some(!reduced));
    let old_name = if hidden {
        "未命名设备"
    } else {
        "我的设备"
    };
    let original = registration(
        Some(dpapi::protect(RECEIVER_TOKEN.as_bytes())),
        &[(old_name, "original")],
    );
    fs::write(&path, original.to_string()).unwrap();
    let (listener, service) = listen();
    let (advance, release) = mpsc::channel();
    let (scan, qr) = mpsc::channel();
    let server = thread::spawn(move || super::add_sender_service::serve(listener, release, qr));

    let main = ui::open_registered(true, path.clone(), move || Some(service.clone()))
        .unwrap()
        .unwrap();
    wait_for(
        || ui::delivery_status() == DeliveryStatus::Ready,
        "original Receiver ready",
    )
    .await;
    click(main.window(), 200.0, 188.0);
    let window = ui::visible_pairing().unwrap();
    assert!(window.get_adding_device());
    assert!(!main.window().is_visible());
    assert_eq!(window.get_stage(), PairingStage::Preparing);
    assert_eq!(window.get_motion_enabled(), !reduced);
    if reduced {
        verify_static_loading(&window);
        evidence(window.window(), "add-preparing-reduced");
    }
    verify_refresh(&window, &advance, &path, &original).await;
    let code = reopen_add_sender(&window, &main, !reduced);
    advance.send(()).unwrap();
    stage(&window, PairingStage::Confirming).await;
    finish_add_sender(&window, &main, code, scan, advance, mode).await;
    let stored: Value = serde_json::from_slice(&fs::read(path).unwrap()).unwrap();
    assert_eq!(
        dpapi::unprotect(&stored["protected_receiver_token"]),
        RECEIVER_TOKEN.as_bytes()
    );
    assert_eq!(
        stored["protected_receiver_token"],
        original["protected_receiver_token"]
    );
    assert_eq!(
        stored["active_pairings"]["0"],
        original["active_pairings"]["0"]
    );
    assert_eq!(ui::delivery_status(), DeliveryStatus::Ready);
    tick().await;
    ui::close();
    server.join().unwrap();
}

fn reopen_add_sender(
    window: &PairingWindow,
    main: &windows_app::MainWindow,
    motion: bool,
) -> Vec<u8> {
    let code = qr_pixels(window);
    for preference in [!motion, motion] {
        click(window.window(), 40.0, 40.0);
        assert!(main.window().is_visible());
        assert!(ui::visible_pairing().is_none());
        set_animation_preference_override(Some(preference));
        click(main.window(), 200.0, 188.0);
        let reopened = ui::visible_pairing().unwrap();
        assert_eq!(reopened.get_motion_enabled(), preference);
        assert_eq!(qr_pixels(&reopened), code);
    }
    code
}

async fn finish_add_sender(
    window: &PairingWindow,
    main: &windows_app::MainWindow,
    code: Vec<u8>,
    scan: mpsc::Sender<Value>,
    advance: mpsc::Sender<()>,
    mode: &str,
) {
    let reduced = mode == "reduced";
    let hidden = mode == "hidden";
    assert_eq!(window.get_motion_enabled(), !reduced);
    if reduced {
        verify_static_loading(window);
        evidence(window.window(), "add-confirming-reduced");
    }
    if hidden {
        click(window.window(), 40.0, 40.0);
        assert!(main.window().is_visible());
    }
    let width = code.len().isqrt();
    scan.send(decode_pixels(width, width, &code)).unwrap();
    let fade_start = current_tick() + Duration::from_secs(2);
    if !hidden {
        update_animations(fade_start);
    }
    advance.send(()).unwrap();
    if hidden {
        verify_hidden_name(window, main).await;
    } else {
        verify_fade(window, fade_start, reduced).await;
        assert_eq!(window.get_sender_name(), "未命名设备");
        window.set_sender_name("我的设备".into());
        window.invoke_finish_naming();
        assert_eq!(window.get_name_error(), "已有一台同名设备，请换一个名称。");
        assert!(window.get_name_focused());
        window.set_sender_name("备用设备".into());
        window.invoke_finish_naming();
        assert!(main.window().is_visible());
        assert_eq!(sender_names(main), ["我的设备", "备用设备"]);
    }
}

fn sender_names(main: &windows_app::MainWindow) -> Vec<slint::SharedString> {
    main.get_sender_devices()
        .iter()
        .map(|row| row.name)
        .collect()
}

async fn verify_refresh(
    window: &PairingWindow,
    advance: &mpsc::Sender<()>,
    path: &Path,
    original: &Value,
) {
    advance.send(()).unwrap();
    stage(window, PairingStage::CreateFailed).await;
    assert_eq!(fs::read_to_string(path).unwrap(), original.to_string());
    window.invoke_refresh();
    stage(window, PairingStage::Expired).await;
    window.invoke_refresh();
    stage(window, PairingStage::Waiting).await;
    advance.send(()).unwrap();
    stage(window, PairingStage::Canceled).await;
    assert_eq!(fs::read_to_string(path).unwrap(), original.to_string());
    window.invoke_refresh();
    stage(window, PairingStage::Waiting).await;
}

async fn verify_hidden_name(window: &PairingWindow, main: &windows_app::MainWindow) {
    stage(window, PairingStage::Completed).await;
    assert!(ui::visible_pairing().is_none());
    let names = || sender_names(main);
    assert_eq!(names(), ["未命名设备", "未命名设备-2"]);
    main.hide().unwrap();
    ui::open();
    assert!(main.window().is_visible());
    assert_eq!(names(), ["未命名设备", "未命名设备-2"]);
}

async fn re_enroll() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("registration.json");
    fs::write(
        &path,
        registration(Some(vec![1]), &[("原有设备", "original")]).to_string(),
    )
    .unwrap();
    let (listener, service) = listen();
    let (scan, qr) = mpsc::channel::<Value>();
    let (second_scan, second_qr) = mpsc::channel::<()>();
    let server = thread::spawn(move || {
        let tls = acceptor();
        let session = "new-receiver";
        let initial = snapshot(session, "awaiting_authorization", 30);
        accept_creation(accept(&tls, &listener), &initial);
        let mut events = accept_events(accept(&tls, &listener), session);
        send_state(&mut events, &initial);
        let qr = qr.recv_timeout(Duration::from_secs(8)).unwrap();
        let claimed = claim(initial);
        send_state(&mut events, &claimed);
        sender_handshake(&mut events, &qr);
        let mut complete = accept_completion(accept(&tls, &listener), session);
        credential_ready(&mut events, complete.get_mut(), claimed);
        let mut redemption = BufReader::new(accept(&tls, &listener));
        assert_redeem(&mut redemption, session, 0);
        write_response(
            redemption.get_mut(),
            "200 OK",
            &json!({"receiverId":"receiver-1","receiverToken":RECEIVER_TOKEN}),
        );
        let mut delivery = accept_delivery(accept(&tls, &listener));
        second_qr.recv_timeout(Duration::from_secs(8)).unwrap();
        let initial = snapshot("replacement", "awaiting_authorization", 30);
        accept_creation(accept(&tls, &listener), &initial);
        let mut events = accept_events(accept(&tls, &listener), "replacement");
        send_state(&mut events, initial);
        assert_peer_left(&mut events);
        assert_delete(accept(&tls, &listener), "replacement");
        assert_peer_left(&mut delivery);
    });
    let main = ui::open_registered(true, path.clone(), move || Some(service.clone()))
        .unwrap()
        .unwrap();
    wait_for(
        || ui::delivery_status() == DeliveryStatus::Credential,
        "unreadable R in tray",
    )
    .await;
    click(main.window(), 200.0, 188.0);
    let window = ui::visible_pairing().unwrap();
    assert!(!window.get_adding_device());
    assert!(!main.window().is_visible());
    stage(&window, PairingStage::Waiting).await;
    let pixels = qr_pixels(&window);
    let width = pixels.len().isqrt();
    scan.send(decode_pixels(width, width, &pixels)).unwrap();
    stage(&window, PairingStage::Completed).await;
    wait_for(
        || ui::delivery_status() == DeliveryStatus::Ready,
        "tray follows re-enrolled Receiver",
    )
    .await;
    window.invoke_finish_naming();
    assert!(main.window().is_visible());
    let mut stored: Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
    stored["protected_receiver_token"] = json!([1]);
    fs::write(&path, stored.to_string()).unwrap();
    second_scan.send(()).unwrap();
    click(main.window(), 200.0, 188.0);
    let replacement = ui::visible_pairing().expect("Main opened first pairing for unreadable R");
    assert!(!replacement.get_adding_device());
    assert!(!main.window().is_visible());
    stage(&replacement, PairingStage::Waiting).await;
    ui::close();
    server.join().unwrap();
}

fn scroll() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("registration.json");
    let names = [
        ("我的设备", "1"),
        ("备用设备", "2"),
        ("工作设备", "3"),
        ("书房设备", "4"),
        ("旅行设备", "5"),
        ("家用设备", "6"),
        ("客厅设备", "7"),
    ];
    fs::write(&path, registration(None, &names).to_string()).unwrap();
    let main = ui::open_registered(true, path, || None).unwrap().unwrap();
    let window = main.window();
    let start = window.take_snapshot().unwrap();
    window.dispatch_event(WindowEvent::PointerScrolled {
        position: LogicalPosition::new(200.0, 470.0),
        delta_x: 0.0,
        delta_y: -300.0,
    });
    window.dispatch_event(WindowEvent::PointerExited);
    let wheel = window.take_snapshot().unwrap();
    click(window, 395.0, 292.0);
    let top = window.take_snapshot().unwrap();
    click(window, 395.0, 535.0);
    let track = window.take_snapshot().unwrap();
    evidence(window, "main-many-devices-list-scrolled");
    click(window, 395.0, 292.0);
    let last = main.get_sender_devices().row_data(6).unwrap();
    main.set_sender_devices(ModelRc::new(VecModel::from(vec![last])));
    let reference = window.take_snapshot().unwrap();
    let seventh_name = region(&reference, 70, 281, 70, 20);
    assert!(seventh_name.iter().any(|pixel| pixel.r < 100));
    assert!(
        region(&top, 24, 268, 352, 292) == region(&start, 24, 268, 352, 292),
        "clicking above the thumb returns to the first row"
    );
    assert!(region(&start, 70, 525, 70, 20) != seventh_name);
    for (input, end) in [("wheel", wheel), ("track", track)] {
        assert!(
            region(&end, 70, 525, 70, 20) == seventh_name,
            "seventh Sender's name after {input}"
        );
        assert!(
            region(&end, 20, 240, 356, 24) == region(&start, 20, 240, 356, 24),
            "the heading stays fixed"
        );
    }
}

pub(super) fn registration(token: Option<Vec<u8>>, senders: &[(&str, &str)]) -> Value {
    let active: serde_json::Map<_, _> = senders
        .iter()
        .enumerate()
        .map(|(index, (name, sender))| {
            (
                index.to_string(),
                json!({"sender_id":sender,"protected_key":dpapi::protect(&[21;32]),"name":name}),
            )
        })
        .collect();
    json!({"version":4,"receiver_id":"receiver-1","protected_receiver_token":token,
        "active_pairings":active,"provisional_pairing":null})
}

fn verify_static_loading(window: &PairingWindow) {
    let frame = window.window().take_snapshot().unwrap();
    update_animations(current_tick() + Duration::from_millis(300));
    let later = window.window().take_snapshot().unwrap();
    assert!(
        region(&frame, 132, 88, 176, 176) == region(&later, 132, 88, 176, 176),
        "the loading card stays static in {:?}",
        window.get_stage()
    );
}

async fn verify_fade(
    window: &PairingWindow,
    start: i_slint_core::animations::Instant,
    reduced: bool,
) {
    stage(window, PairingStage::Completed).await;
    let initial = window.window().take_snapshot().unwrap();
    assert_eq!(current_tick(), start);
    update_animations(start + Duration::from_millis(90));
    let halfway = window.window().take_snapshot().unwrap();
    update_animations(start + Duration::from_millis(180));
    let settled = window.window().take_snapshot().unwrap();
    assert_eq!(
        pixel(&settled, 220, 160),
        Rgba8Pixel {
            r: 227,
            g: 243,
            b: 236,
            a: 255
        }
    );
    assert_eq!(
        pixel(&settled, 172, 128),
        Rgba8Pixel {
            r: 255,
            g: 255,
            b: 255,
            a: 255
        }
    );
    let badge = pixel(&initial, 220, 160).r;
    if reduced {
        assert_eq!(
            region(&initial, 132, 88, 176, 176),
            region(&settled, 132, 88, 176, 176)
        );
        assert_eq!(
            region(&halfway, 132, 88, 176, 176),
            region(&settled, 132, 88, 176, 176)
        );
    } else {
        assert!(
            badge >= 252,
            "completion should start with the loading card"
        );
        assert!(
            pixel(&halfway, 220, 160).r.abs_diff(241) <= 3,
            "initial={badge}, midpoint={:?}",
            pixel(&halfway, 220, 160)
        );
    }
}

fn pixel(image: &SharedPixelBuffer<Rgba8Pixel>, x: usize, y: usize) -> Rgba8Pixel {
    let width = image.width() as usize;
    let scale = width as f32
        / if width * 560 == image.height() as usize * 440 {
            440.0
        } else {
            400.0
        };
    image.as_slice()[(y as f32 * scale) as usize * width + (x as f32 * scale) as usize]
}

fn region(
    image: &SharedPixelBuffer<Rgba8Pixel>,
    x: usize,
    y: usize,
    width: usize,
    height: usize,
) -> Vec<Rgba8Pixel> {
    (y..y + height)
        .flat_map(|row| (x..x + width).map(move |column| pixel(image, column, row)))
        .collect()
}

fn qr_pixels(window: &PairingWindow) -> Vec<u8> {
    window
        .get_qr()
        .to_rgb8()
        .unwrap()
        .as_slice()
        .iter()
        .map(|pixel| pixel.r)
        .collect()
}

async fn stage(window: &PairingWindow, stage: PairingStage) {
    wait_for(
        || window.get_stage() == stage,
        &format!("expected {stage:?}"),
    )
    .await;
}

async fn wait_for(mut condition: impl FnMut() -> bool, expectation: &str) {
    let limit = Instant::now() + Duration::from_secs(10);
    while !condition() {
        assert!(Instant::now() < limit, "{expectation}");
        tick().await;
    }
}

pub(super) async fn tick() {
    let (send, receive) = tokio::sync::oneshot::channel();
    Timer::single_shot(Duration::from_millis(5), move || {
        let _ = send.send(());
    });
    receive.await.unwrap();
}

fn evidence(window: &Window, name: &str) {
    if let Some(directory) = std::env::var_os("PIN_GLIDE_ADD_EVIDENCE").map(PathBuf::from) {
        capture(window, &Path::new(&directory).join(format!("{name}.ppm")));
    }
}
