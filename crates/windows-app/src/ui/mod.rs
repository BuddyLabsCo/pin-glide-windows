use std::cell::RefCell;

use slint::winit_030::WinitWindowAccessor;
use slint::{CloseRequestResponse, ComponentHandle, Model, PlatformError, VecModel, Window};

use crate::service::{DeliveryStatus, PairingOperation, PairingStart, Service};
use crate::storage::{RegistrationStore, SenderDevice, SenderNameError};
use crate::{MainWindow, SenderRowData, WelcomeWindow, platform};

mod pairing;
mod settings;
mod sms;
#[cfg(test)]
pub(crate) use sms::{popup_count, receive as receive_sms};

pub(crate) use pairing::connect_service;
pub use pairing::{open_pairing, visible_pairing};
pub(crate) use settings::initialize as initialize_preferences;
pub use settings::settings_visible;
pub(crate) use settings::{finish_shortcut_capture, restore_shortcut};
pub use sms::show_sms;

pub(crate) fn resume_pairing(connect: fn() -> Option<Service>) -> Result<(), PlatformError> {
    if let Ok(store) = RegistrationStore::for_current_user() {
        if recovery_status(&store).is_none()
            && store.enrollment().is_ok_and(|entry| entry.is_some())
        {
            return pairing::resume(platform::chinese());
        }
        if store.has_registration() {
            start_registered(store, connect);
        }
    }
    Ok(())
}

thread_local! {
    static WELCOME: RefCell<Option<WelcomeWindow>> = const { RefCell::new(None) };
    static MAIN: RefCell<Option<MainWindow>> = const { RefCell::new(None) };
    static REGISTERED: RefCell<Option<PairingOperation>> = const { RefCell::new(None) };
}

pub fn open() {
    if let Err(error) = show() {
        eprintln!("Could not open Pin Glide: {error}");
    }
}

pub fn open_settings() {
    let store = pairing::registration().or_else(|| RegistrationStore::for_current_user().ok());
    if let Err(error) = open_settings_with(store) {
        eprintln!("Could not open Settings: {error}");
    }
}

fn open_settings_with(store: Option<RegistrationStore>) -> Result<(), PlatformError> {
    if let Some(store) = store
        && let Some(main) =
            registered_window(&store, platform::chinese(), pairing::connect_service)?
    {
        settings::show(&main, &store)?;
        hide_welcome()?;
        return pairing::hide();
    }
    if pairing::reopen_pending_name()? {
        return hide_welcome();
    }
    show_welcome().and_then(|()| pairing::hide())
}

pub fn close() {
    pairing::shutdown();
    if let Some(operation) = REGISTERED.take() {
        operation.shutdown();
    }
    close_sms();
    settings::close();
    WELCOME.with_borrow_mut(|welcome| *welcome = None);
    MAIN.with_borrow_mut(|main| *main = None);
}

pub(crate) fn close_sms() {
    sms::close_all();
}

pub(crate) fn latest_otp() -> Option<zeroize::Zeroizing<String>> {
    REGISTERED.with_borrow(|operation| match operation {
        Some(operation) => operation.latest_otp(),
        None => pairing::latest_otp(),
    })
}

fn update_delivery_status() {
    let status = REGISTERED
        .with_borrow(|operation| operation.as_ref().map(PairingOperation::delivery_status))
        .or_else(pairing::delivery_status)
        .unwrap_or(DeliveryStatus::Unregistered);
    platform::set_delivery_status(status);
}

fn registered_revoked() -> bool {
    REGISTERED.with_borrow(|operation| {
        operation
            .as_ref()
            .is_some_and(|operation| operation.delivery_status() == DeliveryStatus::Revoked)
    })
}

fn recovery_status(store: &RegistrationStore) -> Option<DeliveryStatus> {
    if registered_revoked() || store.receiver_revoked() {
        Some(DeliveryStatus::Revoked)
    } else if store.has_registration() && store.receiver_credential().is_err() {
        Some(DeliveryStatus::MissingCredential)
    } else {
        None
    }
}

