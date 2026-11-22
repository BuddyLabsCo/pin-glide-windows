use slint::{
    ComponentHandle,
    platform::{Key, WindowEvent},
};
use windows_app::PairingWindow;
use windows_sys::Win32::{
    Foundation::GlobalFree,
    System::{
        DataExchange::{
            CloseClipboard, EmptyClipboard, EnumClipboardFormats, GetClipboardData, OpenClipboard,
            SetClipboardData,
        },
        Memory::{GMEM_MOVEABLE, GlobalAlloc, GlobalLock, GlobalUnlock},
        Ole::{CF_LOCALE, CF_OEMTEXT, CF_TEXT, CF_UNICODETEXT, OleDuplicateData},
    },
    UI::Input::KeyboardAndMouse::GetActiveWindow,
};

pub(super) fn type_text(window: &PairingWindow, text: &str) {
    window
        .window()
        .dispatch_event(WindowEvent::KeyPressed { text: text.into() });
}

fn paste(window: &PairingWindow) {
    let surface = window.window();
    surface.dispatch_event(WindowEvent::KeyPressed {
        text: Key::Control.into(),
    });
    type_text(window, "v");
    surface.dispatch_event(WindowEvent::KeyReleased {
        text: Key::Control.into(),
    });
}

pub(super) fn verify_paste(window: &PairingWindow) {
    // SAFETY: the fixture copies the original text handle before replacing the clipboard contents.
    unsafe {
        let _restore = ClipboardBackup::new();
        set_text(&[0x529e, 0xd83d, 0xde80, 0]);
        window.set_sender_name("Draft".into());
        window.invoke_focus_name();
        window.window().dispatch_event(WindowEvent::KeyPressed {
            text: Key::Control.into(),
        });
        type_text(window, "a");
        window.window().dispatch_event(WindowEvent::KeyReleased {
            text: Key::Control.into(),
        });
        paste(window);
        assert_eq!(window.get_sender_name(), "办🚀");

        assert_ne!(OpenClipboard(GetActiveWindow()), 0);
        paste(window);
        CloseClipboard();
        assert_eq!(window.get_name_error(), "无法读取剪贴板，请重试。");
        assert_eq!(window.get_sender_name(), "办🚀");

        set_text(&[0xd800, 0]);
        window.set_sender_name("".into());
        paste(window);
    }
    assert_eq!(window.get_name_error(), "名称包含无效字符，请重新输入。");
}

struct ClipboardBackup(Vec<(u32, *mut std::ffi::c_void)>);

impl ClipboardBackup {
    unsafe fn new() -> Self {
        // SAFETY: clipboard handles are copied while open, with an active test window as owner.
        unsafe {
            assert_ne!(OpenClipboard(GetActiveWindow()), 0);
            let mut format = EnumClipboardFormats(0);
            let mut saved = Vec::new();
            let mut supported = true;
            while format != 0 {
                if ![CF_TEXT, CF_OEMTEXT, CF_LOCALE].contains(&(format as u16)) {
                    if format < 0xc000 && format != u32::from(CF_UNICODETEXT) {
                        supported = false;
                        break;
                    }
                    let existing = GetClipboardData(format);
                    let copy = OleDuplicateData(existing, format as u16, GMEM_MOVEABLE);
                    if copy.is_null() {
                        supported = false;
                        break;
                    }
                    saved.push((format, copy));
                }
                format = EnumClipboardFormats(format);
            }
            assert_ne!(CloseClipboard(), 0);
            if !supported {
                for (_, handle) in saved {
                    GlobalFree(handle);
                }
                panic!(
                    "prepare an empty or plain-text clipboard for this desktop test (format {format})"
                );
            }
            Self(saved)
        }
    }
}

impl Drop for ClipboardBackup {
    fn drop(&mut self) {
        // SAFETY: the duplicate is ours until SetClipboardData transfers it to Windows.
        unsafe {
            let opened = OpenClipboard(GetActiveWindow()) != 0;
            let cleared = opened && EmptyClipboard() != 0;
            let mut restored = cleared;
            for (format, handle) in self.0.drain(..) {
                if !cleared || SetClipboardData(format, handle).is_null() {
                    GlobalFree(handle);
                    restored = false;
                }
            }
            if opened {
                CloseClipboard();
            }
            assert!(
                restored || std::thread::panicking(),
                "could not restore clipboard"
            );
        }
    }
}

unsafe fn set_text(units: &[u16]) {
    // SAFETY: the test allocates and initializes every UTF-16 unit before transferring ownership.
    unsafe {
        let owner = GetActiveWindow();
        assert!(!owner.is_null());
        assert_ne!(OpenClipboard(owner), 0);
        assert_ne!(EmptyClipboard(), 0);
        let allocation = GlobalAlloc(GMEM_MOVEABLE, size_of_val(units));
        assert!(!allocation.is_null());
        let data = GlobalLock(allocation).cast::<u16>();
        assert!(!data.is_null());
        data.copy_from_nonoverlapping(units.as_ptr(), units.len());
        GlobalUnlock(allocation);
        let transferred = SetClipboardData(u32::from(CF_UNICODETEXT), allocation);
        CloseClipboard();
        if transferred.is_null() {
            GlobalFree(allocation);
            panic!("could not install clipboard test data");
        }
    }
}
