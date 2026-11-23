use std::{cell::RefCell, sync::Arc};

use crate::service::{
    DeliveryStatus, PairingOperation, PairingOutcome, PairingProgress, PairingStart, Service,
};
use crate::storage::{RegistrationStore, SenderNameError};
use crate::{NamePaste, PairingStage, PairingWindow, platform};
use i_slint_core::{
    input::{InternalKeyEvent, KeyEvent, KeyEventType},
    window::WindowInner,
};
use qrcode::{Color, QrCode};
use slint::{
    CloseRequestResponse, ComponentHandle, Image, PlatformError, Rgb8Pixel, SharedPixelBuffer,
};

#[cfg(test)]
mod tests;

thread_local! {
    static PAIRING: RefCell<Option<View>> = const { RefCell::new(None) };
}

struct View {
    window: PairingWindow,
    operation: PairingOperation,
    rendered_code: Option<Arc<QrCode>>,
    start: Box<dyn Fn() -> PairingOperation>,
    named: bool,
}

pub(super) fn open(chinese: bool) -> Result<(), PlatformError> {
    open_pairing(chinese, start_operation).map(drop)
}

pub(super) fn resume(chinese: bool) -> Result<(), PlatformError> {
    create_view(chinese, start_operation, false, false)?;
    Ok(())
}

pub fn open_pairing(
    chinese: bool,
    start: impl Fn(fn(), PairingStart) -> PairingOperation + 'static,
) -> Result<PairingWindow, PlatformError> {
    if reopen()? {
        return Ok(PAIRING.with_borrow(|pairing| pairing.as_ref().unwrap().window.clone_strong()));
    }
    create_view(chinese, start, true, false)
}

pub fn visible_pairing() -> Option<PairingWindow> {
    PAIRING.with_borrow(|view| {
        view.as_ref()
            .filter(|view| view.window.window().is_visible())
            .map(|view| view.window.clone_strong())
    })
}

pub(super) fn enrolling() -> bool {
    PAIRING.with_borrow(|view| {
        view.as_ref().is_some_and(|view| {
            !view.window.get_adding_device()
                && !matches!(view.operation.current(), PairingProgress::Completed { .. })
        })
    })
}

pub(super) fn open_from_main(
    chinese: bool,
    store: &RegistrationStore,
    connect: impl Fn() -> Option<Service> + Clone + Send + 'static,
    adding: bool,
) -> Result<PairingWindow, PlatformError> {
    let resumable = PAIRING.with_borrow_mut(|view| {
        view.as_mut().is_some_and(|view| {
            view.render();
            view.window.get_adding_device() == adding && !view.named
        })
    });
    if resumable && reopen()? {
        return Ok(PAIRING.with_borrow(|view| view.as_ref().unwrap().window.clone_strong()));
    }
    let store = store.clone();
    create_view(
        chinese,
        move |notify, start| {
            PairingOperation::spawn(
                connect.clone(),
                notify,
                Some(store.clone()),
                if adding {
                    PairingStart::AddSender
                } else {
                    start
                },
                if adding { drop } else { super::sms::receive },
            )
        },
        true,
        adding,
    )
}

fn create_view(
    chinese: bool,
    start: impl Fn(fn(), PairingStart) -> PairingOperation + 'static,
    visible: bool,
    adding_device: bool,
) -> Result<PairingWindow, PlatformError> {
    let window = PairingWindow::new()?;
    window.set_chinese(chinese);
    window.set_adding_device(adding_device);
    window.set_motion_enabled(platform::animations_enabled());
    window.set_stage(PairingStage::Preparing);
    window
        .window()
        .on_close_requested(|| CloseRequestResponse::HideWindow);
    window.on_refresh(refresh);
    window.on_back(back);
    window.on_finish_naming(finish_naming);
    let weak = window.as_weak();
    window.on_read_name_paste(move || {
        weak.upgrade().map_or_else(Default::default, |window| {
            read_name_paste(window.get_chinese())
        })
    });
    let weak = window.as_weak();
    window.on_insert_name_paste(move |text| {
        if let Some(window) = weak.upgrade() {
            insert_name_paste(window.window(), text);
        }
    });
    if visible {
        window.show()?;
        super::focus(window.window());
    }
    let previous = PAIRING.with_borrow_mut(|pairing| {
        let mut view = View {
            window: window.clone_strong(),
            operation: start(notify_progress, PairingStart::ResumePending),
            rendered_code: None,
            start: Box::new(move || start(notify_progress, PairingStart::Fresh)),
            named: false,
        };
        view.render();
        pairing.replace(view)
    });
    if let Some(previous) = previous {
        let _ = previous.window.hide();
        if adding_device && !previous.window.get_adding_device() {
            super::replace_registered(Some(previous.operation));
        } else {
            previous.operation.shutdown();
        }
    }
    Ok(window)
}

