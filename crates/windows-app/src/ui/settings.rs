use std::{
    cell::{Cell, RefCell},
    io,
};

use slint::{CloseRequestResponse, ComponentHandle, PlatformError};

use crate::{
    MainWindow, SettingsWindow,
    platform::{self, launch_at_login::LaunchAtLogin},
    storage::settings::{Language, Settings, SettingsStore},
};

#[cfg(test)]
mod tests;
#[cfg(test)]
pub(super) use tests::test_registration;

thread_local! {
    static PREFERENCES: RefCell<Option<Preferences>> = const { RefCell::new(None) };
    static WINDOW: RefCell<Option<SettingsWindow>> = const { RefCell::new(None) };
    static LANGUAGE_FAILED: Cell<bool> = const { Cell::new(false) };
    static STARTUP_FAILED: Cell<Option<bool>> = const { Cell::new(None) };
    static STARTUP_ENABLED: Cell<bool> = const { Cell::new(false) };
}

struct Preferences {
    store: SettingsStore,
    settings: Settings,
    startup: io::Result<LaunchAtLogin>,
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
            STARTUP_FAILED.set(Some(true));
            eprintln!("Could not initialize launch at login: {error}");
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

    fn save_startup(&mut self, enabled: bool) -> io::Result<()> {
        let startup = self.startup()?;
        let entry = startup.read()?;
        let changed = if enabled {
            !startup.is_enabled(entry.as_deref())?
        } else {
            entry.is_some()
        };
        if changed {
            startup.set(enabled)?;
            if enabled && !startup.enabled().unwrap_or(false) {
                startup.replace(entry.as_deref())?;
                return Err(io::ErrorKind::PermissionDenied.into());
            }
        }
        let mut next = self.settings;
        next.launch_at_login = enabled;
        if let Err(error) = self.store.save(next) {
            if changed && let Err(rollback) = startup.replace(entry.as_deref()) {
                eprintln!("Could not restore launch at login: {rollback}");
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

pub(super) fn show(main: &MainWindow) -> Result<(), PlatformError> {
    let window = WINDOW.with_borrow_mut(|current| {
        if let Some(window) = current {
            return Ok(window.clone_strong());
        }
        let window = SettingsWindow::new()?;
        window
            .window()
            .on_close_requested(|| CloseRequestResponse::HideWindow);
        window.on_choose_language(choose_language);
        window.on_toggle_startup(toggle_startup);
        *current = Some(window.clone_strong());
        Ok::<_, PlatformError>(window)
    })?;
    let weak = main.as_weak();
    window.on_back(move || {
        if let Some(main) = weak.upgrade()
            && let Err(error) = super::show_main(&main).and_then(|()| hide())
        {
            eprintln!("Could not return to Main: {error}");
        }
    });
    refresh(platform::chinese());
    window.show()?;
    main.hide()?;
    super::focus(window.window());
    Ok(())
}

pub(super) fn hide() -> Result<(), PlatformError> {
    WINDOW.with_borrow(|window| window.as_ref().map_or(Ok(()), ComponentHandle::hide))
}

pub(super) fn close() {
    WINDOW.take();
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
    let saved = PREFERENCES.with_borrow_mut(|preferences| {
        preferences
            .as_mut()
            .is_some_and(|state| state.save_startup(enabled).is_ok())
    });
    STARTUP_FAILED.set((!saved).then_some(enabled));
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
        window.set_startup_error(
            match (STARTUP_FAILED.get(), startup, chinese) {
                (Some(true), _, true) => "无法启用开机启动，可再次打开开关重试。",
                (Some(true), _, false) => {
                    "Could not enable launch at login. Turn the switch on again to retry."
                }
                (Some(false), _, true) => "无法关闭开机启动，可再次关闭开关重试。",
                (Some(false), _, false) => {
                    "Could not disable launch at login. Turn the switch off again to retry."
                }
                (None, None, true) => "无法读取开机启动状态，请重试。",
                (None, None, false) => "Could not read the launch-at-login status. Try again.",
                _ => "",
            }
            .into(),
        );
    });
}
