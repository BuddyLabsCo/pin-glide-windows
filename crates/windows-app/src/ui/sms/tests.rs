use std::{fs, path::PathBuf, process::Command};

use serde::Deserialize;
use windows_sys::Win32::UI::WindowsAndMessaging::{
    GetLayeredWindowAttributes, SPI_SETWORKAREA, SendMessageW, WM_DISPLAYCHANGE, WM_DPICHANGED,
    WM_SETTINGCHANGE,
};

use super::*;

thread_local! {
    pub(super) static TIME: Cell<Option<Instant>> = const { Cell::new(None) };
    pub(super) static WORK_AREA: Cell<Option<WorkArea>> = const { Cell::new(None) };
    pub(super) static FAIL_DRAW: Cell<Option<HWND>> = const { Cell::new(None) };
}

const SAMPLE_TIME: u64 = 65;
const FADE_AT_SAMPLE: f32 = 0.523_360_8;
const SLIDE_AT_SAMPLE: f32 = 0.439_204_25;

#[derive(Deserialize)]
struct Sample {
    sms: String,
    otp: String,
}

#[test]
#[ignore = "requires an unlocked interactive Windows desktop"]
fn sms_stack_matches_the_reference_timeline() {
    const CHILD: &str = "PIN_GLIDE_SMS_MOTION_TEST_CHILD";
    if std::env::var_os(CHILD).is_none() {
        let child = Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "ui::sms::tests::sms_stack_matches_the_reference_timeline",
                "--ignored",
            ])
            .env(CHILD, "1")
            .status()
            .unwrap();
        assert!(
            child.success(),
            "SMS motion test failed in its child process"
        );
        return;
    }
    platform::enable_per_monitor_dpi().unwrap();
    crate::startup_tests::select_test_backend().unwrap();
    let tray = platform::Tray::new(&platform::wide("Pin Glide SMS motion fixture")).unwrap();
    let handle = tray.window();
    slint::spawn_local(async move {
        let gallery = include_str!("../../../../../docs/prototypes/ui-tool/data/messages.js");
        let samples: Vec<Sample> = serde_json::from_str(
            gallery
                .trim()
                .strip_prefix("window.UI_CASES = ")
                .unwrap()
                .trim_end_matches(';'),
        )
        .unwrap();
        platform::set_animation_preference_override(Some(false));
        let probe = show(&samples[0]).await;
        let area = STACK.with_borrow(|stack| stack.area.unwrap());
        probe.invoke_dismiss();
        TIME.set(Some(Instant::now()));
        platform::set_animation_preference_override(Some(true));
        check_arrival(&samples, area).await;
        check_middle(&samples).await;
        check_timeout(&samples).await;
        let short_area = fixture_area(800.0, area.scale);
        let tall_area = fixture_area(900.0, area.scale);
        check_overflow(&samples[2], "height-overflow", short_area, 4).await;
        check_overflow(&samples[1], "limit-overflow", tall_area, 6).await;
        check_arrival_layout_failure(&samples[2], short_area).await;
        check_work_area_change(&samples, short_area, handle, true).await;
        check_work_area_change(&samples, short_area, handle, false).await;
        check_dpi_change(&samples, short_area, handle).await;
        slint::quit_event_loop().unwrap();
    })
    .unwrap();
    slint::run_event_loop_until_quit().unwrap();
}

async fn check_arrival(samples: &[Sample], area: WorkArea) {
    let first = show(&samples[0]).await;
    let start = bounds(&first);
    assert_eq!(start.right, area.edges[2]);
    assert_eq!(alpha(&first), 0);
    advance(SAMPLE_TIME);
    assert_eq!(alpha(&first), 133);
    assert_eq!(
        bounds(&first).right,
        start.right - (area.margin() as f32 * FADE_AT_SAMPLE).round() as i32
    );

    let second = show(&samples[1]).await;
    assert_eq!(alpha(&first), 255);
    assert_eq!(bounds(&first).right, area.edges[2] - area.margin());
    assert_eq!(alpha(&second), 0);
    advance(SAMPLE_TIME);
    assert_eq!(alpha(&second), 133);
    capture("arrival", SAMPLE_TIME);
    advance(115);
    assert_eq!(alpha(&second), 255);
    close_all();
}

