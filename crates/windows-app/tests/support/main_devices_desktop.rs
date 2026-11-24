use std::{
    fs,
    path::{Path, PathBuf},
    sync::mpsc,
    thread,
    time::{Duration, Instant},
};

use i_slint_core::{
    accessibility::AccessibleStringProperty, item_tree::ParentItemTraversalMode, items::TextInput,
    window::WindowInner,
};
use serde_json::{Value, json};
use slint::{
    ComponentHandle, LogicalPosition, Model, Window,
    platform::{Key, WindowEvent},
};
use tokio_tungstenite::tungstenite::Message;
use windows_app::{MainWindow, NamePaste, service::DeliveryStatus, ui};
use windows_sys::Win32::{
    Foundation::{HWND, LPARAM, RECT},
    UI::WindowsAndMessaging::{
        EnumWindows, GetClientRect, GetWindowTextW, GetWindowThreadProcessId, IsWindowVisible,
    },
};

use super::{
    add_sender_desktop::{dpapi, registration, tick},
    capture, click,
    fixture::*,
    main_capture::{accessible_name, capture_client},
    name_clipboard::with_modifier,
    select_backend,
};

#[path = "recovery_desktop.rs"]
mod recovery_desktop;

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
const RENAMED: &str = "办公室手机";

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
            ("one-key-pointer", 2, true),
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
    let rename = state == "two-phones";
    let (advance, advanced) = mpsc::channel();
    let (pong, ready) = mpsc::channel();
    let server = (broken || rename).then(|| {
        thread::spawn(move || {
            let mut socket = accept_delivery(accept(&acceptor(), &listener));
            verify_connection(&mut socket, 1, &pong);
            advanced.recv_timeout(TIMEOUT).unwrap();
            if broken {
                verify_connection(&mut socket, 2, &pong);
            } else {
                send_rename_sms(&mut socket);
            }
            socket
        })
    });
    let main = ui::open_registered(true, path.clone(), move || {
        (broken || rename).then(|| service.clone())
    })
    .unwrap()
    .unwrap();
    let window = main.window();
    assert_eq!(main.get_sender_devices().row_count(), count);
    evidence(window, &format!("main-{state}"));
    if count == 0 {
        return;
    }
    if server.is_some() {
        wait_for_pong(&ready).await;
    }
    check_rows(&main, state, count, usize::from(broken));
    assert_eq!(fs::read_to_string(&path).unwrap(), stored.to_string());
    if rename {
        save_rename(&main, &path);
        advance.send(()).unwrap();
        check_delivered_popup().await;
    }

    if broken {
        let pointer = state == "one-key-pointer";
        if pointer {
            open_row(window, 0);
            click(window, 332.0, 336.0);
        } else {
            delete_with_keyboard(window);
        }
        advance.send(()).unwrap();
        assert_eq!(main.get_sender_devices().row_count(), 1);
        assert_eq!(
            main.get_sender_devices().row_data(0).unwrap().name,
            NAMES[1]
        );
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
        tick().await;
        assert_focused_header(window, "编辑名称 备用设备", "false");
        assert_eq!(
            window_pixel(window, 26.0, 292.0).r < 100,
            !pointer,
            "only keyboard deletion should show a focus outline on the surviving row",
        );
        evidence(window, &format!("main-{state}-after-delete"));
        send_key(window, Key::Return);
        assert_editor(window, NAMES[1], "");
        wait_for_pong(&ready).await;
        assert_eq!(ui::delivery_status(), DeliveryStatus::Ready);
    }
    if let Some(server) = server {
        let _socket = server.join().unwrap();
    }
}

