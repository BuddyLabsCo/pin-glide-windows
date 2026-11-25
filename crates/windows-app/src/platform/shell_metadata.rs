use std::{ffi::OsStr, os::windows::ffi::OsStrExt};

use slint::{
    Window,
    winit_030::{
        WinitWindowAccessor,
        winit::raw_window_handle::{HasWindowHandle, RawWindowHandle},
    },
};
use windows::{
    Win32::{
        Foundation::{HWND, PROPERTYKEY},
        Storage::EnhancedStorage::{
            PKEY_AppUserModel_ID, PKEY_AppUserModel_RelaunchCommand,
            PKEY_AppUserModel_RelaunchDisplayNameResource, PKEY_AppUserModel_RelaunchIconResource,
        },
        System::{
            Com::{CoTaskMemAlloc, StructuredStorage::PROPVARIANT},
            Variant::VT_LPWSTR,
        },
        UI::Shell::PropertiesSystem::{IPropertyStore, SHGetPropertyStoreForWindow},
    },
    core::PWSTR,
};
use windows_sys::Win32::{
    Foundation::{LPARAM, LRESULT, WPARAM},
    UI::{
        Shell::{DefSubclassProc, RemoveWindowSubclass, SetWindowSubclass},
        WindowsAndMessaging::WM_NCDESTROY,
    },
};

const PROPERTIES: [(PROPERTYKEY, &str, &str); 4] = [
    (PKEY_AppUserModel_RelaunchCommand, "\"", "\""),
    (PKEY_AppUserModel_RelaunchDisplayNameResource, "@", ",-101"),
    (PKEY_AppUserModel_RelaunchIconResource, "", ",-1"),
    (PKEY_AppUserModel_ID, "BuddyLabsCo.PinGlide", ""),
];

pub(crate) fn configure_taskbar(window: &Window) {
    window.with_winit_window(|native| {
        let Ok(handle) = native.window_handle() else {
            return;
        };
        let RawWindowHandle::Win32(handle) = handle.as_raw() else {
            return;
        };
        let Ok(executable) = std::env::current_exe() else {
            return;
        };
        if let Err(error) = set_properties(HWND(handle.hwnd.get() as _), executable.as_os_str()) {
            eprintln!("Could not configure taskbar display metadata: {error}");
        }
    });
}

fn set_properties(hwnd: HWND, executable: &OsStr) -> windows::core::Result<()> {
    // SAFETY: callers keep the HWND alive; the window property store owns
    // the COM-allocated string after a successful SetValue.
    unsafe {
        if SetWindowSubclass(hwnd.0, Some(cleanup), 0, 0) == 0 {
            return Err(windows::core::Error::from_thread());
        }
        let store: IPropertyStore = SHGetPropertyStoreForWindow(hwnd)?;
        for (key, prefix, suffix) in PROPERTIES {
            let mut value: Vec<u16> = prefix.encode_utf16().collect();
            if key != PKEY_AppUserModel_ID {
                value.extend(executable.encode_wide());
            }
            value.extend(suffix.encode_utf16().chain([0]));
            let text = CoTaskMemAlloc(value.len() * size_of::<u16>()).cast::<u16>();
            if text.is_null() {
                return Err(windows::core::Error::from_hresult(
                    windows::Win32::Foundation::E_OUTOFMEMORY,
                ));
            }
            text.copy_from_nonoverlapping(value.as_ptr(), value.len());
            let mut variant = PROPVARIANT::default();
            let fields = &mut *variant.Anonymous.Anonymous;
            fields.vt = VT_LPWSTR;
            fields.Anonymous.pwszVal = PWSTR(text);
            if let Err(error) = store.SetValue(&key, &variant) {
                let _ =
                    windows::Win32::System::Com::StructuredStorage::PropVariantClear(&mut variant);
                return Err(error);
            }
        }
    }
    Ok(())
}

