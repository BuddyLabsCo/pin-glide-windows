#![cfg_attr(windows, windows_subsystem = "windows")]

#[cfg(windows)]
slint::include_modules!();

#[cfg(windows)]
mod platform;
#[cfg(windows)]
mod ui;

#[cfg(windows)]
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let explicit = !std::env::args_os().any(|argument| argument == "--startup");
    let Some(instance) = platform::Instance::claim(explicit)? else {
        return Ok(());
    };
    run(instance, explicit, select_backend)
}

#[cfg(windows)]
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

#[cfg(windows)]
fn run(
    instance: platform::Instance,
    mut explicit: bool,
    mut select_backend: impl FnMut() -> Result<(), slint::PlatformError>,
) -> Result<(), Box<dyn std::error::Error>> {
    if let Err(error) = platform::enable_per_monitor_dpi() {
        eprintln!("Could not set per-monitor DPI awareness: {error}");
    }
    let tray = platform::Tray::new(instance.window_name())?;
    let _activation = instance
        .listen(tray.window())
        .map_err(|error| {
            eprintln!("Could not listen for another launch: {error}");
        })
        .ok();
    let mut backend_ready = false;
    loop {
        let ui_result = (|| {
            if !backend_ready {
                select_backend()?;
                backend_ready = true;
            }
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

#[cfg(not(windows))]
fn main() {
    eprintln!("The Pin Glide window runs on Windows.");
}

#[cfg(all(test, windows))]
mod tests {
    use std::sync::mpsc::channel;
    use std::time::Duration;

    use windows_sys::Win32::System::Threading::GetCurrentThreadId;
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        FindWindowW, IsWindowVisible, PostThreadMessageW, WM_QUIT,
    };

    use super::{platform, run};

    #[test]
    #[ignore = "requires an interactive Windows desktop with Pin Glide stopped"]
    fn failed_automatic_startup_keeps_the_tray_running_and_explicit_launch_retries() {
        let instance = platform::Instance::claim(false).unwrap().unwrap();
        let window_name = instance.window_name().to_vec();
        // SAFETY: this query returns the ID of the thread that will pump the message queue.
        let tray_thread = unsafe { GetCurrentThreadId() };
        let (attempted, attempts) = channel();
        let observer = std::thread::spawn(move || {
            let timeout = Duration::from_secs(5);
            let result = (|| {
                attempts.recv_timeout(timeout).ok()?;
                // SAFETY: the name is NUL-terminated and owned by this thread.
                let tray = unsafe { FindWindowW(std::ptr::null(), window_name.as_ptr()) };
                // SAFETY: IsWindowVisible accepts the window handle returned by FindWindowW.
                let visible = unsafe { IsWindowVisible(tray) } != 0;
                if tray.is_null() || visible {
                    return None;
                }
                if platform::Instance::claim(true).ok()?.is_some() {
                    return None;
                }
                attempts.recv_timeout(timeout).ok()
            })();
            // SAFETY: the startup thread created the tray window and owns its queue.
            unsafe { PostThreadMessageW(tray_thread, WM_QUIT, 0, 0) };
            result
        });
        run(instance, false, || {
            attempted.send(()).unwrap();
            Err(slint::PlatformError::Other(
                "simulated startup failure".into(),
            ))
        })
        .unwrap();
        assert_eq!(observer.join().unwrap(), Some(()));
    }
}
