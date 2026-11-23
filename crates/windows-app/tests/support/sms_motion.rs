use std::{
    fs,
    path::PathBuf,
    time::{Duration, Instant},
};

use slint::{ComponentHandle, PhysicalPosition, PhysicalSize, Timer, Window};
use windows_app::SmsWindow;
use windows_sys::Win32::{
    Foundation::RECT,
    Graphics::Gdi::{
        BI_RGB, BITMAPINFO, BITMAPINFOHEADER, BitBlt, CAPTUREBLT, CreateCompatibleDC,
        CreateDIBSection, DIB_RGB_COLORS, DeleteDC, DeleteObject, GetDC, ReleaseDC, SRCCOPY,
        SelectObject,
    },
    UI::WindowsAndMessaging::{
        GetLayeredWindowAttributes, HWND_TOPMOST, SWP_NOACTIVATE, SetWindowPos,
    },
};

use super::{
    LogicalPosition, SENDER_NAME, Sample, Sms, click, native_bounds, native_handle,
    set_animation_preference_override, show_sms,
};

slint::slint! {
    #[edition = "1.19"]
    export component SmsBackdrop inherits Window {
        no-frame: true;
        background: #e8f0ed;
    }
}

pub(super) async fn check(samples: &[Sample]) {
    let probe = show_sample(&samples[0]).await;
    let (_, work) = native_bounds(probe.window()).await;
    let scale = probe.window().scale_factor();
    click(probe.window(), LogicalPosition::new(30.0, 95.0));
    let margin = (12.0 * scale).round() as i32;
    let gap = (8.0 * scale).round() as i32;
    let backdrop = Backdrop::new(work, scale).await;
    set_animation_preference_override(Some(true));
    let first = check_arrival(&samples[..2], &backdrop, work.right, margin, gap).await;
    check_middle(samples, first, &backdrop, gap).await;
    check_timeout(samples, &backdrop, work.bottom - margin).await;
    check_overflow(samples, &backdrop, work, gap, margin).await;
    set_animation_preference_override(Some(false));
    check_reduced_motion(samples, &backdrop, gap).await;
}

async fn check_arrival(
    samples: &[Sample],
    backdrop: &Backdrop,
    right: i32,
    margin: i32,
    gap: i32,
) -> SmsWindow {
    let first = show_sample(&samples[0]).await;
    let (starting, _) = native_bounds(first.window()).await;
    assert_eq!(starting.right, right);
    assert_eq!(alpha(first.window()).await, 0);
    pause(65).await;
    let (arriving, _) = native_bounds(first.window()).await;
    assert!(arriving.right < starting.right && arriving.right > right - margin);
    assert!((1..255).contains(&alpha(first.window()).await));
    pause(150).await;
    assert_eq!(native_bounds(first.window()).await.0.right, right - margin);
    assert_eq!(alpha(first.window()).await, 255);
    let second = show_sample(&samples[1]).await;
    pause(65).await;
    assert_eq!(
        native_bounds(second.window()).await.0.bottom,
        native_bounds(first.window()).await.0.top - gap
    );
    assert!((1..255).contains(&alpha(second.window()).await));
    backdrop.capture("arrival");
    click(second.window(), LogicalPosition::new(30.0, 65.0));
    pause(190).await;
    first
}

async fn check_middle(samples: &[Sample], first: SmsWindow, backdrop: &Backdrop, gap: i32) {
    let (first_rect, _) = native_bounds(first.window()).await;
    let middle = show_sample(&samples[2]).await;
    let top = show_sample(&samples[1]).await;
    pause(235).await;
    let (middle_rect, _) = native_bounds(middle.window()).await;
    let (top_rect, _) = native_bounds(top.window()).await;
    assert_eq!(middle_rect.bottom, first_rect.top - gap);
    assert_eq!(top_rect.bottom, middle_rect.top - gap);
    click(middle.window(), LogicalPosition::new(30.0, 95.0));
    pause(65).await;
    assert_eq!(native_bounds(first.window()).await.0.top, first_rect.top);
    let shifted_top = native_bounds(top.window()).await.0.top;
    assert!(
        (top_rect.top + 1..top_rect.top + middle_rect.bottom - middle_rect.top + gap)
            .contains(&shifted_top)
    );
    assert!((1..255).contains(&alpha(middle.window()).await));
    backdrop.capture("middle-dismissal");
    pause(190).await;
    assert!(!middle.window().is_visible());
    assert_eq!(
        native_bounds(top.window()).await.0.bottom,
        first_rect.top - gap
    );
    click(top.window(), LogicalPosition::new(30.0, 65.0));
    click(first.window(), LogicalPosition::new(30.0, 95.0));
    pause(190).await;
}

