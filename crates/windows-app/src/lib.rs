pub mod service;
mod storage;

#[cfg(windows)]
slint::include_modules!();

#[cfg(windows)]
mod application;
#[cfg(windows)]
mod platform;
#[cfg(windows)]
mod test_environment;
#[cfg(windows)]
pub mod ui;
#[cfg(windows)]
pub use application::{launch, select_backend};
#[cfg(windows)]
#[doc(hidden)]
pub use platform::set_animation_preference_override;
#[cfg(all(test, windows))]
mod startup_tests;
