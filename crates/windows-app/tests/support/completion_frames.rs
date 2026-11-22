use std::{path::Path, time::Duration};

use i_slint_core::animations::update_animations;
use slint::{
    ComponentHandle, LogicalPosition, LogicalSize, Rgba8Pixel, SharedPixelBuffer,
    platform::{PointerEventButton, WindowEvent},
};
use windows_app::{PairingStage, PairingWindow};

fn transition_window() -> PairingWindow {
    let window = PairingWindow::new().unwrap();
    window.window().set_size(LogicalSize::new(440.0, 560.0));
    window.set_completion_motion(true);
    window.set_stage(PairingStage::Connecting);
    window
}

pub(super) fn verify_transition(evidence: Option<&Path>) {
    let moving = transition_window();
    moving.set_motion_enabled(true);
    let loading = transition_window();
    loading.set_motion_enabled(true);
    let reduced = transition_window();
    let start = Duration::from_millis(830).into();
    update_animations(start);
    moving.window().take_snapshot().unwrap();
    reduced.window().take_snapshot().unwrap();
    moving.set_stage(PairingStage::Completed);
    moving.window().take_snapshot().unwrap();
    reduced.set_stage(PairingStage::Completed);
    let reduced_first = reduced.window().take_snapshot().unwrap();

    update_animations(start + Duration::from_millis(90));
    let halfway = moving.window().take_snapshot().unwrap();
    let loading = loading.window().take_snapshot().unwrap();
    if let Some(directory) = evidence {
        super::capture(&moving, &directory.join("pair-done-transition-1.ppm"));
    }
    update_animations(start + Duration::from_millis(165));
    let before_end = moving.window().take_snapshot().unwrap();
    if let Some(directory) = evidence {
        super::capture(&moving, &directory.join("pair-done-transition-2.ppm"));
    }
    update_animations(start + Duration::from_millis(195));
    let settled = moving.window().take_snapshot().unwrap();
    assert_eq!(sample(&settled, 220, 160).r, 227);
    assert_ne!(sample(&before_end, 220, 160), sample(&settled, 220, 160));
    assert!(settled.as_slice() == reduced_first.as_slice());

    let backing = sample(&halfway, 220, 160).r;
    assert!(backing.abs_diff(241) <= 2);
    // At half opacity, the software renderer blends the tile base and the peak pulse in order.
    for (x, y) in [(232, 172), (216, 188)] {
        let pulse = sample(&loading, x, y).r;
        let middle = sample(&halfway, x, y).r;
        assert_eq!(pulse, 142);
        assert_eq!(sample(&settled, x, y).r, 227);
        let expected = (u16::from(backing) + 234 + 2 * u16::from(pulse)) / 4;
        assert!(
            u16::from(middle).abs_diff(expected) <= 3,
            "expected loading tile over the badge at ({x}, {y}): {middle} vs {expected}",
        );
    }
}

fn sample(pixels: &SharedPixelBuffer<Rgba8Pixel>, x: usize, y: usize) -> Rgba8Pixel {
    let width = pixels.width() as usize;
    let height = pixels.height() as usize;
    pixels.as_slice()[y * height / 560 * width + x * width / 440]
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