async fn check_timeout(samples: &[Sample], backdrop: &Backdrop, bottom: i32) {
    let first = show_sample(&samples[0]).await;
    pause(450).await;
    let middle = show_sample(&samples[2]).await;
    let top = show_sample(&samples[1]).await;
    pause(200).await;

    let limit = Instant::now() + Duration::from_secs(31);
    while alpha(first.window()).await == 255 {
        pause(15).await;
        assert!(
            first.window().is_visible(),
            "the bottom popup expired without a fade"
        );
        assert!(Instant::now() < limit, "the bottom popup did not expire");
    }
    pause(45).await;
    backdrop.capture("bottom-timeout");
    assert!(top.window().is_visible());
    pause(210).await;
    assert!(!first.window().is_visible());
    assert_eq!(native_bounds(middle.window()).await.0.bottom, bottom);
    assert!(top.window().is_visible());
    click(top.window(), LogicalPosition::new(30.0, 65.0));
    click(middle.window(), LogicalPosition::new(30.0, 95.0));
    pause(190).await;
}

async fn check_reduced_motion(samples: &[Sample], backdrop: &Backdrop, gap: i32) {
    let first = show_sample(&samples[0]).await;
    let middle = show_sample(&samples[2]).await;
    let top = show_sample(&samples[1]).await;
    click(middle.window(), LogicalPosition::new(30.0, 95.0));
    assert!(!middle.window().is_visible());
    assert_eq!(alpha(top.window()).await, 255);
    assert_eq!(
        native_bounds(top.window()).await.0.bottom,
        native_bounds(first.window()).await.0.top - gap
    );
    pause(65).await;
    backdrop.capture("reduced-motion");
    click(top.window(), LogicalPosition::new(30.0, 65.0));
    click(first.window(), LogicalPosition::new(30.0, 95.0));
}

async fn check_overflow(
    samples: &[Sample],
    backdrop: &Backdrop,
    work: RECT,
    gap: i32,
    margin: i32,
) {
    for (name, sample) in [
        ("height-overflow", &samples[2]),
        ("limit-overflow", &samples[1]),
    ] {
        let mut popups = Vec::new();
        let mut used = 0;
        let mut oldest = 0;
        for index in 0..6 {
            let popup = show_sample(sample).await;
            let (rect, _) = native_bounds(popup.window()).await;
            let height = rect.bottom - rect.top + gap;
            used += height;
            popups.push((popup, height));
            let previous = oldest;
            while index - oldest >= 5 || used > work.bottom - work.top - 2 * margin + gap {
                used -= popups[oldest].1;
                oldest += 1;
            }
            if oldest > previous && previous == 0 {
                pause(65).await;
                backdrop.capture(name);
            }
            pause(235).await;
            let mut bottom = work.bottom - margin;
            for (position, (popup, height)) in popups.iter().enumerate() {
                assert_eq!(popup.window().is_visible(), position >= oldest);
                if position >= oldest {
                    assert_eq!(native_bounds(popup.window()).await.0.bottom, bottom);
                    bottom -= height;
                }
            }
        }
        for (popup, _) in popups.into_iter().skip(oldest) {
            click(popup.window(), LogicalPosition::new(30.0, 95.0));
        }
        pause(190).await;
    }
}

async fn show_sample(sample: &Sample) -> SmsWindow {
    show_sms(
        Sms {
            body: sample.sms.clone(),
            otp: sample.otp.clone(),
        },
        SENDER_NAME.into(),
        true,
    )
    .await
    .unwrap()
    .unwrap()
}

async fn pause(millis: u64) {
    let (send, receive) = tokio::sync::oneshot::channel();
    Timer::single_shot(Duration::from_millis(millis), move || {
        let _ = send.send(());
    });
    receive.await.unwrap();
}

async fn alpha(window: &Window) -> u8 {
    let mut alpha = 0;
    // SAFETY: the window owns a live HWND and alpha is a writable output.
    unsafe {
        assert_ne!(
            GetLayeredWindowAttributes(
                native_handle(window).await,
                std::ptr::null_mut(),
                &raw mut alpha,
                std::ptr::null_mut(),
            ),
            0
        );
    }
    alpha
}

