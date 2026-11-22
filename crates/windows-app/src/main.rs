#![cfg_attr(windows, windows_subsystem = "windows")]

#[cfg(windows)]
slint::include_modules!();

#[cfg(windows)]
fn main() -> Result<(), slint::PlatformError> {
    let window = AppWindow::new()?;
    window.set_greeting(receiver_core::GREETING.into());
    window.run()
}

#[cfg(not(windows))]
fn main() {
    eprintln!("The Pin Glide window runs on Windows.");
}
