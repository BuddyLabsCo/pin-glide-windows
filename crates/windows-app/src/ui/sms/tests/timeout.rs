use super::{
    SAMPLE_TIME, STACK, Sample, WorkArea, advance, assert_exit, bounds, capture, close_all, pause,
    show, until,
};
use crate::ui::settings;
use slint::ComponentHandle;
use std::{
    fs,
    time::{Duration, Instant},
};

pub(super) async fn check(samples: &[Sample], animate: bool) {
    let original = settings::popup_duration_seconds().unwrap();
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("settings.json");
    fs::write(&path, br#"{"version":1}"#).unwrap();
    let settings = settings::test_window(&path);
    settings.invoke_apply_popup_duration("2".into());
    crate::set_animation_preference_override(Some(animate));

    let first_started = Instant::now();
    let bottom = show(&samples[0]).await;
    settings.invoke_apply_popup_duration("5".into());
    let later_started = Instant::now();
    let middle = show(&samples[2]).await;
    let top = show(&samples[1]).await;
    advance(220);
    let before = bounds(&bottom);
    let area: WorkArea = STACK.with_borrow(|stack| stack.area.unwrap());

    pause_before(first_started, 2).await;
    assert!(bottom.window().is_visible());
    if animate {
        until(Duration::from_secs(2), || {
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
    } else {
        until(Duration::from_secs(2), || !bottom.window().is_visible()).await;
        capture("bottom-timeout-reduced-motion", 0);
    }
    assert!(first_started.elapsed() >= Duration::from_secs(2));
    assert!(!bottom.window().is_visible());
    assert!(top.window().is_visible());
    assert_eq!(bounds(&middle).bottom, area.edges[3] - area.margin());

    pause_before(later_started, 5).await;
    assert!(middle.window().is_visible() && top.window().is_visible());
    until(Duration::from_secs(2), || super::popup_count() == 0).await;
    assert!(later_started.elapsed() >= Duration::from_secs(5));
    close_all();
    settings.invoke_apply_popup_duration(original.to_string().into());
}

async fn pause_before(started: Instant, seconds: u64) {
    let time = Duration::from_secs(seconds) - Duration::from_millis(150);
    pause(time.saturating_sub(started.elapsed()).as_millis() as u64).await;
}
