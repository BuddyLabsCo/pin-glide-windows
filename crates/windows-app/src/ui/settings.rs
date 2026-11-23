use std::{
    cell::{Cell, RefCell},
    io,
    num::NonZeroU32,
};

use i_slint_core::{MotionPreference, window::WindowInner};
use slint::{CloseRequestResponse, ComponentHandle, PlatformError, SharedString};

use crate::{
    MainWindow, PopupDurationError, SettingsWindow,
    platform::{self, launch_at_login::LaunchAtLogin},
    storage::settings::{Language, Settings, SettingsStore, default_popup_duration_seconds},
};

#[cfg(test)]
mod tests;

thread_local! {
    static PREFERENCES: RefCell<Option<Preferences>> = const { RefCell::new(None) };
    static WINDOW: RefCell<Option<SettingsWindow>> = const { RefCell::new(None) };
    static LANGUAGE_FAILED: Cell<bool> = const { Cell::new(false) };
    static STARTUP_FAILED: RefCell<Option<StartupError>> = const { RefCell::new(None) };
    static STARTUP_ENABLED: Cell<bool> = const { Cell::new(false) };
}

struct Preferences {
    store: SettingsStore,
    settings: Settings,
    startup: io::Result<LaunchAtLogin>,
}

#[derive(Debug, thiserror::Error)]
enum StartupError {
    #[error("Windows Startup Apps has disabled launch at login")]
    WindowsOverride,
    #[error("{change}. Could not restore the prior Windows startup entry: {restore}")]
    Rollback {
        change: Box<StartupError>,
        restore: io::Error,
    },
    #[error("could not set launch at login to {0}: {1}")]
    Change(bool, io::Error),
}

impl Preferences {
    fn load(store: SettingsStore, startup: io::Result<LaunchAtLogin>) -> Self {
        let saved = store.read();
        let first_launch = matches!(saved, Ok(None));
        let mut preferences = Self {
            store,
            settings: saved.ok().flatten().unwrap_or_default(),
            startup,
        };
        if first_launch && let Err(error) = preferences.save_startup(true) {
            eprintln!("Could not initialize launch at login: {error}");
            STARTUP_FAILED.replace(Some(error));
        }
        preferences
    }

    fn save_language(&mut self, language: Language) -> io::Result<()> {
        let mut next = self.settings;
        next.language = language;
        self.store.save(next)?;
        self.settings = next;
        Ok(())
    }

    fn save_popup_duration(&mut self, seconds: NonZeroU32) -> io::Result<()> {
        if self.settings.popup_duration_seconds != seconds {
            let mut next = self.settings;
            next.popup_duration_seconds = seconds;
            self.store.save(next)?;
            self.settings = next;
        }
        Ok(())
    }

    fn save_startup(&mut self, enabled: bool) -> Result<(), StartupError> {
        let change_error = |error| StartupError::Change(enabled, error);
        let mut next = self.settings;
        next.launch_at_login = enabled;
        let staged = self.store.stage(next).map_err(change_error)?;
        let startup = self.startup().map_err(change_error)?;
        let entry = startup.read().map_err(change_error)?;
        let desired = startup.command_for(enabled).map_err(change_error)?;
        if enabled && !startup.approved().map_err(change_error)? {
            return Err(StartupError::WindowsOverride);
        }
        let changed = entry.as_deref() != desired;
        if changed {
            startup.replace(desired).map_err(change_error)?;
        }
        let commit = (|| {
            if enabled && !startup.approved().map_err(change_error)? {
                return Err(StartupError::WindowsOverride);
            }
            staged.commit().map_err(change_error)
        })();
        if let Err(error) = commit {
            if changed && let Err(restore) = startup.replace(entry.as_deref()) {
                return Err(StartupError::Rollback {
                    change: Box::new(error),
                    restore,
                });
            }
            return Err(error);
        }
        self.settings = next;
        Ok(())
    }

    fn startup(&self) -> io::Result<&LaunchAtLogin> {
        self.startup
            .as_ref()
            .map_err(|error| io::Error::new(error.kind(), error.to_string()))
    }

