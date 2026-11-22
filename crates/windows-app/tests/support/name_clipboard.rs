use slint::{
    ComponentHandle,
    platform::{Key, WindowEvent},
};
use windows_app::{NamePaste, PairingWindow};

pub(super) fn type_text(window: &PairingWindow, text: &str) {
    window
        .window()
        .dispatch_event(WindowEvent::KeyPressed { text: text.into() });
}

fn control(window: &PairingWindow, key: &str) {
    let surface = window.window();
    surface.dispatch_event(WindowEvent::KeyPressed {
        text: Key::Control.into(),
    });
    type_text(window, key);
    surface.dispatch_event(WindowEvent::KeyReleased {
        text: Key::Control.into(),
    });
}

pub(super) fn verify_paste(window: &PairingWindow) {
    window.on_read_name_paste(|| NamePaste {
        text: "办🚀e\u{301}".into(),
        ..Default::default()
    });
    window.set_sender_name("Draft".into());
    window.invoke_focus_name();
    window.window().dispatch_event(WindowEvent::KeyPressed {
        text: Key::End.into(),
    });
    control(window, "v");
    assert_eq!(window.get_sender_name(), "Draft办🚀e\u{301}");
    control(window, "z");
    assert_eq!(window.get_sender_name(), "Draft");

    control(window, "a");
    control(window, "v");
    assert_eq!(window.get_sender_name(), "办🚀e\u{301}");
    control(window, "z");
    assert_eq!(window.get_sender_name(), "");
    control(window, "z");
    assert_eq!(window.get_sender_name(), "Draft");
    control(window, "y");
    control(window, "y");
    assert_eq!(window.get_sender_name(), "办🚀e\u{301}");

    window.on_read_name_paste(|| NamePaste {
        error: "无法读取剪贴板，请重试。".into(),
        ..Default::default()
    });
    control(window, "v");
    assert_eq!(window.get_name_error(), "无法读取剪贴板，请重试。");
    assert_eq!(window.get_sender_name(), "办🚀e\u{301}");

    window.on_read_name_paste(|| NamePaste {
        error: "名称包含无效字符，请重新输入。".into(),
        invalid_text: true,
        ..Default::default()
    });
    window.set_sender_name("".into());
    window.window().dispatch_event(WindowEvent::KeyPressed {
        text: Key::Tab.into(),
    });
    assert!(!window.get_name_focused());
    window.invoke_paste_name();
    assert!(window.get_name_focused());
    assert_eq!(window.get_name_error(), "名称包含无效字符，请重新输入。");
}
