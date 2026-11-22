use std::{cell::RefCell, sync::Arc, time::Duration};

use qrcode::{Color, QrCode};
use slint::{
    CloseRequestResponse, ComponentHandle, Image, PlatformError, Rgb8Pixel, SharedPixelBuffer,
    Timer, TimerMode,
};
use windows_app::service::{PairingOperation, PairingOutcome, PairingProgress};

use crate::{PairingStage, PairingWindow, platform};

thread_local! {
    static PAIRING: RefCell<Option<View>> = const { RefCell::new(None) };
}

struct View {
    window: PairingWindow,
    ticker: Timer,
    operation: PairingOperation,
    rendered_code: Option<Arc<QrCode>>,
}

pub(super) fn open(chinese: bool) -> Result<(), PlatformError> {
    if reopen()? {
        return Ok(());
    }
    let window = PairingWindow::new()?;
    window.set_chinese(chinese);
    window.set_motion_enabled(platform::animations_enabled());
    window.set_stage(PairingStage::Preparing);
    window
        .window()
        .on_close_requested(|| CloseRequestResponse::HideWindow);
    window.on_refresh(refresh);
    window.show()?;
    super::focus(window.window());
    PAIRING.with_borrow_mut(|pairing| {
        let view = View {
            window,
            ticker: Timer::default(),
            operation: start_operation(),
            rendered_code: None,
        };
        view.watch_progress();
        *pairing = Some(view);
    });
    Ok(())
}

pub(super) fn reopen() -> Result<bool, PlatformError> {
    PAIRING.with_borrow_mut(|pairing| {
        let Some(view) = pairing else {
            return Ok(false);
        };
        view.render();
        view.window
            .set_motion_enabled(platform::animations_enabled());
        if !view.window.window().is_visible() {
            view.window.show()?;
        }
        super::focus(view.window.window());
        Ok(true)
    })
}

pub(super) fn hide() -> Result<(), PlatformError> {
    PAIRING.with_borrow(|pairing| match pairing {
        Some(view) => view.window.hide(),
        None => Ok(()),
    })
}

pub(super) fn update_motion() {
    PAIRING.with_borrow(|pairing| {
        if let Some(view) = pairing {
            view.window
                .set_motion_enabled(platform::animations_enabled());
        }
    });
}

pub(super) fn shutdown() {
    PAIRING.with_borrow_mut(|pairing| *pairing = None);
}

fn start_operation() -> PairingOperation {
    PairingOperation::start(option_env!("PINGLIDE_SERVICE_URL"))
}

fn refresh() {
    PAIRING.with_borrow_mut(|pairing| {
        let Some(view) = pairing else { return };
        if matches!(view.operation.current(), PairingProgress::Finished(_)) {
            view.operation = start_operation();
            view.render();
            view.watch_progress();
        }
    });
}

impl View {
    fn watch_progress(&self) {
        self.ticker
            .start(TimerMode::Repeated, Duration::from_millis(100), || {
                PAIRING.with_borrow_mut(|pairing| {
                    if let Some(view) = pairing {
                        view.render();
                    }
                });
            });
    }

    fn render(&mut self) {
        let stage = match self.operation.current() {
            PairingProgress::Preparing => PairingStage::Preparing,
            PairingProgress::Waiting { code, seconds } => {
                if !self
                    .rendered_code
                    .as_ref()
                    .is_some_and(|previous| Arc::ptr_eq(previous, &code))
                {
                    self.window.set_qr(qr_image(&code));
                    self.rendered_code = Some(code);
                }
                self.window
                    .set_remaining(format!("{:02}:{:02}", seconds / 60, seconds % 60).into());
                PairingStage::Waiting
            }
            PairingProgress::Confirming => PairingStage::Confirming,
            PairingProgress::Finished(outcome) => {
                self.ticker.stop();
                match outcome {
                    PairingOutcome::CreateFailed => PairingStage::CreateFailed,
                    PairingOutcome::Failed => PairingStage::Failed,
                    PairingOutcome::Expired => PairingStage::Expired,
                    PairingOutcome::Canceled => PairingStage::Canceled,
                }
            }
        };
        if stage != PairingStage::Waiting && self.rendered_code.take().is_some() {
            self.window.set_qr(Image::default());
        }
        self.window.set_stage(stage);
    }
}

fn qr_image(code: &QrCode) -> Image {
    let side = 160;
    let scale = side / (code.width() + 8);
    let offset = (side - code.width() * scale) / 2;
    let mut pixels = SharedPixelBuffer::<Rgb8Pixel>::new(side as u32, side as u32);
    let buffer = pixels.make_mut_slice();
    buffer.fill(Rgb8Pixel::new(255, 255, 255));
    for row in 0..code.width() * scale {
        for column in 0..code.width() * scale {
            if code[(column / scale, row / scale)] == Color::Dark {
                buffer[(offset + row) * side + offset + column] = Rgb8Pixel::new(35, 54, 48);
            }
        }
    }
    Image::from_rgb8(pixels)
}