fn check_rows(main: &MainWindow, state: &str, count: usize, first_healthy: usize) {
    let window = main.window();
    if count == 7 {
        scroll(window, -400.0);
        evidence(window, "main-many-devices-list-scrolled");
        scroll(window, 400.0);
    }
    for (index, name) in NAMES.iter().enumerate().take(count).skip(first_healthy) {
        open_row(window, index);
        assert_editor(window, name, "");
        evidence(window, &format!("main-{state}-edit-row-{}", index + 1));
        send_key(window, Key::Escape);
        assert_focused_header(window, &format!("编辑名称 {name}"), "false");
        scroll(window, 500.0);
    }

    let offset = first_healthy as f32 * 49.0;
    for (scenario, draft) in [
        ("empty", String::new()),
        ("too-long", "长".repeat(41)),
        ("invalid-character", "有效草稿".into()),
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
            with_modifier(window, Key::Control, || send_key(window, "v"));
        }
        click(window, 330.0, 388.0 + offset);
        evidence(window, &format!("main-{state}-edit-error-{scenario}"));
        let row = main.get_sender_devices().row_data(first_healthy).unwrap();
        assert_eq!(row.name, NAMES[first_healthy]);
        assert!(shows_error(window, offset), "{state}: {scenario}");
        if scenario == "invalid-character" {
            assert_editor(window, &draft, "名称包含无效字符，请重新输入。");
        }
        send_key(window, "可");
        assert!(!shows_error(window, offset), "editing clears the error");
        click(window, 268.0, 388.0 + offset);
    }
}

fn save_rename(main: &MainWindow, path: &Path) {
    let window = main.window();
    open_row(window, 0);
    send_key(window, Key::Return);
    assert_eq!(
        main.get_sender_devices().row_data(0).unwrap().name,
        NAMES[0],
        "the edited Sender may keep its name"
    );
    open_row(window, 0);
    replace_draft(window, "草稿");
    click(window, 200.0, 292.0);
    open_row(window, 0);
    replace_draft(window, RENAMED);
    click(window, 330.0, 388.0);
    assert_eq!(main.get_sender_devices().row_data(0).unwrap().name, RENAMED);
    evidence(window, "main-two-phones-name-saved");
    let saved: Value = serde_json::from_str(&fs::read_to_string(path).unwrap()).unwrap();
    assert_eq!(saved["active_pairings"]["0"]["name"], RENAMED);
}

fn verify_connection(socket: &mut Socket, payload: u8, pong: &mpsc::Sender<()>) {
    socket.send(Message::Ping(vec![payload].into())).unwrap();
    assert_eq!(socket.read().unwrap(), Message::Pong(vec![payload].into()));
    pong.send(()).unwrap();
}

fn send_rename_sms(socket: &mut Socket) {
    send_authenticated_delivery(
        socket,
        "0",
        NAMES[0],
        &[21; 32],
        ("after-rename", 3),
        json!({
            "type":"verification_message",
            "smsText":"【鹊信】这是一条测试短信，验证码 482913。请勿用于真实登录。",
            "otp":"482913",
        }),
    );
    let receipt = socket.read().unwrap();
    assert_eq!(
        serde_json::from_str::<Value>(receipt.to_text().unwrap()).unwrap(),
        json!({"type":"receipt", "version":1, "senderId":NAMES[0], "messageId":"after-rename"}),
    );
}

async fn check_delivered_popup() {
    let started = Instant::now();
    loop {
        if let Some(popup) = visible_sms() {
            let handle = popup as usize;
            let (send, receive) = tokio::sync::oneshot::channel();
            thread::spawn(move || send.send(accessible_name(handle, RENAMED)).unwrap());
            let shown = receive.await.unwrap().unwrap();
            if shown {
                if let Some(path) = evidence_path("sms-renamed-sender") {
                    let mut rect = RECT::default();
                    // SAFETY: the popup is live and the output rectangle is writable.
                    assert_ne!(unsafe { GetClientRect(popup, &raw mut rect) }, 0);
                    capture_client(popup, (rect.right, rect.bottom), &path);
                }
                return;
            }
        }
        assert!(
            started.elapsed() < TIMEOUT,
            "SMS popup did not display {RENAMED}"
        );
        tick().await;
    }
}

pub(super) fn visible_sms() -> Option<HWND> {
    let mut popup: HWND = std::ptr::null_mut();
    #[expect(
        clippy::undocumented_unsafe_blocks,
        reason = "the callback only uses the pointer during enumeration"
    )]
    unsafe {
        EnumWindows(Some(find_sms), (&raw mut popup) as LPARAM)
    };
    (!popup.is_null()).then_some(popup)
}

