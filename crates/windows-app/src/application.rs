use crate::{platform, ui};

pub fn launch(explicit: bool) -> Result<(), Box<dyn std::error::Error>> {
    let Some(instance) = platform::Instance::claim(explicit)? else {
        return Ok(());
    };
    run(
        instance,
        explicit,
        select_backend,
        platform::Instance::listen,
    )
}

fn select_backend() -> Result<(), slint::PlatformError> {
    use slint::winit_030::winit::window::WindowButtons;
    slint::BackendSelector::new()
        .backend_name("winit".into())
        .renderer_name("software".into())
        .with_winit_window_attributes_hook(|attributes| {
            attributes
                .with_resizable(false)
                .with_enabled_buttons(WindowButtons::CLOSE)
        })
        .select()
}

pub(crate) fn run(
    instance: platform::Instance,
    mut explicit: bool,
    mut select_backend: impl FnMut() -> Result<(), slint::PlatformError>,
    start_listener: impl FnOnce(
        &platform::Instance,
        windows_sys::Win32::Foundation::HWND,
    ) -> std::io::Result<platform::ActivationListener>,
) -> Result<(), Box<dyn std::error::Error>> {
    if let Err(error) = platform::enable_per_monitor_dpi() {
        eprintln!("Could not set per-monitor DPI awareness: {error}");
    }
    let tray = platform::Tray::new(instance.window_name())?;
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
            ui::resume_pairing()?;
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