fn refresh_delivery_ui() {
    sms::close_invalid();
    let store = REGISTERED
        .with_borrow(|operation| operation.as_ref()?.registration().cloned())
        .or_else(pairing::registration);
    if let Some(store) = store {
        refresh_visible_main(&store);
    }
    update_delivery_status();
}

fn start_registered(
    store: RegistrationStore,
    connect: impl FnOnce() -> Option<Service> + Send + 'static,
) {
    let operation = PairingOperation::spawn(
        connect,
        pairing::notify_progress,
        Some(store),
        PairingStart::Registered,
        sms::receive,
    );
    replace_registered(Some(operation));
    update_delivery_status();
}

fn replace_registered(next: Option<PairingOperation>) {
    let previous = REGISTERED.replace(next);
    if let Some(operation) = previous {
        operation.shutdown();
    }
}

pub fn open_registered(
    chinese: bool,
    path: std::path::PathBuf,
    connect: impl Fn() -> Option<Service> + Clone + Send + 'static,
) -> Result<Option<MainWindow>, PlatformError> {
    let store = RegistrationStore::new(path);
    let window = show_registered_with(&store, chinese, connect.clone())?;
    if window.is_some() {
        start_registered(store, connect);
    }
    Ok(window)
}

pub fn delivery_status() -> DeliveryStatus {
    platform::delivery_status()
}

