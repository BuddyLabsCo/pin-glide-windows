use std::cell::RefCell;

use slint::winit_030::WinitWindowAccessor;
use slint::{CloseRequestResponse, ComponentHandle, PlatformError, VecModel, Window};

use crate::{MainWindow, WelcomeWindow, platform, storage::RegistrationStore};

mod pairing;

pub use pairing::open_pairing;

pub(crate) fn resume_pairing() -> Result<(), PlatformError> {
    pairing::resume(platform::chinese())
}

thread_local! {
    static WELCOME: RefCell<Option<WelcomeWindow>> = const { RefCell::new(None) };
    static MAIN: RefCell<Option<MainWindow>> = const { RefCell::new(None) };
}

pub fn open() {
    if let Err(error) = show() {
        eprintln!("Could not open Pin Glide: {error}");
    }
}

pub fn open_settings() {
    let result = (|| {
        if pairing::reopen_pending_name()? {
            return hide_welcome();
        }
        if let Ok(store) = RegistrationStore::for_current_user()
            && show_registered(&store, platform::chinese())?
        {
            return pairing::hide();
        }
        show_welcome().and_then(|()| pairing::hide())
    })();
    if let Err(error) = result {
        eprintln!("Could not open Pin Glide: {error}");
    }
}

pub fn close() {
    pairing::shutdown();
    WELCOME.with_borrow_mut(|welcome| *welcome = None);
    MAIN.with_borrow_mut(|main| *main = None);
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
        return hide_welcome();
    }
    if let Ok(store) = RegistrationStore::for_current_user()
        && show_registered(&store, platform::chinese())?
    {
        return Ok(());
    }
    show_welcome()
}

fn hide_welcome() -> Result<(), PlatformError> {
    WELCOME.with_borrow(|welcome| welcome.as_ref().map_or(Ok(()), ComponentHandle::hide))
}

fn show_registered(store: &RegistrationStore, chinese: bool) -> Result<bool, PlatformError> {
    let Ok(names) = store.sender_names() else {
        return Ok(false);
    };
    if names.is_empty() {
        return Ok(false);
    }
    let window = MAIN.with_borrow_mut(|main| {
        if let Some(window) = main {
            return Ok(window.clone_strong());
        }
        let window = MainWindow::new()?;
        window.set_chinese(chinese);
        window
            .window()
            .on_close_requested(|| CloseRequestResponse::HideWindow);
        *main = Some(window.clone_strong());
        Ok::<_, PlatformError>(window)
    })?;
    let names = names.into_values().map(Into::into).collect::<Vec<_>>();
    window.set_sender_names(VecModel::from_slice(&names));
    window.show()?;
    platform::configure_main_chrome(window.window());
    focus(window.window());
    Ok(true)
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
