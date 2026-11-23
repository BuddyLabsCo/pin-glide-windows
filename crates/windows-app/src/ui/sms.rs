use std::{
    cell::{Cell, RefCell},
    collections::BTreeMap,
    io,
    time::{Duration, Instant},
};

use i_slint_core::animations::{EasingCurve, easing_curve};
use slint::{
    CloseRequestResponse, ComponentHandle, PhysicalPosition, PlatformError, Timer, TimerMode,
    winit_030::WinitWindowAccessor,
};
use windows_sys::Win32::Foundation::{HWND, RECT};

use receiver_core::delivery::Sms;

use crate::{SmsWindow, platform, service::ReceivedSms};

#[cfg(test)]
mod tests;

thread_local! {
    static STACK: RefCell<Stack> = RefCell::default();
    static NEXT_ID: Cell<u64> = const { Cell::new(0) };
}

const FADE_DURATION: Duration = Duration::from_millis(180);
const SLIDE_DURATION: Duration = Duration::from_millis(220);

#[derive(Default)]
struct Stack {
    visible: BTreeMap<u64, Popup>,
    leaving: Vec<Popup>,
    animation: Timer,
    area: Option<WorkArea>,
}

#[derive(Clone, Copy, PartialEq)]
struct WorkArea {
    edges: [i32; 4],
    scale: f32,
}

impl WorkArea {
    fn read(anchor: &Popup) -> io::Result<Self> {
        #[cfg(test)]
        if let Some(area) = tests::WORK_AREA.get() {
            return Ok(area);
        }
        let RECT {
            left,
            top,
            right,
            bottom,
        } = platform::popup_work_area(anchor.hwnd)?;
        Ok(Self {
            edges: [left, top, right, bottom],
            scale: anchor.window.window().scale_factor(),
        })
    }

    fn margin(self) -> i32 {
        (12.0 * self.scale).round() as i32
    }
}

struct Popup {
    window: SmsWindow,
    hwnd: HWND,
    expiry: Timer,
    motion: Option<Motion>,
}

#[derive(Clone, Copy)]
struct Frame {
    position: PhysicalPosition,
    alpha: u8,
}

struct Motion {
    from: Frame,
    to: Frame,
    started: Instant,
    duration: Duration,
}

impl Motion {
    fn frame(&self, now: Instant) -> Option<Frame> {
        let fraction = now.duration_since(self.started).as_secs_f32() / self.duration.as_secs_f32();
        if fraction >= 1.0 {
            return None;
        }
        let progress = easing_curve(&EasingCurve::CubicBezier([0.0, 0.0, 0.58, 1.0]), fraction);
        let mix = |from: f32, to: f32| (from + (to - from) * progress).round();
        Some(Frame {
            position: PhysicalPosition::new(
                mix(self.from.position.x as f32, self.to.position.x as f32) as i32,
                mix(self.from.position.y as f32, self.to.position.y as f32) as i32,
            ),
            alpha: mix(f32::from(self.from.alpha), f32::from(self.to.alpha)) as u8,
        })
    }
}

fn now() -> Instant {
    #[cfg(test)]
    if let Some(time) = tests::TIME.get() {
        return time;
    }
    Instant::now()
}

impl Popup {
    fn draw(&self, frame: Frame) -> io::Result<()> {
        #[cfg(test)]
        if tests::FAIL_DRAW.get() == Some(self.hwnd) {
            return Err(io::Error::other("injected window update failure"));
        }
        platform::position_popup(self.hwnd, frame.position)?;
        platform::fade_popup(self.hwnd, frame.alpha)
    }

    fn start_motion(&mut self, motion: Motion) -> io::Result<()> {
        self.draw(motion.from)?;
        self.motion = Some(motion);
        Ok(())
    }
}

impl Stack {
    fn layout(&mut self, incoming: Option<u64>, animate: bool, started: Instant) -> io::Result<()> {
        let Some(anchor) = self.visible.first_key_value().map(|(_, popup)| popup) else {
            self.area = None;
            return Ok(());
        };
        let area = WorkArea::read(anchor)?;
        self.layout_in(area, incoming, animate, started)?;
        self.area = Some(area);
        Ok(())
    }

