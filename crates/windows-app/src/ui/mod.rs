use std::cell::RefCell;

use slint::winit_030::WinitWindowAccessor;
use slint::{CloseRequestResponse, ComponentHandle, PlatformError};

use crate::{WelcomeWindow, platform};

thread_local! {
    static WELCOME: RefCell<Option<WelcomeWindow>> = const { RefCell::new(None) };
}

pub fn open() {
    if let Err(error) = show() {
        eprintln!("Could not open Welcome: {error}");
    }
}

pub fn close() {
    WELCOME.with_borrow_mut(|welcome| *welcome = None);
}

pub fn update_motion() {
    WELCOME.with_borrow(|welcome| {
        if let Some(window) = welcome {
            let enabled = platform::animations_enabled();
            if enabled && !window.get_motion_enabled() {
                window.invoke_restart_motion();
            }
            window.set_motion_enabled(enabled);
        }
    });
}

fn show() -> Result<(), PlatformError> {
    let window = WELCOME.with_borrow_mut(|welcome| {
        if let Some(window) = welcome {
            return Ok(window.clone_strong());
        }
        let window = WelcomeWindow::new()?;
        window.set_chinese(platform::chinese());
        window
            .window()
            .on_close_requested(|| CloseRequestResponse::HideWindow);
        *welcome = Some(window.clone_strong());
        Ok::<_, PlatformError>(window)
    })?;
    if !window.window().is_visible() {
        window.set_motion_enabled(platform::animations_enabled());
        window.invoke_restart_motion();
        window.show()?;
    }
    window.window().with_winit_window(|native| {
        if native.is_minimized() == Some(true) {
            native.set_minimized(false);
        }
        native.focus_window();
    });
    Ok(())
}
