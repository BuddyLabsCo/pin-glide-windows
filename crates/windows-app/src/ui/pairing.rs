use std::{cell::RefCell, thread::JoinHandle, time::Duration};

use qrcode::{Color, QrCode};
use receiver_core::pairing::PairingMaterial;
use slint::{
    CloseRequestResponse, ComponentHandle, Image, PlatformError, Rgb8Pixel, SharedPixelBuffer,
    Timer, TimerMode,
};
use time::OffsetDateTime;
use tokio::{sync::oneshot, time::timeout};
use windows_app::service::{NewReceiverSession, Service, SessionState};
use zeroize::Zeroizing;

use crate::{PairingStage, PairingWindow, platform};

thread_local! {
    static PAIRING: RefCell<Option<View>> = const { RefCell::new(None) };
}

struct View {
    window: PairingWindow,
    ticker: Timer,
    deadline: Option<OffsetDateTime>,
    worker: Option<(oneshot::Sender<()>, JoinHandle<()>)>,
}

enum Update {
    Waiting(SharedPixelBuffer<Rgb8Pixel>, OffsetDateTime),
    Confirming,
    Finished(PairingStage),
}

pub(super) fn open(chinese: bool) -> Result<(), PlatformError> {
    let window = PairingWindow::new()?;
    window.set_chinese(chinese);
    window.set_motion_enabled(platform::animations_enabled());
    window.set_stage(PairingStage::Preparing);
    window
        .window()
        .on_close_requested(|| CloseRequestResponse::HideWindow);
    window.on_refresh(start);
    window.show()?;
    super::focus(window.window());
    PAIRING.with_borrow_mut(|pairing| {
        *pairing = Some(View {
            window,
            ticker: Timer::default(),
            deadline: None,
            worker: None,
        });
    });
    start();
    Ok(())
}