async fn check_middle(samples: &[Sample]) {
    let bottom = show(&samples[0]).await;
    let middle = show(&samples[2]).await;
    let top = show(&samples[1]).await;
    advance(220);
    let bottom_before = bounds(&bottom);
    let middle_before = bounds(&middle);
    let top_before = bounds(&top);
    let area = STACK.with_borrow(|stack| stack.area.unwrap());
    let gap = (8.0 * area.scale).round() as i32;
    let distance = middle_before.bottom - middle_before.top + gap;
    middle.invoke_dismiss();
    advance(SAMPLE_TIME);
    assert_eq!(bounds(&bottom).top, bottom_before.top);
    assert_eq!(
        bounds(&top).top,
        top_before.top + (distance as f32 * SLIDE_AT_SAMPLE).round() as i32
    );
    assert_exit(&middle, middle_before, area.margin());
    capture("middle-dismissal", SAMPLE_TIME);
    advance(114);
    assert!(middle.window().is_visible());
    advance(1);
    assert!(!middle.window().is_visible());
    advance(40);
    assert_eq!(bounds(&top).bottom, bottom_before.top - gap);
    close_all();

    platform::set_animation_preference_override(Some(false));
    let bottom = show(&samples[0]).await;
    let middle = show(&samples[2]).await;
    let top = show(&samples[1]).await;
    middle.invoke_dismiss();
    assert!(!middle.window().is_visible());
    assert_eq!(bounds(&top).bottom, bounds(&bottom).top - gap);
    capture("reduced-motion", 0);
    close_all();
    platform::set_animation_preference_override(Some(true));
}

async fn check_timeout(samples: &[Sample]) {
    let bottom = show(&samples[0]).await;
    pause(1500).await;
    let middle = show(&samples[2]).await;
    let top = show(&samples[1]).await;
    advance(220);
    let before = bounds(&bottom);
    let area = STACK.with_borrow(|stack| stack.area.unwrap());
    until(Duration::from_secs(31), || {
        STACK.with_borrow(|stack| {
            stack
                .leaving
                .iter()
                .any(|popup| std::ptr::eq(popup.window.window(), bottom.window()))
        })
    })
    .await;
    advance(SAMPLE_TIME);
    assert_exit(&bottom, before, area.margin());
    capture("bottom-timeout", SAMPLE_TIME);
    advance(155);
    assert!(!bottom.window().is_visible());
    assert!(top.window().is_visible());
    assert_eq!(bounds(&middle).bottom, area.edges[3] - area.margin());
    close_all();
}

async fn check_overflow(sample: &Sample, name: &str, area: WorkArea, incoming: usize) {
    WORK_AREA.set(Some(area));
    let mut popups = Vec::new();
    for _ in 1..incoming {
        popups.push(show(sample).await);
        advance(220);
    }
    let before = bounds(&popups[0]);
    let height = before.bottom - before.top;
    let gap = (8.0 * area.scale).round() as i32;
    let usable = area.edges[3] - area.edges[1] - 2 * area.margin();
    assert_eq!(popup_count(), incoming - 1);
    if incoming == 6 {
        assert!(6 * height + 5 * gap <= usable, "six short cards must fit");
    } else {
        assert!(
            4 * height + 3 * gap > usable,
            "height must evict the fourth card"
        );
    }
    popups.push(show(sample).await);
    advance(SAMPLE_TIME);
    assert_exit(&popups[0], before, area.margin());
    capture(name, SAMPLE_TIME);
    advance(155);
    assert!(!popups[0].window().is_visible());
    assert!(popups[1..].iter().all(|popup| popup.window().is_visible()));
    assert_eq!(popup_count(), incoming - 1);
    assert_eq!(bounds(&popups[1]).bottom, area.edges[3] - area.margin());
    close_all();
    WORK_AREA.set(None);
}

async fn check_arrival_layout_failure(sample: &Sample, area: WorkArea) {
    WORK_AREA.set(Some(area));
    let oldest = show(sample).await;
    let retained = show(sample).await;
    let _ = show(sample).await;
    advance(220);
    FAIL_DRAW.set(Some(handle(&retained)));
    assert!(show_sms(sms(sample), "Fixture".into(), true).await.is_err());
    FAIL_DRAW.set(None);
    assert!(STACK.with_borrow(|stack| stack.animation.running()));
    TIME.set(Some(now() + FADE_DURATION));
    until(Duration::from_secs(2), || !oldest.window().is_visible()).await;
    assert!(retained.window().is_visible());
    close_all();
    WORK_AREA.set(None);
}

