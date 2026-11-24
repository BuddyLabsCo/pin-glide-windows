use crate::{platform, ui};

pub fn launch(explicit: bool) -> Result<(), Box<dyn std::error::Error>> {
    launch_app(explicit, ui::connect_service)
}

#[cfg(debug_assertions)]
#[doc(hidden)]
pub fn launch_with_connection(
    explicit: bool,
    connect: fn() -> Option<crate::service::Service>,
) -> Result<(), Box<dyn std::error::Error>> {
    if crate::test_environment::directory().is_none() {
        return Err(std::io::Error::other("a Pin Glide test directory is required").into());
    }
    launch_app(explicit, connect)
}

fn launch_app(
    explicit: bool,
    connect: fn() -> Option<crate::service::Service>,
) -> Result<(), Box<dyn std::error::Error>> {
    crate::test_environment::validate()?;
    let Some(instance) = platform::Instance::claim(explicit)? else {
        return Ok(());
    };
    if crate::test_environment::directory().is_some() {
        let animation_override = match std::env::var("PINGLIDE_TEST_REDUCED_MOTION").as_deref() {
            Ok("0") => Some(true),
            Ok("1") => Some(false),
            _ => None,
        };
        platform::set_animation_preference_override(animation_override);
    }
    ui::initialize_preferences();
    run_with_connection(
        instance,
        explicit,
        select_backend,
        platform::Instance::listen,
        connect,
    )
}

pub fn select_backend() -> Result<(), slint::PlatformError> {
    use slint::winit_030::winit::{
        event_loop::EventLoop, platform::windows::EventLoopBuilderExtWindows, window::WindowButtons,
    };
    let mut backend = slint::BackendSelector::new()
        .backend_name("winit".into())
        .renderer_name("software".into())
        .with_winit_window_attributes_hook(|attributes| {
            platform::popup_attributes(attributes)
                .with_resizable(false)
                .with_enabled_buttons(WindowButtons::CLOSE)
        });
    if crate::test_environment::directory().is_some() {
        let mut builder = EventLoop::with_user_event();
        builder.with_any_thread(true);
        backend = backend.with_winit_event_loop_builder(builder);
    }
    backend.select()
}

pub(crate) fn run_with_connection(
    instance: platform::Instance,
    mut explicit: bool,
    mut select_backend: impl FnMut() -> Result<(), slint::PlatformError>,
    start_listener: impl FnOnce(
        &platform::Instance,
        windows_sys::Win32::Foundation::HWND,
    ) -> std::io::Result<platform::ActivationListener>,
    connect: fn() -> Option<crate::service::Service>,
) -> Result<(), Box<dyn std::error::Error>> {
    if let Err(error) = platform::enable_per_monitor_dpi() {
        eprintln!("Could not set per-monitor DPI awareness: {error}");
    }
    let tray = platform::Tray::new(instance.window_name())?;
    ui::restore_shortcut();
    let _activation = match start_listener(&instance, tray.window()) {
        Ok(listener) => Some(listener),
        Err(error) => {
            eprintln!("Could not listen for another launch: {error}");
            tray.poll_activation(instance.activation_event())?;
            None
        }
    };
    let mut backend_ready = false;
    loop {
        let ui_result = (|| {
            if !backend_ready {
                select_backend()?;
                backend_ready = true;
            }
            ui::resume_pairing(connect)?;
            if explicit {
                ui::open();
            }
            slint::run_event_loop_until_quit()
        })();
        ui::close();
        match ui_result {
            Ok(()) => return Ok(()),
            Err(error) => eprintln!("Could not run the Welcome interface: {error}"),
        }
        if !tray.run_without_ui()? {
            return Ok(());
        }
        explicit = true;
    }
}
