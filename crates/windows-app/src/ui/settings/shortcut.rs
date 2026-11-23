use std::{cell::Cell, io};

use slint::{
    ComponentHandle,
    winit_030::{
        EventResult, WinitWindowAccessor,
        winit::{
            event::WindowEvent,
            raw_window_handle::{HasWindowHandle, RawWindowHandle},
        },
    },
};

use crate::{
    SettingsWindow,
    platform::{
        self,
        shortcut::{self, ShortcutError},
    },
    storage::settings::Shortcut,
};

use super::{PREFERENCES, WINDOW};

thread_local! {
    static ERROR: Cell<Option<ShortcutError>> = const { Cell::new(None) };
    static RESTORE_CONFLICT: Cell<bool> = const { Cell::new(false) };
}

pub(crate) fn restore_shortcut() {
    let saved = PREFERENCES.with_borrow(|preferences| preferences.as_ref()?.settings.shortcut);
    let error = platform::replace_shortcut(saved, || Ok(())).err();
    let conflict = matches!(error, Some(ShortcutError::Conflict));
    RESTORE_CONFLICT.set(conflict);
    ERROR.set(error.filter(|_| !conflict));
}

pub(super) fn connect(window: &SettingsWindow) {
    let weak = window.as_weak();
    window.on_change_shortcut(move || {
        if let Some(window) = weak.upgrade() {
            let result = start(&window);
            window.set_capturing_shortcut(result.is_ok());
            ERROR.set(result.err());
            render(&window);
        }
    });
    let weak = window.as_weak();
    window.on_clear_shortcut(move || {
        if let Some(window) = weak.upgrade() {
            cancel(&window);
            save(&window, None);
        }
    });
    let weak = window.as_weak();
    window.window().on_winit_window_event(move |_, event| {
        if matches!(event, WindowEvent::Focused(false))
            && let Some(window) = weak.upgrade()
        {
            cancel(&window);
        }
        EventResult::Propagate
    });
    render(window);
}

fn start(window: &SettingsWindow) -> Result<(), ShortcutError> {
    let result = window.window().with_winit_window(|native| {
        let handle = native
            .window_handle()
            .map_err(|_| ShortcutError::Register)?;
        let RawWindowHandle::Win32(handle) = handle.as_raw() else {
            return Err(ShortcutError::Register);
        };
        platform::capture_shortcut(handle.hwnd.get() as _)
    });
    result.unwrap_or(Err(ShortcutError::Register))
}

pub(super) fn cancel(window: &SettingsWindow) {
    if window.get_capturing_shortcut() {
        shortcut::finish_capture();
        window.set_capturing_shortcut(false);
        render(window);
    }
}

pub(crate) fn finish_shortcut_capture(apply: bool) {
    WINDOW.with_borrow(|window| {
        let Some(window) = window
            .as_ref()
            .filter(|window| window.get_capturing_shortcut())
        else {
            return;
        };
        let draft = shortcut::finish_capture();
        window.set_capturing_shortcut(false);
        if apply {
            if let Some(draft) = draft {
                save(window, Some(draft));
            } else {
                ERROR.set(Some(ShortcutError::Invalid));
                render(window);
            }
        } else {
            render(window);
        }
    });
}

fn save(window: &SettingsWindow, chord: Option<Shortcut>) {
    let result = platform::replace_shortcut(chord, || {
        PREFERENCES.with_borrow_mut(|preferences| {
            let state = preferences.as_mut().ok_or(io::ErrorKind::NotFound)?;
            if state.settings.shortcut != chord {
                let mut next = state.settings.clone();
                next.shortcut = chord;
                state.store.save(&next)?;
                state.settings = next;
            }
            Ok(())
        })
    });
    if result.is_ok() {
        RESTORE_CONFLICT.set(false);
    }
    ERROR.set(result.err());
    render(window);
}

pub(super) fn render(window: &SettingsWindow) {
    let effective = shortcut::effective();
    window.set_shortcut(effective.map_or_else(String::new, Shortcut::label).into());
    window.set_can_clear_shortcut(
        effective.is_some()
            || PREFERENCES.with_borrow(|preferences| {
                preferences
                    .as_ref()
                    .is_some_and(|state| state.settings.shortcut.is_some())
            }),
    );
    window.set_shortcut_error(
        match (ERROR.get(), window.get_chinese()) {
            (Some(ShortcutError::Invalid), true) => "请输入完整且受支持的快捷键组合。",
            (Some(ShortcutError::Invalid), false) => "Enter a complete, supported shortcut.",
            (Some(ShortcutError::Conflict), true) if effective.is_some() => {
                "此快捷键已被占用，原有快捷键保持不变。"
            }
            (Some(ShortcutError::Conflict), false) if effective.is_some() => {
                "This shortcut is already in use. The previous shortcut is unchanged."
            }
            (Some(ShortcutError::Conflict), true) => "此快捷键已被占用。",
            (Some(ShortcutError::Conflict), false) => "This shortcut is already in use.",
            (Some(ShortcutError::Register), true) => "无法在 Windows 中注册快捷键，请重试。",
            (Some(ShortcutError::Register), false) => {
                "Could not register the shortcut with Windows. Try again."
            }
            (Some(ShortcutError::Save), true) => "无法保存快捷键，请重试。",
            (Some(ShortcutError::Save), false) => "Could not save the shortcut. Try again.",
            (None, chinese) if RESTORE_CONFLICT.get() && !window.get_capturing_shortcut() => {
                if chinese {
                    "已保存的快捷键被占用，当前没有生效的快捷键。"
                } else {
                    "The saved shortcut is already in use. No shortcut is active."
                }
            }
            (None, _) => "",
        }
        .into(),
    );
}