pub fn update_motion() {
    settings::update_motion();
    pairing::update_motion();
    sms::refresh_layout(false);
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

pub(crate) fn refresh_preferences() {
    let chinese = platform::chinese();
    platform::refresh_tooltip();
    WELCOME.with_borrow(|welcome| {
        if let Some(window) = welcome {
            window.set_chinese(chinese);
        }
    });
    MAIN.with_borrow(|main| {
        if let Some(window) = main
            && window.get_chinese() != chinese
        {
            window.set_chinese(chinese);
            window.invoke_clear_editor_error();
        }
    });
    pairing::update_language(chinese);
    sms::update_language(chinese);
    settings::refresh(chinese);
}

pub(crate) fn resize_sms_popups() {
    sms::refresh_layout(true);
}

fn show() -> Result<(), PlatformError> {
    settings::hide()?;
    let main_visible = MAIN.with_borrow(|main| {
        main.as_ref()
            .is_some_and(|window| window.window().is_visible())
    });
    if !main_visible && pairing::reopen()? {
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
    show_registered_with(store, chinese, pairing::connect_service).map(|window| window.is_some())
}

fn show_registered_with(
    store: &RegistrationStore,
    chinese: bool,
    connect: impl Fn() -> Option<Service> + Clone + Send + 'static,
) -> Result<Option<MainWindow>, PlatformError> {
    let window = registered_window(store, chinese, connect)?;
    if let Some(window) = &window {
        show_main(window)?;
    }
    Ok(window)
}

fn registered_window(
    store: &RegistrationStore,
    chinese: bool,
    connect: impl Fn() -> Option<Service> + Clone + Send + 'static,
) -> Result<Option<MainWindow>, PlatformError> {
    if !store.has_registration() && !registered_revoked() {
        return Ok(None);
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
        let weak = window.as_weak();
        let pairing_store = store.clone();
        window.on_open_pairing(move || {
            let Some(window) = weak.upgrade() else { return };
            let adding = recovery_status(&pairing_store).is_none() && !pairing::enrolling();
            let result = pairing::open_from_main(
                window.get_chinese(),
                &pairing_store,
                connect.clone(),
                adding,
            );
            match result {
                Ok(pairing) if pairing.window().is_visible() => {
                    let _ = window.hide();
                }
                Ok(_) => {}
                Err(error) => eprintln!("Could not open pairing: {error}"),
            }
        });
        let weak = window.as_weak();
        let store = store.clone();
        window.on_open_settings(move || {
            if let Some(main) = weak.upgrade()
                && let Err(error) = settings::show(&main, &store)
            {
                eprintln!("Could not open Settings: {error}");
            }
        });
        *main = Some(window.clone_strong());
        Ok::<_, PlatformError>(window)
    })?;
    refresh_main(&window, store);
    register_sender_actions(&window, store.clone());
    Ok(Some(window))
}

fn show_main(window: &MainWindow) -> Result<(), PlatformError> {
    window.show()?;
    platform::configure_main_chrome(window.window());
    focus(window.window());
    Ok(())
}

fn refresh_visible_main(store: &RegistrationStore) {
    MAIN.with_borrow(|main| {
        if let Some(window) = main
            && window.window().is_visible()
        {
            refresh_main(window, store);
        }
    });
}

fn refresh_main(window: &MainWindow, store: &RegistrationStore) {
    let recovery = recovery_status(store);
    window.set_receiver_revoked(recovery == Some(DeliveryStatus::Revoked));
    window.set_receiver_credential_missing(recovery == Some(DeliveryStatus::MissingCredential));
    let devices = if recovery.is_some() {
        Vec::new()
    } else {
        store.sender_devices().unwrap_or_default()
    };
    set_sender_devices(window, devices);
}

fn set_sender_devices(window: &MainWindow, devices: Vec<SenderDevice>) {
    let rows = devices
        .into_iter()
        .map(|device| SenderRowData {
            pairing_id: device.pairing_id.into(),
            name: device.name.into(),
            needs_repair: device.needs_repair,
        })
        .collect::<Vec<_>>();
    if !window.get_sender_devices().iter().eq(rows.iter().cloned()) {
        window.invoke_close_editor();
        window.set_sender_devices(VecModel::from_slice(&rows));
    }
}

fn register_sender_actions(window: &MainWindow, store: RegistrationStore) {
    let weak = window.as_weak();
    window.on_read_name_paste(move || {
        weak.upgrade().map_or_else(Default::default, |window| {
            pairing::read_name_paste(window.get_chinese())
        })
    });
    let weak = window.as_weak();
    window.on_insert_name_paste(move |text| {
        if let Some(window) = weak.upgrade() {
            pairing::insert_name_paste(window.window(), text);
        }
    });

    let rename_store = store.clone();
    let weak = window.as_weak();
    window.on_rename_sender(move |index, draft, invalid| {
        let Some(window) = weak.upgrade() else {
            return Default::default();
        };
        let result = (|| {
            if invalid {
                return Err(SenderNameError::InvalidCharacter);
            }
            let rows = window.get_sender_devices();
            let index = index as usize;
            let mut row = rows.row_data(index).ok_or(SenderNameError::Storage)?;
            let name = rename_store
                .name_sender(&row.pairing_id, &draft.encode_utf16().collect::<Vec<_>>())?;
            row.name = name.into();
            rows.set_row_data(index, row);
            Ok(())
        })();
        result.err().map_or_else(Default::default, |error| {
            pairing::name_error(window.get_chinese(), error).into()
        })
    });

    let weak = window.as_weak();
    window.on_delete_sender(move |index, pointer| {
        let Some(window) = weak.upgrade() else {
            return Default::default();
        };
        if let Some(row) = window.get_sender_devices().row_data(index as usize)
            && store.delete_unreadable_pairing(&row.pairing_id).is_ok()
        {
            window.invoke_close_editor();
            refresh_visible_main(&store);
            let weak = window.as_weak();
            let _ = slint::invoke_from_event_loop(move || {
                if let Some(window) = weak.upgrade() {
                    window.invoke_focus_first_sender(pointer);
                }
            });
            update_delivery_status();
            return "".into();
        }
        if window.get_chinese() {
            "无法删除设备，请重试。".into()
        } else {
            "Could not delete the device. Try again.".into()
        }
    });
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