    fn layout_in(
        &mut self,
        area: WorkArea,
        incoming: Option<u64>,
        animate: bool,
        started: Instant,
    ) -> io::Result<()> {
        let [_, top, right, work_bottom] = area.edges;
        let margin = area.margin();
        let gap = (8.0 * area.scale).round() as i32;
        let bounds = self
            .visible
            .iter()
            .map(|(&id, popup)| Ok((id, platform::popup_bounds(popup.hwnd)?)))
            .collect::<io::Result<Vec<_>>>()?;
        let mut available = work_bottom - top - 2 * margin + gap;
        let fitting = bounds
            .iter()
            .rev()
            .take(5)
            .take_while(|(_, rect)| {
                available -= rect.bottom - rect.top + gap;
                available >= 0
            })
            .count();
        let (evicted, retained) = bounds.split_at(bounds.len() - fitting);
        for &(id, _) in evicted {
            self.remove(id, animate && Some(id) != incoming, started);
        }

        let mut bottom = work_bottom - margin;
        for &(id, bounds) in retained {
            let popup = self.visible.get_mut(&id).unwrap();
            bottom -= bounds.bottom - bounds.top;
            let to = Frame {
                position: PhysicalPosition::new(
                    right - margin - (bounds.right - bounds.left),
                    bottom,
                ),
                alpha: 255,
            };
            bottom -= gap;
            popup.motion = None;
            if animate && Some(id) == incoming {
                popup.start_motion(Motion {
                    from: Frame {
                        position: PhysicalPosition::new(to.position.x + margin, to.position.y),
                        alpha: 0,
                    },
                    to,
                    started,
                    duration: FADE_DURATION,
                })?;
            } else if animate && bounds.top != to.position.y {
                popup.start_motion(Motion {
                    from: Frame {
                        position: PhysicalPosition::new(to.position.x, bounds.top),
                        alpha: 255,
                    },
                    to,
                    started,
                    duration: SLIDE_DURATION,
                })?;
            } else {
                popup.draw(to)?;
            }
        }
        Ok(())
    }

    fn remove(&mut self, id: u64, animate: bool, started: Instant) -> bool {
        let Some(mut popup) = self.visible.remove(&id) else {
            return false;
        };
        popup.expiry.stop();
        if animate && start_exit(&mut popup, started).is_ok() {
            self.leaving.push(popup);
        } else {
            clear(&popup.window);
        }
        true
    }

    fn tick(&mut self, animate: bool) {
        let now = now();
        for popup in self.visible.values_mut() {
            if let Some(motion) = popup.motion.take() {
                if let Some(frame) = animate.then(|| motion.frame(now)).flatten() {
                    let _ = popup.draw(frame);
                    popup.motion = Some(motion);
                } else {
                    let _ = popup.draw(motion.to);
                }
            }
        }
        self.leaving.retain(|popup| {
            if let Some(frame) = animate.then(|| popup.motion.as_ref()?.frame(now)).flatten()
                && popup.draw(frame).is_ok()
            {
                return true;
            }
            clear(&popup.window);
            false
        });
        if self.leaving.is_empty() && self.visible.values().all(|popup| popup.motion.is_none()) {
            self.animation.stop();
        }
    }

    fn schedule_frame(&mut self, animate: bool) {
        if !animate {
            self.tick(false);
        } else if !self.animation.running()
            && (!self.leaving.is_empty()
                || self.visible.values().any(|popup| popup.motion.is_some()))
        {
            self.animation
                .start(TimerMode::Repeated, Duration::from_millis(16), || {
                    STACK.with_borrow_mut(|stack| stack.tick(true));
                });
        }
    }
}

fn start_exit(popup: &mut Popup, started: Instant) -> io::Result<()> {
    let bounds = platform::popup_bounds(popup.hwnd)?;
    let offset = (12.0 * popup.window.window().scale_factor()).round() as i32;
    platform::prepare_popup_exit(popup.hwnd)?;
    let from = Frame {
        position: PhysicalPosition::new(bounds.left, bounds.top),
        alpha: 255,
    };
    let to = Frame {
        position: PhysicalPosition::new(bounds.left + offset, bounds.top),
        alpha: 0,
    };
    popup.start_motion(Motion {
        from,
        to,
        started,
        duration: FADE_DURATION,
    })
}

pub(crate) fn receive(message: ReceivedSms) {
    if platform::popup_epoch() == Some(message.epoch) {
        let _ = slint::invoke_from_event_loop(move || {
            let _ = slint::spawn_local(show_sms_for_session(
                message.sms,
                message.sender_name,
                platform::chinese(),
                message.epoch,
            ));
        });
    }
}