async fn check_work_area_change(samples: &[Sample], original: WorkArea, tray: HWND, animate: bool) {
    platform::set_animation_preference_override(Some(animate));
    WORK_AREA.set(Some(original));
    let leaving = show(&samples[1]).await;
    let oldest = show(&samples[0]).await;
    let middle = show(&samples[2]).await;
    let newest = show(&samples[3]).await;
    advance(220);
    leaving.invoke_dismiss();
    advance(SAMPLE_TIME);
    let was_leaving = bounds(&leaving);
    let oldest_before = bounds(&oldest);
    let mut area = original;
    area.edges[2] -= 63;
    area.edges[3] -= 45;
    area.edges[1] = area.edges[3] - (500.0 * area.scale).round() as i32;
    WORK_AREA.set(Some(area));
    // SAFETY: the fixture owns the live tray HWND on the current thread.
    unsafe { SendMessageW(tray, WM_SETTINGCHANGE, SPI_SETWORKAREA as usize, 0) };
    until(Duration::from_secs(2), || popup_count() == 2).await;
    assert_anchored(&middle, &newest, area);
    assert_eq!(oldest.window().is_visible(), animate);
    if animate {
        assert!(leaving.window().is_visible());
        assert_eq!(alpha(&leaving), 122);
        let still_leaving = bounds(&leaving);
        assert_eq!(
            (still_leaving.left, still_leaving.top),
            (was_leaving.left, was_leaving.top)
        );
        assert_eq!(alpha(&oldest), 255);
        advance(SAMPLE_TIME);
        assert_exit(&oldest, oldest_before, area.margin());
        advance(115);
        assert!(!oldest.window().is_visible() && !leaving.window().is_visible());
        capture("work-area-change", 180);
    } else {
        assert!(!leaving.window().is_visible());
    }
    close_all();
    WORK_AREA.set(None);
}

async fn check_dpi_change(samples: &[Sample], original: WorkArea, tray: HWND) {
    WORK_AREA.set(Some(original));
    let lower = show(&samples[0]).await;
    let upper = show(&samples[2]).await;
    let mut area = original;
    area.edges[2] -= 63;
    WORK_AREA.set(Some(area));
    // SAFETY: the fixture owns the live tray HWND on the current thread.
    unsafe { SendMessageW(tray, WM_DISPLAYCHANGE, 0, 0) };
    until(Duration::from_secs(2), || {
        bounds(&lower).right == area.edges[2] - area.margin()
    })
    .await;
    assert_anchored(&lower, &upper, area);

    let old_scale = lower.window().scale_factor();
    area.scale = if old_scale < 1.75 { 2.0 } else { 1.25 };
    WORK_AREA.set(Some(area));
    for popup in [&lower, &upper] {
        let mut suggested = bounds(popup);
        suggested.right = suggested.left
            + ((suggested.right - suggested.left) as f32 * area.scale / old_scale).round() as i32;
        suggested.bottom = suggested.top
            + ((suggested.bottom - suggested.top) as f32 * area.scale / old_scale).round() as i32;
        let dpi = (96.0 * area.scale) as usize;
        // SAFETY: both HWNDs are owned by this fixture and the suggested rectangle lives through dispatch.
        unsafe {
            SendMessageW(
                handle(popup),
                WM_DPICHANGED,
                dpi | (dpi << 16),
                (&raw const suggested) as isize,
            )
        };
    }
    until(Duration::from_secs(2), || {
        lower.window().scale_factor() == area.scale
            && upper.window().scale_factor() == area.scale
            && is_anchored(&lower, &upper, area)
    })
    .await;
    for popup in [&lower, &upper] {
        assert!(((extent(bounds(popup))[0] as f32 / area.scale) - 380.0).abs() <= 1.0);
    }
    close_all();
    WORK_AREA.set(None);
}

fn assert_anchored(lower: &SmsWindow, upper: &SmsWindow, area: WorkArea) {
    assert!(
        is_anchored(lower, upper, area),
        "the stack did not reanchor"
    );
}

fn is_anchored(lower: &SmsWindow, upper: &SmsWindow, area: WorkArea) -> bool {
    let lower_bounds = bounds(lower);
    let upper_bounds = bounds(upper);
    let gap = (8.0 * area.scale).round() as i32;
    let right = area.edges[2] - area.margin();
    lower.window().is_visible()
        && upper.window().is_visible()
        && lower_bounds.right == right
        && upper_bounds.right == right
        && lower_bounds.bottom == area.edges[3] - area.margin()
        && upper_bounds.bottom == lower_bounds.top - gap
}

