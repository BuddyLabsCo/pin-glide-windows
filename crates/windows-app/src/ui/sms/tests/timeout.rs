use super::{
    EXPIRY_INTERVAL, REMOVALS, SAMPLE_TIME, STACK, Sample, advance, assert_exit, bounds, capture,
    close_all, handle, pause, show, until,
};
use crate::{SmsWindow, ui::settings};
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
    REMOVALS.with_borrow_mut(|times| times.clear());

    let bottom = TimedPopup::open(&samples[0]).await;
    settings.invoke_apply_popup_duration("5".into());
    let middle = TimedPopup::open(&samples[2]).await;
    pause(300).await;
    let top = TimedPopup::open(&samples[1]).await;
    advance(220);
    let before = bounds(&bottom.window);
    let area = STACK.with_borrow(|stack| stack.area.unwrap());

    bottom.assert_expiry(2).await;
    if animate {
        assert!(bottom.window.window().is_visible());
        advance(SAMPLE_TIME);
        assert_exit(&bottom.window, before, area.margin());
        capture("bottom-timeout", SAMPLE_TIME);
        advance(114);
        assert!(bottom.window.window().is_visible());
        advance(1);
        assert!(!bottom.window.window().is_visible());
        advance(40);
    } else {
        assert!(!bottom.window.window().is_visible());
        capture("bottom-timeout-reduced-motion", 0);
    }
    assert!(top.active() && middle.active());
    assert_eq!(bounds(&middle.window).bottom, area.edges[3] - area.margin());
    middle.assert_expiry(5).await;
    top.assert_expiry(5).await;
    close_all();

    if !animate {
        settings.invoke_apply_popup_duration("1".into());
        check_rearm(&samples[0]).await;
    }
    settings.invoke_apply_popup_duration(original.to_string().into());
}

async fn check_rearm(sample: &Sample) {
    let cap = Duration::from_millis(600);
    EXPIRY_INTERVAL.set(Some(cap));
    let popup = TimedPopup::open(sample).await;
    assert_eq!(popup.interval(), Some(cap));
    until(Duration::from_millis(850), || {
        popup.interval().is_some_and(|interval| interval < cap)
    })
    .await;
    assert!(popup.active());
    popup.assert_expiry(1).await;
    assert!(!popup.window.window().is_visible());
    EXPIRY_INTERVAL.set(None);
}

struct TimedPopup {
    window: SmsWindow,
    id: u64,
    before_open: Instant,
    after_open: Instant,
}

impl TimedPopup {
    async fn open(sample: &Sample) -> Self {
        let before_open = Instant::now();
        let window = show(sample).await;
        let after_open = Instant::now();
        let hwnd = handle(&window);
        let id = STACK.with_borrow(|stack| {
            *stack
                .visible
                .iter()
                .find(|(_, popup)| popup.hwnd == hwnd)
                .unwrap()
                .0
        });
        Self {
            window,
            id,
            before_open,
            after_open,
        }
    }

    fn active(&self) -> bool {
        STACK.with_borrow(|stack| stack.visible.contains_key(&self.id))
    }

    fn interval(&self) -> Option<Duration> {
        STACK.with_borrow(|stack| {
            let timer = &stack.visible.get(&self.id)?.expiry;
            timer.running().then(|| timer.interval())
        })
    }

    async fn assert_expiry(&self, seconds: u64) {
        let duration = Duration::from_secs(seconds);
        let earliest = self.before_open + duration;
        let latest = self.after_open + duration + Duration::from_millis(150);
        let expired_at = || REMOVALS.with_borrow(|times| times.get(&self.id).copied());
        until(
            latest.saturating_duration_since(Instant::now()) + Duration::from_millis(10),
            || expired_at().is_some() || Instant::now() >= latest,
        )
        .await;
        let actual = expired_at().unwrap_or_else(|| {
            panic!(
                "popup {} did not expire by its {seconds}s deadline",
                self.id
            );
        });
        assert!(
            (earliest..=latest).contains(&actual),
            "popup {} expired after {:?}, outside its {:?}..={:?} deadline window",
            self.id,
            actual.duration_since(self.before_open),
            duration,
            latest.duration_since(self.before_open)
        );
    }
}
