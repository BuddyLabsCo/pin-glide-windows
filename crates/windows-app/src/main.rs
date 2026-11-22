#![cfg_attr(windows, windows_subsystem = "windows")]

#[cfg(windows)]
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let explicit = !std::env::args_os().any(|argument| argument == "--startup");
    windows_app::launch(explicit)
}

#[cfg(not(windows))]
fn main() {
    eprintln!("The Pin Glide window runs on Windows.");
}
