use std::{
    fs, io,
    path::{Path, PathBuf},
    process::{Child, Command},
};

use windows_sys::Win32::{
    Foundation::{ERROR_FILE_NOT_FOUND, ERROR_SUCCESS},
    System::Registry::{HKEY_CURRENT_USER, RRF_RT_ANY, RegDeleteTreeW, RegGetValueW},
};

const RUN: &str = r"Software\Microsoft\Windows\CurrentVersion\Run";
const APPROVAL: &str = r"Software\Microsoft\Windows\CurrentVersion\Explorer\StartupApproved\Run";

pub struct Fixture {
    directory: tempfile::TempDir,
    original: SystemSettings,
}

#[derive(Debug, PartialEq)]
struct SystemSettings {
    file: Option<Vec<u8>>,
    run: Option<(u32, Vec<u8>)>,
    approval: Option<(u32, Vec<u8>)>,
}

impl Fixture {
    pub fn new() -> Self {
        if !cfg!(debug_assertions) {
            panic!("fixture requires a debug build");
        }
        let directory = tempfile::Builder::new()
            .prefix("PinGlideTest-")
            .tempdir()
            .unwrap();
        let original = system_settings();
        Self {
            directory,
            original,
        }
    }

    pub fn path(&self) -> &Path {
        self.directory.path()
    }

    fn registry_root(&self) -> String {
        format!(
            r"Software\BuddyLabsCo\{}",
            self.path().file_name().unwrap().to_str().unwrap()
        )
    }

    pub fn startup(&self) -> Option<(u32, Vec<u8>)> {
        registry_value(&format!(r"{}\Run", self.registry_root()))
    }

    pub fn command(&self) -> Command {
        let mut command = Command::new(env!("CARGO_BIN_EXE_windows-app"));
        command.env("PINGLIDE_TEST_DIRECTORY", self.path());
        command
    }

    pub fn verify_system_settings(&self) {
        assert_eq!(system_settings(), self.original);
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let key = wide(&self.registry_root());
        // SAFETY: the key names this fixture's unique non-startup subtree in HKCU.
        unsafe { RegDeleteTreeW(HKEY_CURRENT_USER, key.as_ptr()) };
        if !std::thread::panicking() {
            self.verify_system_settings();
        }
    }
}

pub struct AppProcess(pub Child);

impl Drop for AppProcess {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

fn system_settings() -> SystemSettings {
    use std::{ffi::OsString, os::windows::ffi::OsStringExt, ptr::null_mut};
    use windows_sys::Win32::{
        System::Com::CoTaskMemFree,
        UI::Shell::{FOLDERID_LocalAppData, SHGetKnownFolderPath},
    };

    let mut path = null_mut();
    // SAFETY: the known folder ID and Shell output pointer are valid.
    let result = unsafe { SHGetKnownFolderPath(&FOLDERID_LocalAppData, 0, null_mut(), &mut path) };
    assert!(result >= 0 && !path.is_null());
    // SAFETY: Shell returns a NUL-terminated string owned by this call.
    let settings = unsafe {
        let length = (0..).find(|index| *path.add(*index) == 0).unwrap();
        let folder = OsString::from_wide(std::slice::from_raw_parts(path, length));
        CoTaskMemFree(path.cast());
        PathBuf::from(folder)
            .join("Pin Glide")
            .join("settings.json")
    };
    let file = match fs::read(settings) {
        Ok(bytes) => Some(bytes),
        Err(error) if error.kind() == io::ErrorKind::NotFound => None,
        Err(error) => panic!("real settings are unreadable: {error}"),
    };
    SystemSettings {
        file,
        run: registry_value(RUN),
        approval: registry_value(APPROVAL),
    }
}

fn registry_value(key: &str) -> Option<(u32, Vec<u8>)> {
    use std::ptr::null_mut;
    let key = wide(key);
    let name = wide("Pin Glide");
    let mut size = 0;
    let mut kind = 0;
    // SAFETY: registry names are terminated and outputs are writable.
    let result = unsafe {
        RegGetValueW(
            HKEY_CURRENT_USER,
            key.as_ptr(),
            name.as_ptr(),
            RRF_RT_ANY,
            &mut kind,
            null_mut(),
            &mut size,
        )
    };
    if result == ERROR_FILE_NOT_FOUND {
        return None;
    }
    assert_eq!(result, ERROR_SUCCESS);
    let mut bytes = vec![0; size as usize + 2];
    size = bytes.len() as u32;
    // SAFETY: the output buffer has the byte capacity reported by size.
    let result = unsafe {
        RegGetValueW(
            HKEY_CURRENT_USER,
            key.as_ptr(),
            name.as_ptr(),
            RRF_RT_ANY,
            &mut kind,
            bytes.as_mut_ptr().cast(),
            &mut size,
        )
    };
    assert_eq!(result, ERROR_SUCCESS);
    bytes.truncate(size as usize);
    Some((kind, bytes))
}

fn wide(text: &str) -> Vec<u16> {
    text.encode_utf16().chain([0]).collect()
}
