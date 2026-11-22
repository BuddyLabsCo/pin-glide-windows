use std::{
    fs,
    io::Write,
    path::Path,
    ptr,
    sync::mpsc,
    thread,
    time::{Duration, Instant},
};

use serde_json::json;
use windows_sys::Win32::{
    Foundation::{HWND, LPARAM},
    Graphics::Gdi::{
        BI_RGB, BITMAPINFO, BitBlt, CreateCompatibleDC, CreateDIBSection, DIB_RGB_COLORS, DeleteDC,
        DeleteObject, GetDC, ReleaseDC, SRCCOPY, SelectObject,
    },
    UI::WindowsAndMessaging::{EnumWindows, GetWindowThreadProcessId, IsWindowVisible},
};

const WIDTH: i32 = 400;
const HEIGHT: i32 = 576;

pub(super) fn seed_sender_names(path: &Path, duplicate: bool, hidden: bool) {
    let names = if duplicate {
        json!({
            "previous": {"sender_id": "previous", "protected_key": [], "name": "我的设备"}
        })
    } else if hidden {
        json!({
            "previous": {"sender_id": "previous", "protected_key": [], "name": "未命名设备"},
            "spare": {"sender_id": "spare", "protected_key": [], "name": "未命名设备-2"}
        })
    } else {
        return;
    };
    fs::write(
        path,
        json!({
            "version": 4,
            "receiver_id": "receiver-1",
            "active_pairings": names
        })
        .to_string(),
    )
    .unwrap();
}

pub(super) fn main_window() -> Option<HWND> {
    let mut main: HWND = ptr::null_mut();
    // SAFETY: the output pointer is valid for the synchronous enumeration.
    unsafe {
        EnumWindows(Some(find_main), (&raw mut main) as LPARAM);
    }
    (!main.is_null()).then_some(main)
}

pub(super) fn check_main(
    opened_at: Instant,
    label: &mut Option<mpsc::Receiver<bool>>,
    expected: &str,
    evidence: Option<&Path>,
) {
    if let Some(label) = label {
        match label.try_recv() {
            Ok(found) => {
                assert!(found, "Main exposes the saved Sender label");
                slint::quit_event_loop().unwrap();
            }
            Err(mpsc::TryRecvError::Empty) => {}
            Err(error) => panic!("{error}"),
        }
    } else if opened_at.elapsed() >= Duration::from_millis(250) {
        *label = Some(verify_main(expected, evidence));
    }
}

fn verify_main(expected: &str, evidence: Option<&Path>) -> mpsc::Receiver<bool> {
    let main = main_window().expect("a visible 400 × 576 Main window");
    if let Some(directory) = evidence {
        capture_main(main, &directory.join("main-ready.ppm"));
    }
    let expected = expected.to_owned();
    let handle = main as usize;
    let (send, result) = mpsc::channel();
    thread::spawn(move || {
        let found = accessible_name(handle, &expected).unwrap();
        send.send(found).unwrap();
    });
    result
}

fn accessible_name(handle: usize, expected: &str) -> windows::core::Result<bool> {
    use windows::Win32::{
        Foundation::HWND,
        System::Com::{
            CLSCTX_INPROC_SERVER, COINIT_MULTITHREADED, CoCreateInstance, CoInitializeEx,
            CoUninitialize,
        },
        UI::Accessibility::{CUIAutomation, IUIAutomation, TreeScope_Descendants},
    };
    // SAFETY: this fresh thread uses COM only between initialization and teardown.
    unsafe {
        CoInitializeEx(None, COINIT_MULTITHREADED).ok()?;
        let result = (|| {
            let automation: IUIAutomation =
                CoCreateInstance(&CUIAutomation, None, CLSCTX_INPROC_SERVER)?;
            let root = automation.ElementFromHandle(HWND(handle as *mut _))?;
            let children =
                root.FindAll(TreeScope_Descendants, &automation.CreateTrueCondition()?)?;
            for index in 0..children.Length()? {
                let element = children.GetElement(index)?;
                if !element.CurrentIsOffscreen()?.as_bool() && element.CurrentName()? == expected {
                    return Ok(true);
                }
            }
            Ok(false)
        })();
        CoUninitialize();
        result
    }
}

unsafe extern "system" fn find_main(window: HWND, output: LPARAM) -> i32 {
    // SAFETY: this is the current enumeration entry and the caller supplies a writable HWND.
    unsafe {
        let mut process = 0;
        GetWindowThreadProcessId(window, &mut process);
        if process == std::process::id()
            && IsWindowVisible(window) != 0
            && super::window_size::client_size(window, WIDTH, HEIGHT).is_some()
        {
            *(output as *mut HWND) = window;
            return 0;
        }
    }
    1
}

fn capture_main(window: HWND, path: &Path) {
    let (width, height) = super::window_size::client_size(window, WIDTH, HEIGHT).unwrap();
    // SAFETY: the window is live and the DIB buffer stays allocated until its pixels are copied.
    let rgb = unsafe {
        let client = GetDC(window);
        assert!(!client.is_null());
        let target = CreateCompatibleDC(client);
        assert!(!target.is_null());
        let mut info = BITMAPINFO::default();
        info.bmiHeader.biSize = size_of_val(&info.bmiHeader) as u32;
        info.bmiHeader.biWidth = width;
        info.bmiHeader.biHeight = -height;
        info.bmiHeader.biPlanes = 1;
        info.bmiHeader.biBitCount = 32;
        info.bmiHeader.biCompression = BI_RGB;
        let mut pixels = ptr::null_mut();
        let image = CreateDIBSection(
            client,
            &raw const info,
            DIB_RGB_COLORS,
            &raw mut pixels,
            ptr::null_mut(),
            0,
        );
        assert!(!image.is_null() && !pixels.is_null());
        let previous = SelectObject(target, image);
        let copied = BitBlt(target, 0, 0, width, height, client, 0, 0, SRCCOPY);
        let rgb = std::slice::from_raw_parts(pixels.cast::<u8>(), (width * height * 4) as usize)
            .as_chunks::<4>()
            .0
            .iter()
            .flat_map(|pixel| [pixel[2], pixel[1], pixel[0]])
            .collect::<Vec<_>>();
        SelectObject(target, previous);
        DeleteObject(image);
        DeleteDC(target);
        ReleaseDC(window, client);
        assert_ne!(copied, 0);
        rgb
    };
    let mut file = fs::File::create(path).unwrap();
    write!(file, "P6\n{width} {height}\n255\n").unwrap();
    file.write_all(&rgb).unwrap();
}