pub(super) fn reopen() -> Result<bool, PlatformError> {
    PAIRING.with_borrow(|pairing| {
        let Some(view) = pairing else {
            return Ok(false);
        };
        view.tick();
        view.window
            .set_motion_enabled(platform::animations_enabled());
        if !view.window.window().is_visible() {
            view.window.show()?;
        }
        super::focus(view.window.window());
        Ok(true)
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
    let view = PAIRING.with_borrow_mut(Option::take);
    if let Some((exit, worker)) = view.and_then(|view| view.worker) {
        let _ = exit.send(());
        let _ = worker.join();
    }
}

fn start() {
    PAIRING.with_borrow_mut(|pairing| {
        let Some(view) = pairing.as_mut() else { return };
        if view.worker.is_some() {
            return;
        }
        view.window.set_stage(PairingStage::Preparing);
        let (exit, exit_requested) = oneshot::channel();
        let worker = std::thread::Builder::new()
            .name("pairing".into())
            .spawn(move || {
                let stage = tokio::runtime::Builder::new_current_thread()
                    .enable_all()
                    .build()
                    .map_or(PairingStage::CreateFailed, |runtime| {
                        runtime.block_on(run(exit_requested))
                    });
                publish(Update::Finished(stage));
            });
        match worker {
            Ok(worker) => view.worker = Some((exit, worker)),
            Err(_) => view.window.set_stage(PairingStage::CreateFailed),
        }
    });
}

impl View {
    fn tick(&self) {
        if let Some(deadline) = self.deadline {
            let seconds =
                ((deadline - OffsetDateTime::now_utc()).whole_milliseconds() + 999) / 1000;
            if seconds <= 0 {
                self.window.set_qr(Image::default());
                self.window.set_stage(PairingStage::Expired);
            } else {
                self.window
                    .set_remaining(format!("{:02}:{:02}", seconds / 60, seconds % 60).into());
            }
        }
    }

    fn update(&mut self, update: Update) {
        match update {
            Update::Waiting(pixels, deadline) => {
                self.deadline = Some(deadline);
                self.window.set_qr(Image::from_rgb8(pixels));
                self.window.set_stage(PairingStage::Waiting);
                self.tick();
                self.ticker
                    .start(TimerMode::Repeated, Duration::from_millis(200), || {
                        PAIRING.with_borrow(|pairing| {
                            if let Some(view) = pairing {
                                view.tick();
                            }
                        });
                    });
            }
            Update::Confirming => {
                self.clear_qr();
                self.window.set_stage(PairingStage::Confirming);
            }
            Update::Finished(stage) => {
                self.clear_qr();
                self.worker = None;
                self.window.set_stage(stage);
            }
        }
    }

    fn clear_qr(&mut self) {
        self.ticker.stop();
        self.deadline = None;
        self.window.set_qr(Image::default());
    }
}

fn publish(update: Update) {
    let _ = slint::invoke_from_event_loop(move || {
        PAIRING.with_borrow_mut(|pairing| {
            if let Some(view) = pairing.as_mut() {
                view.update(update);
            }
        });
    });
}

async fn run(mut exit_requested: oneshot::Receiver<()>) -> PairingStage {
    let Some(origin) = option_env!("PINGLIDE_SERVICE_URL") else {
        return PairingStage::CreateFailed;
    };
    let (Ok(service), Ok(material)) = (Service::new(origin), PairingMaterial::generate()) else {
        return PairingStage::CreateFailed;
    };
    let session = tokio::select! {
        result = service.new_receiver() => match result {
            Ok(session) => session,
            Err(_) => return PairingStage::CreateFailed,
        },
        _ = &mut exit_requested => return PairingStage::Canceled,
    };
    let (stage, exiting) = tokio::select! {
        stage = observe(&service, &session, &material) => (stage, false),
        _ = &mut exit_requested => (PairingStage::Canceled, true),
    };
    if exiting || matches!(stage, PairingStage::CreateFailed | PairingStage::Failed) {
        let _ = timeout(Duration::from_secs(2), service.cancel_receiver(&session)).await;
    }
    stage
}

async fn observe(
    service: &Service,
    session: &NewReceiverSession,
    material: &PairingMaterial,
) -> PairingStage {
    let created_deadline = session.expires_at.min(session.pairing_token_expires_at);
    let now = OffsetDateTime::now_utc();
    let deadline = created_deadline.min(now + Duration::from_secs(300));
    let Ok(lifetime) = Duration::try_from(deadline - now) else {
        return PairingStage::Expired;
    };
    timeout(lifetime, watch(service, session, material, deadline))
        .await
        .unwrap_or(PairingStage::Expired)
}

async fn watch(
    service: &Service,
    session: &NewReceiverSession,
    material: &PairingMaterial,
    deadline: OffsetDateTime,
) -> PairingStage {
    let Ok(Ok((mut events, mut state))) = timeout(Duration::from_secs(15), async {
        let mut events = service.pairing_events(session).await?;
        let state = events.next_state().await?;
        Ok::<_, windows_app::service::PairingEventError>((events, state))
    })
    .await
    else {
        return PairingStage::CreateFailed;
    };
    let mut waiting = false;
    loop {
        match state {
            SessionState::AwaitingAuthorization if !waiting => {
                if let Some(pixels) = qr_image(material, session) {
                    publish(Update::Waiting(pixels, deadline));
                    waiting = true;
                } else {
                    return PairingStage::CreateFailed;
                }
            }
            SessionState::AwaitingAuthorization => {}
            SessionState::AwaitingConfirmation => publish(Update::Confirming),
            SessionState::Canceled => return PairingStage::Canceled,
            SessionState::Expired => return PairingStage::Expired,
            _ => return PairingStage::Failed,
        }
        match events.next_state().await {
            Ok(next) => state = next,
            Err(_) => return PairingStage::Failed,
        }
    }
}

fn qr_image(
    material: &PairingMaterial,
    session: &NewReceiverSession,
) -> Option<SharedPixelBuffer<Rgb8Pixel>> {
    let payload = Zeroizing::new(material.qr_payload(&session.session_id, &session.receiver_id)?);
    let code = QrCode::new(payload.as_bytes()).ok()?;
    let side = 160;
    let scale = side / (code.width() + 8);
    if scale == 0 {
        return None;
    }
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
    Some(pixels)
}
