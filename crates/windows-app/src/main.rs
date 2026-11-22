#![cfg_attr(windows, windows_subsystem = "windows")]

#[cfg(windows)]
slint::include_modules!();

#[cfg(windows)]
mod platform;
#[cfg(windows)]
mod ui;

#[cfg(windows)]
fn main() -> Result<(), Box<dyn std::error::Error>> {
    use platform::{Instance, Tray};
    use slint::winit_030::winit::window::WindowButtons;

    let explicit = !std::env::args_os().any(|argument| argument == "--startup");
    let Some(instance) = Instance::claim(explicit)? else {
        return Ok(());
    };

    platform::enable_per_monitor_dpi()?;
    slint::BackendSelector::new()
        .backend_name("winit".into())
        .renderer_name("software".into())
        .with_winit_window_attributes_hook(|attributes| {
            attributes
                .with_resizable(false)
                .with_enabled_buttons(WindowButtons::CLOSE)
        })
        .select()?;
    let _tray = Tray::new(instance.window_name())?;
    let _activation = instance.listen()?;
    if explicit {
        ui::open();
    }
    slint::run_event_loop_until_quit()?;
    ui::close();
    Ok(())
}

#[cfg(not(windows))]
fn main() {
    eprintln!("The Pin Glide window runs on Windows.");
}