unsafe extern "system" fn find_sms(window: HWND, output: LPARAM) -> i32 {
    let mut process = 0;
    let mut title = [0; 64];
    // SAFETY: Windows supplies the handle and the enumerator supplies the output pointer.
    unsafe {
        GetWindowThreadProcessId(window, &raw mut process);
        if process == std::process::id() && IsWindowVisible(window) != 0 {
            let size = GetWindowTextW(window, title.as_mut_ptr(), title.len() as i32);
            let title = String::from_utf16_lossy(&title[..size as usize]);
            if matches!(title.as_str(), "鹊信 · 新短信" | "Pin Glide · New SMS") {
                *(output as *mut HWND) = window;
                return 0;
            }
        }
    }
    1
}

fn assert_editor(window: &Window, name: &str, error: &str) {
    let focus = WindowInner::from_pub(window)
        .focus_item
        .borrow()
        .upgrade()
        .expect("the row or its editor should hold focus");
    assert!(
        focus.downcast::<TextInput>().is_some(),
        "expected focus on the draft {name:?}"
    );
    let value = focus.accessible_string_property(AccessibleStringProperty::Value);
    let description = focus.accessible_string_property(AccessibleStringProperty::Description);
    assert_eq!(value.as_deref(), Some(name));
    assert_eq!(description.as_deref(), Some(error));
}

fn assert_focused_header(window: &Window, label: &str, expanded: &str) {
    let header = WindowInner::from_pub(window)
        .focus_item
        .borrow()
        .upgrade()
        .and_then(|focus| focus.parent_item(ParentItemTraversalMode::FindAllParents))
        .expect("the Sender header should hold focus");
    assert_eq!(
        header
            .accessible_string_property(AccessibleStringProperty::Label)
            .as_deref(),
        Some(label),
        "focus should be on the expected Sender row",
    );
    assert_eq!(
        header
            .accessible_string_property(AccessibleStringProperty::Expanded)
            .as_deref(),
        Some(expanded),
        "unexpected expansion state for {label}",
    );
}

fn delete_with_keyboard(window: &Window) {
    open_row(window, 0);
    assert_focused_header(window, "收起 我的设备", "true");
    window.dispatch_event(WindowEvent::PointerMoved {
        position: LogicalPosition::new(200.0, 292.0),
    });
    let hovered = window_pixel(window, 200.0, 311.0);
    window.dispatch_event(WindowEvent::PointerMoved {
        position: LogicalPosition::new(332.0, 336.0),
    });
    assert_focused_header(window, "收起 我的设备", "true");
    assert!(
        window_pixel(window, 200.0, 311.0).r > hovered.r,
        "expanded header retains its hover tint"
    );
    window.dispatch_event(WindowEvent::PointerExited);
    evidence(window, "main-one-key-delete-expanded");
    send_key(window, Key::Tab);
    let inner = WindowInner::from_pub(window);
    let focused = inner.focus_item.borrow().upgrade().unwrap();
    let description = focused
        .accessible_string_property(AccessibleStringProperty::Label)
        .or_else(|| {
            focused
                .parent_item(ParentItemTraversalMode::FindAllParents)?
                .accessible_string_property(AccessibleStringProperty::Label)
        });
    assert_eq!(
        description.as_deref(),
        Some("删除"),
        "Tab should move to Delete"
    );
    with_modifier(window, Key::Shift, || send_key(window, Key::Tab));
    assert_focused_header(window, "收起 我的设备", "true");
    send_key(window, Key::Tab);
    send_key(window, Key::Return);
}

fn window_pixel(window: &Window, x: f32, y: f32) -> slint::Rgba8Pixel {
    let image = window.take_snapshot().unwrap();
    let width = image.width() as usize;
    let scale = width as f32 / 400.0;
    image.as_slice()[(y * scale) as usize * width + (x * scale) as usize]
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
    with_modifier(window, Key::Control, || send_key(window, "a"));
    if draft.is_empty() {
        send_key(window, Key::Backspace);
    } else {
        send_key(window, draft);
    }
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
        tick().await;
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
    if let Some(path) = evidence_path(state) {
        capture(window, &path);
    }
}

fn evidence_path(state: &str) -> Option<PathBuf> {
    let directory = PathBuf::from(std::env::var_os("PIN_GLIDE_MAIN_EVIDENCE")?);
    Some(directory.join(format!("{state}.ppm")))
}
