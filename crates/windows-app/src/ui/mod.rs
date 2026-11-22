use std::cell::RefCell;

use slint::winit_030::WinitWindowAccessor;
use slint::{CloseRequestResponse, ComponentHandle, PlatformError, Window};

use crate::{WelcomeWindow, platform};

mod pairing;

pub use pairing::open_pairing;

pub(crate) fn resume_pairing() -> Result<(), PlatformError> {
    pairing::resume(platform::chinese())
}

thread_local! {
    static WELCOME: RefCell<Option<WelcomeWindow>> = const { RefCell::new(None) };
}

pub fn open() {
    if let Err(error) = show() {
        eprintln!("Could not open Welcome: {error}");
    }
}

pub fn open_settings() {
    if let Err(error) = show_welcome().and_then(|()| pairing::hide()) {
        eprintln!("Could not open Welcome: {error}");
    }
}

pub fn close() {
    pairing::shutdown();
    WELCOME.with_borrow_mut(|welcome| *welcome = None);
}

pub fn update_motion() {
    pairing::update_motion();
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
    if pairing::reopen()? {
        WELCOME.with_borrow(|welcome| match welcome {
            Some(window) => window.hide(),
            None => Ok(()),
        })?;
        return Ok(());
    }
    show_welcome()
}

fn show_welcome() -> Result<(), PlatformError> {
    let window = WELCOME.with_borrow_mut(|welcome| {
        if let Some(window) = welcome {
            return Ok(window.clone_strong());
        }
        let window = WelcomeWindow::new()?;
        window.set_chinese(platform::chinese());
        window
            .window()
            .on_close_requested(|| CloseRequestResponse::HideWindow);
        let welcome_window = window.as_weak();
        window.on_start_pairing(move || {
            if let Some(window) = welcome_window.upgrade() {
                match pairing::open(window.get_chinese()) {
                    Ok(()) => {
                        let _ = window.hide();
                    }
                    Err(error) => eprintln!("Could not open pairing: {error}"),
                }
            }
        });
        *welcome = Some(window.clone_strong());
        Ok::<_, PlatformError>(window)
    })?;
    if !window.window().is_visible() {
        window.set_motion_enabled(platform::animations_enabled());
        window.invoke_restart_motion();
        window.show()?;
    }
    focus(window.window());
    Ok(())
}

fn focus(window: &Window) {
    window.with_winit_window(|native| {
        if native.is_minimized() == Some(true) {
            native.set_minimized(false);
        }
        native.focus_window();
    });
}