#[cfg(test)]
pub(crate) fn popup_count() -> usize {
    STACK.with_borrow(|stack| stack.visible.len())
}

pub async fn show_sms(
    sms: Sms,
    sender_name: String,
    chinese: bool,
) -> Result<Option<SmsWindow>, PlatformError> {
    let Some(epoch) = platform::popup_epoch() else {
        return Ok(None);
    };
    show_sms_for_session(sms, sender_name, chinese, epoch).await
}

async fn show_sms_for_session(
    sms: Sms,
    sender_name: String,
    chinese: bool,
    epoch: u64,
) -> Result<Option<SmsWindow>, PlatformError> {
    if platform::popup_epoch() != Some(epoch) {
        return Ok(None);
    }
    let window = platform::new_popup()?;
    window.set_chinese(chinese);
    let native = window.window().winit_window().await?;
    if platform::popup_epoch() != Some(epoch) {
        return Ok(None);
    }
    window.set_otp(sms.otp.into());
    window.set_sms(sms.body.into());
    window.set_sender_name(sender_name.into());
    let native_error = |error| {
        PlatformError::OtherError(format!("Could not configure the SMS popup: {error}").into())
    };
    let owner = platform::prepare_popup(&native).map_err(native_error)?;
    if let Some(position) = STACK.with_borrow(|stack| {
        stack
            .visible
            .first_key_value()
            .map(|(_, popup)| popup.window.window().position())
    }) {
        platform::position_popup(owner, position).map_err(native_error)?;
    }
    window.show()?;
    if platform::popup_epoch() != Some(epoch) {
        clear(&window);
        return Ok(None);
    }

    let id = NEXT_ID.get();
    NEXT_ID.set(id + 1);
    let popup = window.as_weak();
    window.on_copy_code(move || {
        if STACK.with_borrow(|stack| stack.visible.contains_key(&id))
            && let Some(window) = popup.upgrade()
        {
            let otp = window.get_otp();
            if !otp.is_empty() {
                platform::write_unicode_clipboard(owner, &otp);
            }
        }
        dismiss(id);
    });
    window.on_dismiss(move || dismiss(id));
    window.window().on_close_requested(move || {
        let _ = slint::invoke_from_event_loop(move || dismiss(id));
        CloseRequestResponse::KeepWindowShown
    });
    let timer = Timer::default();
    timer.start(TimerMode::SingleShot, Duration::from_secs(30), move || {
        dismiss(id)
    });
    let animate = platform::animations_enabled();
    let visible = STACK.with_borrow_mut(|stack| {
        if let Some((_, previous)) = stack.visible.last_key_value()
            && let Err(error) = platform::order_popup(owner, previous.hwnd)
        {
            clear(&window);
            return Err(native_error(error));
        }
        stack.visible.insert(
            id,
            Popup {
                window: window.clone_strong(),
                hwnd: owner,
                expiry: timer,
                motion: None,
            },
        );
        let started = now();
        let result = stack.layout(Some(id), animate, started);
        if result.is_err() {
            stack.remove(id, false, started);
            let _ = stack.layout(None, false, started);
        }
        stack.schedule_frame(animate);
        result.map_err(native_error)?;
        Ok(stack.visible.contains_key(&id))
    })?;
    if visible {
        native.request_redraw();
    }
    Ok(visible.then_some(window))
}

fn dismiss(id: u64) {
    let animate = platform::animations_enabled();
    STACK.with_borrow_mut(|stack| {
        let started = now();
        if stack.remove(id, animate, started) {
            let _ = stack.layout(None, animate, started);
            stack.schedule_frame(animate);
        }
    });
}

pub(super) fn update_motion() {
    STACK.with_borrow_mut(|stack| {
        let area = stack
            .visible
            .first_key_value()
            .and_then(|(_, anchor)| WorkArea::read(anchor).ok());
        if let Some(area) = area
            && stack.area != Some(area)
        {
            if stack.layout_in(area, None, false, now()).is_ok() {
                stack.area = Some(area);
            }
            stack.tick(false);
        } else if !platform::animations_enabled() {
            stack.tick(false);
        }
    });
}

pub(super) fn close_all() {
    let stack = STACK.take();
    for popup in stack.visible.into_values().chain(stack.leaving) {
        clear(&popup.window);
    }
}

fn clear(window: &SmsWindow) {
    let _ = window.hide();
    window.set_sms(Default::default());
    window.set_otp(Default::default());
}
