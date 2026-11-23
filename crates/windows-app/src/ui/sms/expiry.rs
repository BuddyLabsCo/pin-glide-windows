use std::time::{Duration, Instant};

use slint::{Timer, TimerMode};

use super::{STACK, dismiss};

const MAX_INTERVAL: Duration = Duration::from_secs(30 * 24 * 60 * 60);

pub(super) fn start(timer: &Timer, id: u64, started: Instant, duration: Duration) {
    let remaining = duration.saturating_sub(started.elapsed());
    timer.start(
        TimerMode::SingleShot,
        remaining.min(MAX_INTERVAL),
        move || {
            if started.elapsed() >= duration {
                dismiss(id);
            } else {
                STACK.with_borrow(|stack| {
                    if let Some(popup) = stack.visible.get(&id) {
                        start(&popup.expiry, id, started, duration);
                    }
                });
            }
        },
    );
}
