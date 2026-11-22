use std::{fs, io::Write, path::Path, ptr};

use serde_json::json;
use windows_sys::Win32::{
    Foundation::{HWND, LPARAM, RECT},
    Graphics::Gdi::{
        BI_RGB, BITMAPINFO, BitBlt, CreateCompatibleDC, CreateDIBSection, DIB_RGB_COLORS, DeleteDC,
        DeleteObject, GetDC, ReleaseDC, SRCCOPY, SelectObject,
    },
    UI::WindowsAndMessaging::{
        EnumWindows, GetClientRect, GetWindowThreadProcessId, IsWindowVisible,
    },
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

pub(super) fn verify_main(evidence: Option<&Path>) {
    let mut main: HWND = ptr::null_mut();
    // SAFETY: the output pointer is valid for the synchronous enumeration.
    unsafe {
        EnumWindows(Some(find_main), (&raw mut main) as LPARAM);
    }
    assert!(!main.is_null(), "a visible 400 × 576 Main window");
    if let Some(directory) = evidence {
        capture_main(main, &directory.join("main-ready.ppm"));
    }
}

unsafe extern "system" fn find_main(window: HWND, output: LPARAM) -> i32 {
    // SAFETY: this is the current enumeration entry and the caller supplies a writable HWND.
    unsafe {
        let mut process = 0;
        GetWindowThreadProcessId(window, &mut process);
        if process == std::process::id() && IsWindowVisible(window) != 0 {
            let mut client = RECT::default();
            if GetClientRect(window, &raw mut client) != 0
                && (client.right, client.bottom) == (WIDTH, HEIGHT)
            {
                *(output as *mut HWND) = window;
                return 0;
            }
        }
    }
    1
}

fn capture_main(window: HWND, path: &Path) {
    // SAFETY: the window is live and the DIB buffer stays allocated until its pixels are copied.
    let rgb = unsafe {
        let client = GetDC(window);
        assert!(!client.is_null());
        let target = CreateCompatibleDC(client);
        assert!(!target.is_null());
        let mut info = BITMAPINFO::default();
        info.bmiHeader.biSize = size_of_val(&info.bmiHeader) as u32;
        info.bmiHeader.biWidth = WIDTH;
        info.bmiHeader.biHeight = -HEIGHT;
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
        let copied = BitBlt(target, 0, 0, WIDTH, HEIGHT, client, 0, 0, SRCCOPY);
        let rgb = std::slice::from_raw_parts(pixels.cast::<u8>(), (WIDTH * HEIGHT * 4) as usize)
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
    write!(file, "P6\n{WIDTH} {HEIGHT}\n255\n").unwrap();
    file.write_all(&rgb).unwrap();
}
