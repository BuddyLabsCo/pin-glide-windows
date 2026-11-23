use std::{
    path::{Path, PathBuf},
    thread,
    time::{Duration, Instant},
};

use windows_sys::Win32::{
    Foundation::{HWND, RECT},
    UI::{HiDpi::GetDpiForWindow, WindowsAndMessaging::GetClientRect},
};

use super::main_capture::{capture_bgra, capture_client, capture_rgb, rgb, write_ppm};

pub(super) fn capture_immediate(window: HWND, state: &str) {
    if let Some(path) = evidence_path(state) {
        capture_client(window, client_size(window), &path);
    }
}

pub(super) fn first_switch_frame(
    window: HWND,
    enabled: bool,
    state: &str,
    activate: impl FnOnce() + Send,
) -> Vec<u8> {
    let rail = if enabled { [0, 97, 181] } else { [250; 3] };
    let start = switch_pixels(window);
    let ((left, top), (width, _)) = switch_bounds(window);
    let center = (physical(window, 365) - left) as usize;
    let evidence = evidence_path(state);
    assert_ne!(
        start[center * 3..(center + 1) * 3],
        rail,
        "switch must start in the opposite state"
    );
    let (origin, size, offset) = if evidence.is_some() {
        let size = client_size(window);
        ((0, 0), size, (top * size.0 + left) as usize)
    } else {
        ((left, top), (width, 1), 0)
    };
    thread::scope(|scope| {
        scope.spawn(activate);
        let deadline = Instant::now() + Duration::from_secs(3);
        loop {
            let frame = capture_bgra(window, origin, size);
            let scan = &frame[offset..offset + width as usize];
            let [blue, green, red, _] = scan[center];
            if [red, green, blue] == rail {
                if let Some(path) = &evidence {
                    write_ppm(path, size, &rgb(&frame));
                }
                return rgb(scan);
            }
            assert!(
                Instant::now() < deadline,
                "the switch never rendered {state}"
            );
            thread::sleep(Duration::from_millis(1));
        }
    })
}

pub(super) fn switch_pixels(window: HWND) -> Vec<u8> {
    let (origin, size) = switch_bounds(window);
    capture_rgb(window, origin, size)
}

fn switch_bounds(window: HWND) -> ((i32, i32), (i32, i32)) {
    let left = physical(window, 346);
    (
        (left, physical(window, 150)),
        (physical(window, 386) - left, 1),
    )
}

fn client_size(window: HWND) -> (i32, i32) {
    let mut rect = RECT::default();
    // SAFETY: the window is live and the client bounds output is writable.
    unsafe { assert_ne!(GetClientRect(window, &raw mut rect), 0) };
    (rect.right, rect.bottom)
}

fn physical(window: HWND, logical: i32) -> i32 {
    // SAFETY: Windows supplies the live Settings handle.
    logical * unsafe { GetDpiForWindow(window) } as i32 / 96
}

fn evidence_path(state: &str) -> Option<PathBuf> {
    std::env::var_os("PIN_GLIDE_SETTINGS_EVIDENCE")
        .map(|directory| Path::new(&directory).join(format!("{state}.ppm")))
}
