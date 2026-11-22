use std::{path::Path, time::Duration};

use slint::{
    ComponentHandle, LogicalPosition, Rgba8Pixel, SharedPixelBuffer,
    platform::{PointerEventButton, WindowEvent},
};
use windows_app::PairingWindow;

#[derive(Default)]
pub(super) struct CompletionFrames {
    first: Option<SharedPixelBuffer<Rgba8Pixel>>,
    intermediate: Vec<u8>,
}

impl CompletionFrames {
    pub(super) fn observe(
        &mut self,
        window: &PairingWindow,
        elapsed: Duration,
        moving: bool,
        evidence: Option<&Path>,
    ) -> bool {
        let frame = window.window().take_snapshot().unwrap();
        let first = self.first.get_or_insert_with(|| frame.clone());
        let level = check_background(&frame);
        if moving
            && level < check_background(first)
            && level > 227
            && self.intermediate.last().is_none_or(|&last| level < last)
        {
            self.intermediate.push(level);
            if let Some(directory) = evidence
                && self.intermediate.len() <= 2
            {
                let name = format!("pair-done-transition-{}.ppm", self.intermediate.len());
                super::capture(window, &directory.join(name));
            }
        }
        if elapsed < Duration::from_millis(210) {
            return false;
        }
        if moving {
            assert!(!self.intermediate.is_empty(), "a partially faded overlay");
            assert_eq!(level, 227);
        } else {
            assert!(first.as_slice() == frame.as_slice());
        }
        true
    }
}

fn check_background(pixels: &SharedPixelBuffer<Rgba8Pixel>) -> u8 {
    sample(pixels, 220, 160).r
}

fn sample(pixels: &SharedPixelBuffer<Rgba8Pixel>, x: usize, y: usize) -> Rgba8Pixel {
    let width = pixels.width() as usize;
    pixels.as_slice()[y * width / 440 * width + x * width / 440]
}

pub(super) fn verify_done_pointer_states(window: &PairingWindow) {
    let surface = window.window();
    let point = LogicalPosition::new(275.0, 435.0);
    let done_fill = || sample(&surface.take_snapshot().unwrap(), 275, 435);
    surface.dispatch_event(WindowEvent::PointerMoved {
        position: LogicalPosition::new(10.0, 10.0),
    });
    let rest = done_fill();
    surface.dispatch_event(WindowEvent::PointerMoved { position: point });
    let hover = done_fill();
    assert_ne!(rest, hover);
    surface.dispatch_event(WindowEvent::PointerPressed {
        position: point,
        button: PointerEventButton::Left,
    });
    assert_eq!(done_fill(), hover);
    let outside = LogicalPosition::new(10.0, 435.0);
    surface.dispatch_event(WindowEvent::PointerMoved { position: outside });
    surface.dispatch_event(WindowEvent::PointerReleased {
        position: outside,
        button: PointerEventButton::Left,
    });
    assert_eq!(done_fill(), rest);
}