fn fixture_area(height: f32, scale: f32) -> WorkArea {
    WorkArea {
        edges: [0, 1400 - (height * scale).round() as i32, 1200, 1400],
        scale,
    }
}

fn advance(millis: u64) {
    TIME.set(Some(now() + Duration::from_millis(millis)));
    STACK.with_borrow_mut(|stack| stack.tick(true));
}

fn assert_exit(popup: &SmsWindow, before: RECT, offset: i32) {
    assert_eq!(alpha(popup), 122);
    assert_eq!(bounds(popup).top, before.top);
    assert_eq!(
        bounds(popup).right,
        before.right + (offset as f32 * FADE_AT_SAMPLE).round() as i32
    );
}

fn handle(popup: &SmsWindow) -> HWND {
    use slint::winit_030::winit::raw_window_handle::{HasWindowHandle, RawWindowHandle};

    popup
        .window()
        .with_winit_window(|window| {
            let RawWindowHandle::Win32(handle) = window.window_handle().unwrap().as_raw() else {
                panic!("expected a native Windows popup");
            };
            handle.hwnd.get() as HWND
        })
        .unwrap()
}

fn bounds(popup: &SmsWindow) -> RECT {
    platform::popup_bounds(handle(popup)).unwrap()
}

fn alpha(popup: &SmsWindow) -> u8 {
    let mut alpha = 0;
    // SAFETY: the fixture owns this window and alpha is writable.
    unsafe {
        assert_ne!(
            GetLayeredWindowAttributes(
                handle(popup),
                std::ptr::null_mut(),
                &raw mut alpha,
                std::ptr::null_mut()
            ),
            0
        );
    }
    alpha
}

fn sms(sample: &Sample) -> Sms {
    Sms {
        body: sample.sms.clone(),
        otp: sample.otp.clone(),
    }
}

async fn show(sample: &Sample) -> SmsWindow {
    show_sms(sms(sample), "我的设备".into(), true)
        .await
        .unwrap()
        .unwrap()
}

async fn until(limit: Duration, ready: impl Fn() -> bool) {
    let deadline = Instant::now() + limit;
    while !ready() {
        assert!(Instant::now() < deadline, "desktop state did not settle");
        pause(10).await;
    }
}

async fn pause(millis: u64) {
    let (send, receive) = tokio::sync::oneshot::channel();
    Timer::single_shot(Duration::from_millis(millis), move || {
        let _ = send.send(());
    });
    receive.await.unwrap();
}

fn capture(name: &str, elapsed: u64) {
    let Some(directory) = std::env::var_os("PINGLIDE_SMS_EVIDENCE_DIR").map(PathBuf::from) else {
        return;
    };
    STACK.with_borrow(|stack| {
        let area = stack.area.unwrap();
        let [_, top, right, bottom] = area.edges;
        let width = (404.0 * area.scale).round() as i32;
        let height = bottom - top;
        let mut canvas = vec![[232_u8, 240, 237]; (width * height) as usize];
        for popup in stack.visible.values().rev().chain(&stack.leaving) {
            let rect = bounds(&popup.window);
            let image = popup.window.window().take_snapshot().unwrap();
            let alpha = u32::from(alpha(&popup.window));
            for (index, pixel) in image.as_slice().iter().enumerate() {
                let x = rect.left - (right - width) + index as i32 % image.width() as i32;
                let y = rect.top - top + index as i32 / image.width() as i32;
                if (0..width).contains(&x) && (0..height).contains(&y) {
                    let dest = &mut canvas[(y * width + x) as usize];
                    for (channel, source) in dest.iter_mut().zip([pixel.r, pixel.g, pixel.b]) {
                        *channel = ((u32::from(source) * alpha
                            + u32::from(*channel) * (255 - alpha)
                            + 127)
                            / 255) as u8;
                    }
                }
            }
        }
        let mut ppm = format!("P6\n{width} {height}\n255\n").into_bytes();
        ppm.extend(canvas.into_iter().flatten());
        fs::write(directory.join(format!("stack-{name}.ppm")), ppm).unwrap();
        fs::write(
            directory.join(format!("stack-{name}.txt")),
            format!("{width} {height} {} {elapsed}", area.scale),
        )
        .unwrap();
    });
}