    fn is_startup_enabled(&self) -> io::Result<bool> {
        self.startup()?.enabled()
    }
}

pub(crate) fn initialize() {
    match SettingsStore::for_current_user() {
        Ok(store) => install(Preferences::load(
            store,
            LaunchAtLogin::for_current_executable(),
        )),
        Err(error) => eprintln!("Could not locate Settings: {error}"),
    }
}

fn install(preferences: Preferences) {
    platform::set_language(preferences.settings.language);
    PREFERENCES.replace(Some(preferences));
}

pub(super) fn popup_duration_seconds() -> NonZeroU32 {
    PREFERENCES.with_borrow(|preferences| {
        preferences
            .as_ref()
            .map_or_else(default_popup_duration_seconds, |state| {
                state.settings.popup_duration_seconds
            })
    })
}

#[cfg(test)]
pub(super) fn save_test_popup_duration(path: &std::path::Path, seconds: NonZeroU32) {
    let store = SettingsStore::new(path.to_owned());
    let settings = store.read().unwrap().unwrap_or_default();
    let mut preferences = Preferences {
        store,
        settings,
        startup: Err(io::ErrorKind::NotFound.into()),
    };
    preferences.save_popup_duration(seconds).unwrap();
    PREFERENCES.replace(Some(preferences));
}

pub(super) fn show(
    main: &MainWindow,
    store: &crate::storage::RegistrationStore,
) -> Result<(), PlatformError> {
    let window = WINDOW.with_borrow_mut(|current| {
        if let Some(window) = current {
            return Ok(window.clone_strong());
        }
        let window = SettingsWindow::new()?;
        window.set_popup_duration(popup_duration_seconds().to_string().into());
        let weak = window.as_weak();
        window.window().on_close_requested(move || {
            if let Some(window) = weak.upgrade() {
                window.invoke_focus_page();
            }
            CloseRequestResponse::HideWindow
        });
        window.on_choose_language(choose_language);
        window.on_toggle_startup(toggle_startup);
        let weak = window.as_weak();
        window.on_apply_popup_duration(move |draft| {
            if let Some(window) = weak.upgrade() {
                apply_popup_duration(&window, draft);
            }
        });
        *current = Some(window.clone_strong());
        Ok::<_, PlatformError>(window)
    })?;
    let weak = main.as_weak();
    let store = store.clone();
    window.on_back(move || {
        let Some(main) = weak.upgrade() else { return };
        if let Ok(devices) = store.sender_devices() {
            super::set_sender_devices(&main, devices);
        }
        if let Err(error) = super::show_main(&main).and_then(|()| hide()) {
            eprintln!("Could not return to Main: {error}");
        }
    });
    refresh(platform::chinese());
    window.invoke_focus_page();
    window.show()?;
    main.hide()?;
    super::focus(window.window());
    Ok(())
}

pub(super) fn hide() -> Result<(), PlatformError> {
    WINDOW.with_borrow(|window| {
        if let Some(window) = window {
            window.invoke_focus_page();
            window.hide()?;
        }
        Ok(())
    })
}

#[doc(hidden)]
pub fn settings_visible() -> bool {
    WINDOW.with_borrow(|window| {
        window
            .as_ref()
            .is_some_and(|window| window.window().is_visible())
    })
}

pub(super) fn close() {
    WINDOW.take();
}

pub(super) fn update_motion() {
    WINDOW.with_borrow(|window| {
        if let Some(window) = window {
            apply_motion(window);
        }
    });
}

fn apply_motion(window: &SettingsWindow) {
    let preference = if platform::animations_enabled() {
        MotionPreference::NoPreference
    } else {
        MotionPreference::Reduced
    };
    WindowInner::from_pub(window.window())
        .context()
        .set_motion_preference(preference);
}