struct Backdrop {
    window: Option<SmsBackdrop>,
    area: RECT,
    directory: Option<PathBuf>,
}

impl Backdrop {
    async fn new(work: RECT, scale: f32) -> Self {
        let inset = (8.0 * scale).round() as i32;
        let area = RECT {
            left: work.right - (404.0 * scale).round() as i32,
            top: work.top + inset,
            bottom: work.bottom - inset,
            ..work
        };
        let directory = std::env::var_os("PINGLIDE_SMS_EVIDENCE_DIR").map(PathBuf::from);
        let window = if let Some(directory) = &directory {
            let width = area.right - area.left;
            let height = area.bottom - area.top;
            let bleed = (12.0 * scale).round() as i32;
            fs::write(
                directory.join("stack-metrics.txt"),
                format!("{width} {} {scale} {inset}", work.bottom - work.top),
            )
            .unwrap();
            let window = SmsBackdrop::new().unwrap();
            window
                .window()
                .set_position(PhysicalPosition::new(area.left - bleed, work.top - bleed));
            window.window().set_size(PhysicalSize::new(
                (width + 2 * bleed) as u32,
                (height + 2 * (bleed + inset)) as u32,
            ));
            window.show().unwrap();
            let hwnd = native_handle(window.window()).await;
            // SAFETY: the test owns the live backdrop handle and keeps it below subsequently shown popups.
            unsafe {
                assert_ne!(
                    SetWindowPos(
                        hwnd,
                        HWND_TOPMOST,
                        area.left - bleed,
                        work.top - bleed,
                        width + 2 * bleed,
                        height + 2 * (bleed + inset),
                        SWP_NOACTIVATE
                    ),
                    0
                );
            }
            pause(65).await;
            Some(window)
        } else {
            None
        };
        Self {
            window,
            area,
            directory,
        }
    }

    fn capture(&self, name: &str) {
        let Some(directory) = &self.directory else {
            return;
        };
        let width = self.area.right - self.area.left;
        let height = self.area.bottom - self.area.top;
        let bitmap_info = BITMAPINFO {
            bmiHeader: BITMAPINFOHEADER {
                biSize: size_of::<BITMAPINFOHEADER>() as u32,
                biWidth: width,
                biHeight: -height,
                biPlanes: 1,
                biBitCount: 32,
                biCompression: BI_RGB,
                ..Default::default()
            },
            ..Default::default()
        };
        let mut result = format!("P6\n{width} {height}\n255\n").into_bytes();
        // SAFETY: the DIB contains width * height 32-bit pixels and stays live until it is deleted.
        unsafe {
            let screen = GetDC(std::ptr::null_mut());
            let memory = CreateCompatibleDC(screen);
            let mut pixels = std::ptr::null_mut();
            let bitmap = CreateDIBSection(
                screen,
                &raw const bitmap_info,
                DIB_RGB_COLORS,
                &raw mut pixels,
                std::ptr::null_mut(),
                0,
            );
            assert!(!bitmap.is_null() && !pixels.is_null());
            let previous = SelectObject(memory, bitmap);
            assert_ne!(
                BitBlt(
                    memory,
                    0,
                    0,
                    width,
                    height,
                    screen,
                    self.area.left,
                    self.area.top,
                    SRCCOPY | CAPTUREBLT
                ),
                0
            );
            let pixels =
                std::slice::from_raw_parts(pixels.cast::<u8>(), (width * height * 4) as usize);
            let covered_rows = pixels
                .chunks_exact(width as usize * 4)
                .filter(|row| row[..3] == [0xed, 0xf0, 0xe8])
                .count();
            result.extend(
                pixels
                    .as_chunks::<4>()
                    .0
                    .iter()
                    .flat_map(|pixel| [pixel[2], pixel[1], pixel[0]]),
            );
            SelectObject(memory, previous);
            DeleteObject(bitmap);
            DeleteDC(memory);
            ReleaseDC(std::ptr::null_mut(), screen);
            assert_eq!(
                covered_rows, height as usize,
                "the evidence backdrop is obscured"
            );
        }
        fs::write(directory.join(format!("stack-{name}.ppm")), result).unwrap();
    }
}

impl Drop for Backdrop {
    fn drop(&mut self) {
        if let Some(window) = &self.window {
            window.hide().unwrap();
        }
    }
}
