use std::{
    fs,
    path::{Path, PathBuf},
    sync::mpsc,
    thread,
    time::{Duration, Instant, SystemTime},
};

use aes_gcm::{Aes256Gcm, KeyInit, Nonce, aead::Aead};
use base64::{Engine, engine::general_purpose::STANDARD};
use receiver_core::delivery::{Arrival, Envelope, Inbox};
use serde_json::{Value, json};
use slint::{
    ComponentHandle, LogicalPosition, Model, Timer, Window,
    platform::{Key, WindowEvent},
};
use tokio_tungstenite::tungstenite::Message;
use windows_app::{MainWindow, NamePaste, service::DeliveryStatus, ui};

use super::{
    add_sender_desktop::{dpapi, registration},
    capture, click,
    fixture::*,
    select_backend,
};

const NAMES: [&str; 7] = [
    "我的设备",
    "备用设备",
    "工作设备",
    "书房设备",
    "旅行设备",
    "家用设备",
    "客厅设备",
];
const TIMEOUT: Duration = Duration::from_secs(5);

#[test]
#[ignore = "requires an interactive Windows desktop"]
fn edits_sender_rows_and_deletes_only_the_unreadable_pairing() {
    select_backend();
    slint::spawn_local(async {
        for (state, count, broken) in [
            ("no-phones", 0, false),
            ("ready", 1, false),
            ("two-phones", 2, false),
            ("many-devices", 7, false),
            ("one-key", 2, true),
        ] {
            exercise(state, count, broken).await;
            ui::close();
        }
        slint::quit_event_loop().unwrap();
    })
    .unwrap();
    slint::run_event_loop_until_quit().unwrap();
}

async fn exercise(state: &str, count: usize, broken: bool) {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("registration.json");
    let senders = NAMES[..count]
        .iter()
        .map(|&name| (name, name))
        .collect::<Vec<_>>();
    let mut stored = registration(Some(dpapi::protect(RECEIVER_TOKEN.as_bytes())), &senders);
    if broken {
        stored["active_pairings"]["0"]["protected_key"] = json!([1, 2]);
    }
    fs::write(&path, stored.to_string()).unwrap();
    let (listener, service) = listen();
    let (delete, deleted) = mpsc::channel();
    let (pong, ready) = mpsc::channel();
    let server = broken.then(|| {
        thread::spawn(move || {
            let mut socket = accept_delivery(accept(&acceptor(), &listener));
            for payload in [1, 2] {
                socket.send(Message::Ping(vec![payload].into())).unwrap();
                assert_eq!(socket.read().unwrap(), Message::Pong(vec![payload].into()));
                pong.send(()).unwrap();
                if payload == 1 {
                    deleted.recv_timeout(TIMEOUT).unwrap();
                }
            }
            socket
        })
    });
    let main = ui::open_registered(true, path.clone(), move || broken.then(|| service.clone()))
        .unwrap()
        .unwrap();
    let window = main.window();
    assert_eq!(main.get_sender_devices().row_count(), count);
    evidence(window, &format!("main-{state}"));
    if count == 0 {
        return;
    }
    if broken {
        wait_for_pong(&ready).await;
    }
    check_rows(&main, state, count, usize::from(broken));
    if state == "two-phones" {
        save_rename(&main, &path).await;
    }

    if broken {
        click(window, 200.0, 292.0);
        evidence(window, "main-one-key-delete-expanded");
        click(window, 332.0, 336.0);
        delete.send(()).unwrap();
        wait_for_pong(&ready).await;
        assert_eq!(ui::delivery_status(), DeliveryStatus::Ready);
        evidence(window, "main-one-key-after-delete");
        assert_eq!(main.get_sender_devices().row_count(), 1);
        assert_eq!(sender_name(&main, 0), NAMES[1]);
        let after: Value = serde_json::from_str(&fs::read_to_string(&path).unwrap()).unwrap();
        assert!(after["active_pairings"].get("0").is_none());
        assert_eq!(
            after["active_pairings"]["1"],
            stored["active_pairings"]["1"]
        );
        assert_eq!(
            after["protected_receiver_token"],
            stored["protected_receiver_token"]
        );
        let _socket = server.unwrap().join().unwrap();
    }
}

fn check_rows(main: &MainWindow, state: &str, count: usize, first_healthy: usize) {
    let window = main.window();
    if count == 7 {
        scroll(window, -400.0);
        evidence(window, "main-many-devices-list-scrolled");
        scroll(window, 400.0);
    }
    for index in first_healthy..count {
        open_row(window, index);
        evidence(window, &format!("main-{state}-edit-row-{}", index + 1));
        send_key(window, Key::Escape);
        scroll(window, 500.0);
    }

    let offset = first_healthy as f32 * 49.0;
    for (scenario, draft) in [
        ("empty", String::new()),
        ("too-long", "长".repeat(41)),
        ("invalid-character", String::new()),
        ("duplicate", NAMES[1 - first_healthy].into()),
    ] {
        if count == 1 && scenario == "duplicate" {
            continue;
        }
        open_row(window, first_healthy);
        replace_draft(window, &draft);
        if scenario == "too-long" {
            send_key(window, Key::Home);
        }
        if scenario == "invalid-character" {
            main.on_read_name_paste(|| NamePaste {
                invalid_text: true,
                ..Default::default()
            });
            control(window, "v");
        }
        click(window, 330.0, 388.0 + offset);
        evidence(window, &format!("main-{state}-edit-error-{scenario}"));
        assert_eq!(sender_name(main, first_healthy), NAMES[first_healthy]);
        assert!(shows_error(window, offset), "{state}: {scenario}");
        send_key(window, "可");
        assert!(!shows_error(window, offset), "editing clears the error");
        click(window, 268.0, 388.0 + offset);
    }
}