pub(super) fn reopen() -> Result<bool, PlatformError> {
    reopen_view(false)
}

pub(super) fn reopen_pending_name() -> Result<bool, PlatformError> {
    reopen_view(true)
}

pub(super) fn registration() -> Option<RegistrationStore> {
    PAIRING.with_borrow(|pairing| pairing.as_ref()?.operation.registration().cloned())
}

fn reopen_view(pending_name_only: bool) -> Result<bool, PlatformError> {
    PAIRING.with_borrow_mut(|pairing| {
        let Some(view) = pairing else {
            return Ok(false);
        };
        view.render();
        if pending_name_only && (view.named || view.window.get_stage() != PairingStage::Completed) {
            return Ok(false);
        }
        if view.named {
            if let Some(store) = view.operation.registration() {
                return super::show_registered(store, view.window.get_chinese());
            }
            return Ok(false);
        }
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

pub(super) fn update_language(chinese: bool) {
    PAIRING.with_borrow(|pairing| {
        if let Some(view) = pairing
            && view.window.get_chinese() != chinese
        {
            view.window.set_chinese(chinese);
            view.window.set_name_error(Default::default());
        }
    });
}

pub(super) fn shutdown() {
    if let Some(view) = PAIRING.with_borrow_mut(Option::take) {
        view.operation.shutdown();
    }
}

fn start_operation(notify: fn(), start: PairingStart) -> PairingOperation {
    PairingOperation::start_receiving(operation_service(), notify, start, super::sms::receive)
}

fn operation_service() -> impl FnOnce() -> Option<Service> + Send {
    #[cfg(test)]
    let fixture = tests::connect();
    move || {
        #[cfg(test)]
        if fixture.is_some() {
            return fixture;
        }
        connect_service()
    }
}

pub(super) fn connect_service() -> Option<Service> {
    option_env!("PINGLIDE_SERVICE_URL").and_then(|origin| Service::new(origin).ok())
}

pub(super) fn delivery_status() -> Option<DeliveryStatus> {
    PAIRING.with_borrow(|view| view.as_ref().map(|view| view.operation.delivery_status()))
}

pub(super) fn latest_otp() -> Option<zeroize::Zeroizing<String>> {
    PAIRING.with_borrow(|view| view.as_ref()?.operation.latest_otp())
}

pub(super) fn notify_progress() {
    let _ = slint::invoke_from_event_loop(|| {
        PAIRING.with_borrow_mut(|pairing| {
            if let Some(view) = pairing {
                view.render();
                if !view.window.get_adding_device()
                    && view.window.get_stage() == PairingStage::Completed
                {
                    super::replace_registered(None);
                }
            }
        });
        super::update_delivery_status();
    });
}

fn back() {
    PAIRING.with_borrow_mut(|pairing| {
        let Some(view) = pairing else { return };
        let Some(store) = view.operation.registration() else {
            return;
        };
        if let Ok(true) = super::show_registered(store, view.window.get_chinese()) {
            let _ = view.window.hide();
            view.render();
            view.named |= view.window.get_stage() == PairingStage::Completed;
        }
    });
}

fn refresh() {
    PAIRING.with_borrow_mut(|pairing| {
        let Some(view) = pairing else { return };
        if matches!(view.operation.current(), PairingProgress::Finished(_)) {
            view.operation = (view.start)();
            view.render();
        }
    });
}

pub(super) fn read_name_paste(chinese: bool) -> NamePaste {
    let mut pasted = NamePaste::default();
    match platform::read_unicode_clipboard() {
        Ok(Some(text)) => {
            pasted.text = text.replace("\r\n", " ").replace(['\r', '\n'], " ").into();
        }
        Ok(None) => {}
        Err(platform::ClipboardError::InvalidText) => {
            pasted.invalid_text = true;
            pasted.error = name_error(chinese, SenderNameError::InvalidCharacter).into();
        }
        Err(platform::ClipboardError::Unavailable) => {
            pasted.error = if chinese {
                "无法读取剪贴板，请重试。"
            } else {
                "Could not read the clipboard. Try again."
            }
            .into();
        }
    }
    pasted
}

pub(super) fn insert_name_paste(window: &slint::Window, text: slint::SharedString) {
    let inner = WindowInner::from_pub(window);
    let field = inner.focus_item.borrow().upgrade();
    if let Some(field) = field {
        let mut key_event = KeyEvent::default();
        key_event.text = text;
        // Route the validated text through the editor without normalizing pasted Unicode.
        field.borrow().as_ref().key_event(
            &InternalKeyEvent {
                key_event,
                event_type: KeyEventType::CommitComposition,
                ..Default::default()
            },
            &inner.window_adapter(),
            &field,
        );
    }
}

fn finish_naming() {
    PAIRING.with_borrow_mut(|pairing| {
        let Some(view) = pairing else { return };
        if view.named {
            return;
        }
        let PairingProgress::Completed { pairing_id } = view.operation.current() else {
            return;
        };
        let Some(store) = view.operation.registration() else {
            return;
        };
        let input = view.window.get_sender_name();
        let result = if view.window.get_invalid_paste() {
            Err(SenderNameError::InvalidCharacter)
        } else {
            store.name_sender(&pairing_id, &input.encode_utf16().collect::<Vec<_>>())
        };
        let error = match result {
            Ok(_) => match super::show_registered(store, view.window.get_chinese()) {
                Ok(true) => {
                    view.named = true;
                    let _ = view.window.hide();
                    return;
                }
                _ => SenderNameError::Storage,
            },
            Err(error) => error,
        };
        view.window
            .set_name_error(name_error(view.window.get_chinese(), error).into());
        view.window.invoke_focus_name();
    });
}

pub(super) fn name_error(chinese: bool, error: SenderNameError) -> &'static str {
    match (chinese, error) {
        (true, SenderNameError::Empty) => "请填写设备名称。",
        (true, SenderNameError::TooLong) => "设备名称最多 40 个字符。",
        (true, SenderNameError::InvalidCharacter) => "名称包含无效字符，请重新输入。",
        (true, SenderNameError::Duplicate) => "已有一台同名设备，请换一个名称。",
        (true, SenderNameError::Storage) => "无法保存设备名称，请重试。",
        (false, SenderNameError::Empty) => "Enter a device name.",
        (false, SenderNameError::TooLong) => "Use at most 40 characters.",
        (false, SenderNameError::InvalidCharacter) => "The name contains an invalid character.",
        (false, SenderNameError::Duplicate) => "A device already uses this name. Choose another.",
        (false, SenderNameError::Storage) => "Could not save the device name. Try again.",
    }
}

impl View {
    fn render(&mut self) {
        let progress = self.operation.current();
        let stage = match &progress {
            PairingProgress::Preparing => PairingStage::Preparing,
            PairingProgress::Waiting { code, seconds } => {
                if !self
                    .rendered_code
                    .as_ref()
                    .is_some_and(|previous| Arc::ptr_eq(previous, code))
                {
                    self.window.set_qr(qr_image(code));
                    self.rendered_code = Some(Arc::clone(code));
                }
                self.window
                    .set_remaining(format!("{:02}:{:02}", seconds / 60, seconds % 60).into());
                PairingStage::Waiting
            }
            PairingProgress::Confirming => PairingStage::Confirming,
            PairingProgress::CredentialReady => PairingStage::CredentialReady,
            PairingProgress::Checking => PairingStage::Checking,
            PairingProgress::Connecting => PairingStage::Connecting,
            PairingProgress::Completed { .. } => PairingStage::Completed,
            PairingProgress::Finished(outcome) => match outcome {
                PairingOutcome::CreateFailed => PairingStage::CreateFailed,
                PairingOutcome::Failed => PairingStage::Failed,
                PairingOutcome::Expired => PairingStage::Expired,
                PairingOutcome::Canceled => PairingStage::Canceled,
                PairingOutcome::KeyConfirmed(_) => PairingStage::Failed,
            },
        };
        if stage != PairingStage::Waiting && self.rendered_code.take().is_some() {
            self.window.set_qr(Image::default());
        }
        if stage == PairingStage::Completed && self.window.get_stage() != stage {
            let visible = self.window.window().is_visible();
            self.named = !visible;
            self.window.set_completion_motion(visible);
            if !visible
                && self.window.get_adding_device()
                && let Some(store) = self.operation.registration()
            {
                super::refresh_visible_main(store);
            }
            if visible
                && let PairingProgress::Completed { pairing_id } = progress
                && let Ok(Some(name)) = self.operation.sender_label(&pairing_id)
            {
                self.window.set_sender_name(name.into());
            }
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