unsafe extern "system" fn cleanup(
    hwnd: windows_sys::Win32::Foundation::HWND,
    message: u32,
    wparam: WPARAM,
    lparam: LPARAM,
    id: usize,
    _: usize,
) -> LRESULT {
    // SAFETY: the subclass runs on the owning window thread before destruction.
    unsafe {
        if message == WM_NCDESTROY {
            if let Ok(store) = SHGetPropertyStoreForWindow::<IPropertyStore>(HWND(hwnd)) {
                for (key, _, _) in PROPERTIES {
                    let _ = store.SetValue(&key, &PROPVARIANT::default());
                }
            }
            RemoveWindowSubclass(hwnd, Some(cleanup), id);
        }
        DefSubclassProc(hwnd, message, wparam, lparam)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{cell::Cell, ptr::null_mut};
    use windows::Win32::System::Com::StructuredStorage::{PropVariantClear, PropVariantToString};
    use windows::Win32::System::Variant::VT_EMPTY;
    use windows_sys::Win32::UI::WindowsAndMessaging::{CreateWindowExW, DestroyWindow};

    thread_local! {
        static CLEANUP_OBSERVED: Cell<bool> = const { Cell::new(false) };
    }

    struct TestWindow(HWND);

    impl Drop for TestWindow {
        fn drop(&mut self) {
            // SAFETY: the test owns this window on the current thread.
            unsafe {
                if DestroyWindow(self.0.0) == 0 {
                    RemoveWindowSubclass(self.0.0, Some(observe_cleanup), 0);
                }
            }
        }
    }

    unsafe extern "system" fn observe_cleanup(
        hwnd: windows_sys::Win32::Foundation::HWND,
        message: u32,
        wparam: WPARAM,
        lparam: LPARAM,
        _: usize,
        _: usize,
    ) -> LRESULT {
        // SAFETY: the callback executes on the thread owning the live window.
        unsafe {
            if message == WM_NCDESTROY {
                let cleared =
                    SHGetPropertyStoreForWindow::<IPropertyStore>(HWND(hwnd)).is_ok_and(|store| {
                        [
                            PKEY_AppUserModel_RelaunchCommand,
                            PKEY_AppUserModel_RelaunchDisplayNameResource,
                            PKEY_AppUserModel_RelaunchIconResource,
                            PKEY_AppUserModel_ID,
                        ]
                        .iter()
                        .all(|key| {
                            store.GetValue(key).is_ok_and(|mut value| {
                                let empty = value.Anonymous.Anonymous.vt == VT_EMPTY;
                                let _ = PropVariantClear(&mut value);
                                empty
                            })
                        })
                    });
                CLEANUP_OBSERVED.set(cleared);
            }
            DefSubclassProc(hwnd, message, wparam, lparam)
        }
    }

    #[test]
    fn native_taskbar_properties_reference_receiver_resources() {
        // SAFETY: create and destroy a hidden window on this thread, keeping
        // the property store and returned PROPVARIANTs within its lifetime.
        unsafe {
            let hwnd = CreateWindowExW(
                0,
                crate::platform::wide("STATIC").as_ptr(),
                null_mut(),
                0,
                0,
                0,
                0,
                0,
                null_mut(),
                null_mut(),
                null_mut(),
                null_mut(),
            );
            assert!(!hwnd.is_null());
            let hwnd = HWND(hwnd);
            let window = TestWindow(hwnd);
            CLEANUP_OBSERVED.set(false);
            assert_ne!(SetWindowSubclass(hwnd.0, Some(observe_cleanup), 0, 0), 0);
            let path = r"C:\Program Files\Pin Glide\windows-app.exe";
            set_properties(hwnd, OsStr::new(r"C:\Previous Receiver\windows-app.exe")).unwrap();
            set_properties(hwnd, OsStr::new(path)).unwrap();
            let store: IPropertyStore = SHGetPropertyStoreForWindow(hwnd).unwrap();
            for (key, expected) in [
                (PKEY_AppUserModel_RelaunchCommand, format!("\"{path}\"")),
                (
                    PKEY_AppUserModel_RelaunchDisplayNameResource,
                    format!("@{path},-101"),
                ),
                (PKEY_AppUserModel_RelaunchIconResource, format!("{path},-1")),
                (PKEY_AppUserModel_ID, "BuddyLabsCo.PinGlide".into()),
            ] {
                let mut value = store.GetValue(&key).unwrap();
                let mut text = [0u16; 256];
                PropVariantToString(&value, &mut text).unwrap();
                PropVariantClear(&mut value).unwrap();
                let length = text.iter().position(|&unit| unit == 0).unwrap();
                assert_eq!(String::from_utf16(&text[..length]).unwrap(), expected);
            }
            drop(store);
            drop(window);
            assert!(
                CLEANUP_OBSERVED.get(),
                "taskbar properties remained on window destruction"
            );
        }
    }
}
