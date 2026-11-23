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
        BI_RGB, BITMAPINFO, BitBlt, CAPTUREBLT, CreateCompatibleDC, CreateDIBSection,
        DIB_RGB_COLORS, DeleteDC, DeleteObject, GetDC, ReleaseDC, SRCCOPY, SelectObject,
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

pub(super) fn accessible_name(handle: usize, expected: &str) -> windows::core::Result<bool> {
    with_automation(|automation| Ok(named_element(automation, handle, expected, None)?.is_some()))
}

#[allow(dead_code)]
pub(super) fn live_region_is_polite(handle: usize, label: &str) -> bool {
    use windows::{
        Win32::UI::Accessibility::{IUIAutomationElement2, Polite},
        core::Interface,
    };

    with_automation(|automation| {
        let error = named_element(automation, handle, label, None)?.expect("visible error");
        let error: IUIAutomationElement2 = error.cast()?;
        // SAFETY: COM is initialized and the element belongs to the fixture.
        Ok(unsafe { error.CurrentLiveSetting()? == Polite })
    })
    .unwrap()
}

#[allow(dead_code)]
pub(super) fn toggle(window: usize, label: &str) {
    use windows::Win32::UI::Accessibility::{
        IUIAutomationTogglePattern, UIA_ButtonControlTypeId, UIA_TogglePatternId,
    };
    with_automation(|automation| {
        let element = named_element(automation, window, label, Some(UIA_ButtonControlTypeId))?
            .expect("visible startup switch");
        // SAFETY: the element is the fixture's live, accessible Switch.
        unsafe {
            element
                .GetCurrentPatternAs::<IUIAutomationTogglePattern>(UIA_TogglePatternId)?
                .Toggle()
        }
    })
    .unwrap();
}

#[allow(dead_code)]
pub(super) fn edit_value(window: usize, label: &str, value: Option<&str>) -> String {
    use windows::{
        Win32::UI::Accessibility::{
            IUIAutomationValuePattern, UIA_EditControlTypeId, UIA_ValuePatternId,
        },
        core::BSTR,
    };
    with_automation(|automation| {
        let element = named_element(automation, window, label, Some(UIA_EditControlTypeId))?
            .expect("visible input");
        // SAFETY: the live fixture element and its value pattern are used on the COM thread.
        unsafe {
            let input =
                element.GetCurrentPatternAs::<IUIAutomationValuePattern>(UIA_ValuePatternId)?;
            if let Some(value) = value {
                input.SetValue(&BSTR::from(value))?;
            }
            Ok(input.CurrentValue()?.to_string())
        }
    })
    .unwrap()
}

fn named_element(
    automation: &windows::Win32::UI::Accessibility::IUIAutomation,
    handle: usize,
    expected: &str,
    role: Option<windows::Win32::UI::Accessibility::UIA_CONTROLTYPE_ID>,
) -> windows::core::Result<Option<windows::Win32::UI::Accessibility::IUIAutomationElement>> {
    use windows::Win32::{Foundation::HWND, UI::Accessibility::TreeScope_Descendants};
    // SAFETY: COM is initialized, and the handle identifies the fixture window.
    unsafe {
        let root = automation.ElementFromHandle(HWND(handle as *mut _))?;
        let children = root.FindAll(TreeScope_Descendants, &automation.CreateTrueCondition()?)?;
        for index in 0..children.Length()? {
            let element = children.GetElement(index)?;
            if (role.is_none() || Some(element.CurrentControlType()?) == role)
                && !element.CurrentIsOffscreen()?.as_bool()
                && element.CurrentName()? == expected
            {
                return Ok(Some(element));
            }
        }
        Ok(None)
    }
}

fn with_automation<T>(
    query: impl FnOnce(&windows::Win32::UI::Accessibility::IUIAutomation) -> windows::core::Result<T>,
) -> windows::core::Result<T> {
    use windows::Win32::{
        System::Com::{
            CLSCTX_INPROC_SERVER, COINIT_MULTITHREADED, CoCreateInstance, CoInitializeEx,
            CoUninitialize,
        },
        UI::Accessibility::CUIAutomation,
    };
    // SAFETY: this fresh thread uses COM only between initialization and teardown.
    unsafe {
        CoInitializeEx(None, COINIT_MULTITHREADED).ok()?;
        let result = CoCreateInstance(&CUIAutomation, None, CLSCTX_INPROC_SERVER)
            .and_then(|automation| query(&automation));
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
    let size = super::window_size::client_size(window, WIDTH, HEIGHT).unwrap();
    capture_client(window, size, path);
}

pub(super) fn capture_client(window: HWND, (width, height): (i32, i32), path: &Path) {
    capture(window, (0, 0), (width, height), path);
}

pub(super) fn capture(
    window: HWND,
    (left, top): (i32, i32),
    (width, height): (i32, i32),
    path: &Path,
) {
    write_ppm(
        path,
        (width, height),
        &capture_rgb(window, (left, top), (width, height)),
    );
}

pub(super) fn capture_rgb(window: HWND, origin: (i32, i32), size: (i32, i32)) -> Vec<u8> {
    rgb(&capture_bgra(window, origin, size))
}

pub(super) fn rgb(pixels: &[[u8; 4]]) -> Vec<u8> {
    pixels
        .iter()
        .flat_map(|[blue, green, red, _]| [*red, *green, *blue])
        .collect()
}

pub(super) fn capture_bgra(
    window: HWND,
    (left, top): (i32, i32),
    (width, height): (i32, i32),
) -> Vec<[u8; 4]> {
    // SAFETY: the window is live and the DIB buffer stays allocated until its pixels are copied.
    unsafe {
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
        let copied = BitBlt(
            target,
            0,
            0,
            width,
            height,
            client,
            left,
            top,
            SRCCOPY | CAPTUREBLT,
        );
        let frame = std::slice::from_raw_parts(pixels.cast::<[u8; 4]>(), (width * height) as usize)
            .to_vec();
        SelectObject(target, previous);
        DeleteObject(image);
        DeleteDC(target);
        ReleaseDC(window, client);
        assert_ne!(copied, 0);
        frame
    }
}

pub(super) fn write_ppm(path: &Path, (width, height): (i32, i32), rgb: &[u8]) {
    let mut file = fs::File::create(path).unwrap();
    write!(file, "P6\n{width} {height}\n255\n").unwrap();
    file.write_all(rgb).unwrap();
}
