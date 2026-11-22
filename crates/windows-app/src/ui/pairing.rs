use std::{cell::RefCell, sync::Arc};

use crate::service::{PairingOperation, PairingOutcome, PairingProgress, PairingStart, Service};
use crate::storage::SenderNameError;
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
    if PairingOperation::has_pending_enrollment() {
        create_view(chinese, start_operation, false)?;
    }
    Ok(())
}

pub fn open_pairing(
    chinese: bool,
    start: impl Fn(fn(), PairingStart) -> PairingOperation + 'static,
) -> Result<PairingWindow, PlatformError> {
    if reopen()? {
        return Ok(PAIRING.with_borrow(|pairing| pairing.as_ref().unwrap().window.clone_strong()));
    }
    create_view(chinese, start, true)
}

fn create_view(
    chinese: bool,
    start: impl Fn(fn(), PairingStart) -> PairingOperation + 'static,
    visible: bool,
) -> Result<PairingWindow, PlatformError> {
    let window = PairingWindow::new()?;
    window.set_chinese(chinese);
    window.set_motion_enabled(platform::animations_enabled());
    window.set_stage(PairingStage::Preparing);
    window
        .window()
        .on_close_requested(|| CloseRequestResponse::HideWindow);
    window.on_refresh(refresh);
    window.on_finish_naming(finish_naming);
    window.on_read_name_paste(move || read_name_paste(chinese));
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
    PAIRING.with_borrow_mut(|pairing| {
        let mut view = View {
            window: window.clone_strong(),
            operation: start(notify_progress, PairingStart::ResumePending),
            rendered_code: None,
            start: Box::new(move || start(notify_progress, PairingStart::Fresh)),
            named: false,
        };
        view.render();
        *pairing = Some(view);
    });
    Ok(window)
}

pub(super) fn reopen() -> Result<bool, PlatformError> {
    reopen_view(false)
}

pub(super) fn reopen_pending_name() -> Result<bool, PlatformError> {
    reopen_view(true)
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

pub(super) fn shutdown() {
    if let Some(view) = PAIRING.with_borrow_mut(Option::take) {
        view.operation.shutdown();
    }
}

fn start_operation(notify: fn(), start: PairingStart) -> PairingOperation {
    #[cfg(test)]
    if let Some(service) = tests::connect() {
        return PairingOperation::start(move || Some(service), notify, start);
    }
    let connect =
        || option_env!("PINGLIDE_SERVICE_URL").and_then(|origin| Service::new(origin).ok());
    PairingOperation::start(connect, notify, start)
}

fn notify_progress() {
    let _ = slint::invoke_from_event_loop(|| {
        PAIRING.with_borrow_mut(|pairing| {
            if let Some(view) = pairing {
                view.render();
            }
        });
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

fn read_name_paste(chinese: bool) -> NamePaste {
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

fn insert_name_paste(window: &slint::Window, text: slint::SharedString) {
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
            Ok(()) => match super::show_registered(store, view.window.get_chinese()) {
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

fn name_error(chinese: bool, error: SenderNameError) -> &'static str {
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