async fn save_rename(main: &MainWindow, path: &Path) {
    let window = main.window();
    open_row(window, 0);
    send_key(window, Key::Return);
    assert_eq!(
        sender_name(main, 0),
        NAMES[0],
        "the edited Sender may keep its name"
    );
    open_row(window, 0);
    replace_draft(window, "草稿");
    click(window, 200.0, 292.0);
    open_row(window, 0);
    replace_draft(window, "办公室手机");
    click(window, 330.0, 388.0);
    assert_eq!(sender_name(main, 0), "办公室手机");
    evidence(window, "main-two-phones-name-saved");
    let saved: Value = serde_json::from_str(&fs::read_to_string(path).unwrap()).unwrap();
    assert_eq!(saved["active_pairings"]["0"]["name"], "办公室手机");
    open_row(window, 0);
    replace_draft(window, NAMES[0]);
    send_key(window, Key::Return);
    sms_popup(&sender_name(main, 0)).await;
}

fn sender_name(main: &MainWindow, index: usize) -> slint::SharedString {
    let row = main.get_sender_devices().row_data(index).unwrap();
    row.name
}

async fn sms_popup(sender: &str) {
    let key = [21; 32];
    let nonce = [3; 12];
    let body = "【鹊信】这是一条测试短信，验证码 482913。请勿用于真实登录。";
    let plaintext = json!({"type":"verification_message", "smsText":body, "otp":"482913"});
    let ciphertext = Aes256Gcm::new((&key).into())
        .encrypt(&Nonce::from(nonce), plaintext.to_string().as_bytes())
        .unwrap();
    let sms = Inbox::default()
        .receive(
            Arrival {
                time: SystemTime::now(),
                order: 1,
            },
            Envelope {
                pairing_id: "0",
                crypto_version: 1,
                nonce: &STANDARD.encode(nonce),
                ciphertext: &STANDARD.encode(ciphertext),
            },
            |_| Some(zeroize::Zeroizing::new(key)),
        )
        .unwrap();
    let popup = ui::show_sms(sms, sender.into(), true)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(popup.get_sender_name(), sender);
    evidence(popup.window(), "sms-otp");
    popup.hide().unwrap();
}

fn open_row(window: &Window, index: usize) {
    if index >= 6 {
        scroll(window, -500.0);
    }
    let baseline = if index >= 6 {
        535.0
    } else {
        292.0 + 49.0 * index as f32
    };
    click(window, 104.0, baseline);
}

fn replace_draft(window: &Window, draft: &str) {
    control(window, "a");
    if draft.is_empty() {
        send_key(window, Key::Backspace);
    } else {
        send_key(window, draft);
    }
}

fn control(window: &Window, text: &str) {
    window.dispatch_event(WindowEvent::KeyPressed {
        text: Key::Control.into(),
    });
    send_key(window, text);
    window.dispatch_event(WindowEvent::KeyReleased {
        text: Key::Control.into(),
    });
}

fn send_key(window: &Window, text: impl Into<slint::SharedString>) {
    let text: slint::SharedString = text.into();
    for event in [
        WindowEvent::KeyPressed { text: text.clone() },
        WindowEvent::KeyReleased { text },
    ] {
        window.dispatch_event(event);
    }
}

fn scroll(window: &Window, delta_y: f32) {
    window.dispatch_event(WindowEvent::PointerScrolled {
        position: LogicalPosition::new(200.0, 470.0),
        delta_x: 0.0,
        delta_y,
    });
    window.dispatch_event(WindowEvent::PointerExited);
}

async fn wait_for_pong(ready: &mpsc::Receiver<()>) {
    let start = Instant::now();
    loop {
        match ready.try_recv() {
            Ok(()) => return,
            Err(mpsc::TryRecvError::Disconnected) => panic!("delivery socket closed"),
            Err(mpsc::TryRecvError::Empty) => {}
        }
        assert!(
            start.elapsed() < TIMEOUT,
            "delivery socket stopped responding"
        );
        let (send, receive) = tokio::sync::oneshot::channel();
        Timer::single_shot(Duration::from_millis(5), move || {
            let _ = send.send(());
        });
        receive.await.unwrap();
    }
}

fn shows_error(window: &Window, offset: f32) -> bool {
    let pixels = window.take_snapshot().unwrap();
    let scale = pixels.width() as f32 / 400.0;
    let left = (40.0 * scale).round() as usize;
    let top = ((372.0 + offset) * scale).round() as usize;
    let width = pixels.width() as usize;
    (top..top + (20.0 * scale).round() as usize).any(|row| {
        pixels.as_slice()[row * width + left..row * width + left + (280.0 * scale) as usize]
            .iter()
            .any(|pixel| pixel.r.saturating_sub(pixel.g) > 50 && pixel.g < 100)
    })
}

fn evidence(window: &Window, state: &str) {
    if let Some(directory) = std::env::var_os("PIN_GLIDE_MAIN_EVIDENCE").map(PathBuf::from) {
        capture(window, &directory.join(format!("{state}.ppm")));
    }
}