fn apply_popup_duration(window: &SettingsWindow, draft: SharedString) {
    let text = draft.trim();
    let Some(seconds) = text
        .parse::<NonZeroU32>()
        .ok()
        .filter(|_| !text.starts_with('+'))
    else {
        window.set_popup_duration_error(PopupDurationError::Invalid);
        return;
    };
    let saved = PREFERENCES.with_borrow_mut(|preferences| {
        preferences
            .as_mut()
            .is_some_and(|state| state.save_popup_duration(seconds).is_ok())
    });
    if saved {
        window.set_popup_duration(text.into());
        window.set_popup_duration_error(PopupDurationError::None);
    } else {
        window.set_popup_duration(popup_duration_seconds().to_string().into());
        window.set_popup_duration_error(PopupDurationError::Save);
    }
}

fn choose_language(index: i32) {
    let language = match index {
        0 => Language::System,
        1 => Language::Chinese,
        2 => Language::English,
        _ => return,
    };
    let saved = PREFERENCES.with_borrow_mut(|preferences| {
        preferences
            .as_mut()
            .is_some_and(|state| state.save_language(language).is_ok())
    });
    LANGUAGE_FAILED.set(!saved);
    if saved {
        platform::set_language(language);
    }
    super::refresh_preferences();
}

fn toggle_startup(enabled: bool) {
    let result = PREFERENCES.with_borrow_mut(|preferences| {
        preferences
            .as_mut()
            .ok_or_else(|| StartupError::Change(enabled, io::ErrorKind::NotFound.into()))?
            .save_startup(enabled)
    });
    if let Err(error) = &result {
        eprintln!("{error}");
    }
    STARTUP_FAILED.replace(result.err());
    refresh(platform::chinese());
}

pub(super) fn refresh(chinese: bool) {
    WINDOW.with_borrow(|window| {
        let Some(window) = window else { return };
        let language = PREFERENCES.with_borrow(|preferences| {
            preferences
                .as_ref()
                .map_or(Language::System, |state| state.settings.language)
        });
        let startup = PREFERENCES.with_borrow(|preferences| {
            preferences
                .as_ref()
                .and_then(|state| state.is_startup_enabled().ok())
        });
        if let Some(enabled) = startup {
            STARTUP_ENABLED.set(enabled);
        }
        window.set_chinese(chinese);
        apply_motion(window);
        window.set_language_index(match language {
            Language::System => 0,
            Language::Chinese => 1,
            Language::English => 2,
        });
        window.set_startup(STARTUP_ENABLED.get());
        window.set_language_error(
            match (LANGUAGE_FAILED.get(), chinese) {
                (true, true) => "无法保存界面语言，请重试。",
                (true, false) => "Could not save the interface language. Try again.",
                _ => "",
            }
            .into(),
        );
        STARTUP_FAILED.with_borrow(|error| window.set_startup_error(
            match (error, startup, chinese) {
                (Some(StartupError::WindowsOverride), Some(true), _) => "",
                (Some(StartupError::WindowsOverride), _, true) => "请在 Windows 设置 → 应用 → 启动中启用鹊信。",
                (Some(StartupError::WindowsOverride), _, false) => "Enable Pin Glide in Windows Settings → Apps → Startup.",
                (Some(StartupError::Rollback { .. }), _, true) => "无法恢复原开机启动设置，请在 Windows 设置 → 应用 → 启动中检查。",
                (Some(StartupError::Rollback { .. }), _, false) => "Could not restore launch at login. Check Windows Settings → Apps → Startup.",
                (Some(StartupError::Change(true, _)), _, true) => "无法启用开机启动，可再次打开开关重试。",
                (Some(StartupError::Change(true, _)), _, false) => {
                    "Could not enable launch at login. Turn the switch on again to retry."
                }
                (Some(StartupError::Change(false, _)), _, true) => "无法关闭开机启动，可再次关闭开关重试。",
                (Some(StartupError::Change(false, _)), _, false) => {
                    "Could not disable launch at login. Turn the switch off again to retry."
                }
                (None, None, true) => "无法读取开机启动状态，请重试。",
                (None, None, false) => "Could not read the launch-at-login status. Try again.",
                _ => "",
            }
            .into(),
        ));
    });
}
