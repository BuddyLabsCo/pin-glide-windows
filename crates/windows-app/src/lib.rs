pub mod service;
mod storage;

#[cfg(windows)]
slint::include_modules!();

#[cfg(windows)]
mod application;
#[cfg(windows)]
mod platform;
#[cfg(windows)]
pub mod ui;
#[cfg(windows)]
pub use application::{launch, select_backend};
#[cfg(all(test, windows))]
mod startup_tests;
#[cfg(test)]
extern crate self as windows_app;
