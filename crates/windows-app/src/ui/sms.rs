use std::{
    cell::{Cell, RefCell},
    collections::BTreeMap,
    time::Duration,
};

use slint::{
    CloseRequestResponse, ComponentHandle, PlatformError, Timer, TimerMode,
    winit_030::WinitWindowAccessor,
};

use receiver_core::delivery::Sms;

use crate::{SmsWindow, platform, service::ReceivedSms};

thread_local! {
    static POPUPS: RefCell<BTreeMap<u64, (SmsWindow, Timer)>> = const { RefCell::new(BTreeMap::new()) };
    static NEXT_ID: Cell<u64> = const { Cell::new(0) };
}

pub(super) fn receive(message: ReceivedSms) {
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
    window.show()?;
    if let Err(error) = platform::place_popup(&native, owner) {
        let _ = window.hide();
        return Err(native_error(error));
    }
    if platform::popup_epoch() != Some(epoch) {
        clear(window);
        return Ok(None);
    }

    let id = NEXT_ID.get();
    NEXT_ID.set(id + 1);
    let popup = window.as_weak();
    window.on_copy_code(move || {
        if let Some(window) = popup.upgrade() {
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
        CloseRequestResponse::HideWindow
    });
    let timer = Timer::default();
    timer.start(TimerMode::SingleShot, Duration::from_secs(30), move || {
        dismiss(id)
    });
    POPUPS.with_borrow_mut(|popups| popups.insert(id, (window.clone_strong(), timer)));
    Ok(Some(window))
}

fn dismiss(id: u64) {
    if let Some((window, _timer)) = POPUPS.with_borrow_mut(|popups| popups.remove(&id)) {
        clear(window);
    }
}

pub(super) fn close_all() {
    for (_, (window, _timer)) in POPUPS.take() {
        clear(window);
    }
}

fn clear(window: SmsWindow) {
    let _ = window.hide();
    window.set_sms(Default::default());
    window.set_otp(Default::default());
}
