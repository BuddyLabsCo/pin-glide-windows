use std::{
    cell::{Cell, RefCell},
    collections::BTreeMap,
    time::Duration,
};

use slint::{
    CloseRequestResponse, ComponentHandle, PlatformError, Timer, TimerMode,
    winit_030::WinitWindowAccessor,
};

use crate::{SmsWindow, platform, service::ReceivedSms};

#[cfg(test)]
mod tests;

thread_local! {
    static POPUPS: RefCell<BTreeMap<u64, (SmsWindow, Timer)>> = const { RefCell::new(BTreeMap::new()) };
    static NEXT_ID: Cell<u64> = const { Cell::new(0) };
}

pub(super) fn receive(message: ReceivedSms) {
    if platform::session_unlocked() {
        let _ = slint::invoke_from_event_loop(move || {
            let _ = slint::spawn_local(show(message, platform::chinese()));
        });
    }
}

async fn show(message: ReceivedSms, chinese: bool) -> Result<(), PlatformError> {
    let window = platform::new_popup()?;
    window.set_chinese(chinese);
    window.set_otp(message.sms.otp.as_str().into());
    window.set_sms(message.sms.body.into());
    window.set_sender_name(message.sender_name.into());
    let native = window.window().winit_window().await?;
    if !platform::session_unlocked() {
        return Ok(());
    }
    let native_error = |error| {
        PlatformError::OtherError(format!("Could not configure the SMS popup: {error}").into())
    };
    let owner = platform::prepare_popup(&native).map_err(native_error)?;
    window.show()?;
    if let Err(error) = platform::place_popup(&native) {
        let _ = window.hide();
        return Err(native_error(error));
    }

    let id = NEXT_ID.get();
    NEXT_ID.set(id + 1);
    let otp = zeroize::Zeroizing::new(message.sms.otp);
    window.on_copy_code(move || {
        if !otp.is_empty() {
            platform::write_unicode_clipboard(owner, &otp);
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
    POPUPS.with_borrow_mut(|popups| popups.insert(id, (window, timer)));
    Ok(())
}

fn dismiss(id: u64) {
    if let Some((window, _timer)) = POPUPS.with_borrow_mut(|popups| popups.remove(&id)) {
        let _ = window.hide();
    }
}

pub(super) fn close_all() {
    for (_, (window, _timer)) in POPUPS.take() {
        let _ = window.hide();
    }
}
