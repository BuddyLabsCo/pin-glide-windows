use crate::{platform, service::ReceivedSms};

use super::{LayoutMotion, Popup, PopupSource, STACK, Stack, clear, now, show_sms_for_session};

pub(crate) fn receive(message: ReceivedSms) {
    if eligible(message.epoch, Some(&message.source)) {
        let _ = slint::invoke_from_event_loop(move || {
            let _ = slint::spawn_local(show_sms_for_session(
                message.sms,
                message.sender_name,
                platform::chinese(),
                message.epoch,
                Some(message.source),
            ));
        });
    }
}

pub(super) fn eligible(epoch: u64, source: Option<&PopupSource>) -> bool {
    platform::popup_epoch() == Some(epoch) && source.is_none_or(PopupSource::is_active)
}

fn active(popup: &Popup) -> bool {
    popup.source.as_ref().is_none_or(PopupSource::is_active)
}

pub(super) fn can_copy(stack: &Stack, id: u64) -> bool {
    stack.visible.get(&id).is_some_and(active)
}

pub(in crate::ui) fn close_invalid() {
    STACK.with_borrow_mut(|stack| {
        let before = stack.visible.len();
        let retain = |popup: &Popup| {
            if active(popup) {
                return true;
            }
            popup.expiry.stop();
            clear(&popup.window);
            false
        };
        stack.visible.retain(|_, popup| retain(popup));
        stack.leaving.retain(retain);
        let animate = platform::animations_enabled();
        if before != stack.visible.len() {
            let _ = stack.layout(None, LayoutMotion::for_stack(animate), now());
        }
        stack.schedule_frame(animate);
    });
}
