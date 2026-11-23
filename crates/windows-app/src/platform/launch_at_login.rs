use std::{io, os::windows::ffi::OsStrExt, ptr::null_mut};

use windows_sys::Win32::{
    Foundation::{ERROR_FILE_NOT_FOUND, ERROR_SUCCESS, WIN32_ERROR},
    System::Registry::{
        HKEY_CURRENT_USER, REG_SZ, RRF_NOEXPAND, RRF_RT_REG_BINARY, RRF_RT_REG_SZ,
        RegDeleteKeyValueW, RegGetValueW, RegSetKeyValueW,
    },
};

use super::wide;

const RUN: &str = r"Software\Microsoft\Windows\CurrentVersion\Run";
const STARTUP_APPROVED: &str =
    r"Software\Microsoft\Windows\CurrentVersion\Explorer\StartupApproved\Run";

pub(crate) struct LaunchAtLogin {
    name: Vec<u16>,
    command: Vec<u16>,
}

impl LaunchAtLogin {
    pub(crate) fn for_current_executable() -> io::Result<Self> {
        let executable = std::env::current_exe()?;
        let command = [b'"' as u16]
            .into_iter()
            .chain(executable.as_os_str().encode_wide())
            .chain(wide("\" --startup"))
            .collect();
        Ok(Self {
            name: wide("Pin Glide"),
            command,
        })
    }

    #[cfg(test)]
    pub(crate) fn for_test(name: &str) -> Self {
        Self {
            name: wide(name),
            command: wide("\"C:\\Pin Glide test\\pin-glide.exe\" --startup"),
        }
    }

    pub(crate) fn read(&self) -> io::Result<Option<Vec<u16>>> {
        let key = wide(RUN);
        let mut size = 0;
        // SAFETY: names are terminated and the call writes only the required buffer size.
        let result = unsafe {
            RegGetValueW(
                HKEY_CURRENT_USER,
                key.as_ptr(),
                self.name.as_ptr(),
                RRF_RT_REG_SZ | RRF_NOEXPAND,
                null_mut(),
                null_mut(),
                &mut size,
            )
        };
        if result == ERROR_FILE_NOT_FOUND {
            return Ok(None);
        }
        registry_result(result)?;
        let mut command = vec![0u16; (size as usize).div_ceil(size_of::<u16>()) + 1];
        size = size_of_val(command.as_slice()) as u32;
        // SAFETY: the buffer has the byte capacity passed in size and names are terminated.
        registry_result(unsafe {
            RegGetValueW(
                HKEY_CURRENT_USER,
                key.as_ptr(),
                self.name.as_ptr(),
                RRF_RT_REG_SZ | RRF_NOEXPAND,
                null_mut(),
                command.as_mut_ptr().cast(),
                &mut size,
            )
        })?;
        command.truncate(size as usize / size_of::<u16>());
        Ok(Some(command))
    }

    pub(crate) fn is_enabled(&self, entry: Option<&[u16]>) -> io::Result<bool> {
        if entry != Some(self.command.as_slice()) {
            return Ok(false);
        }
        let key = wide(STARTUP_APPROVED);
        let mut approval = [0u8; 12];
        let mut size = size_of_val(&approval) as u32;
        // SAFETY: names are terminated and approval has the buffer capacity passed in size.
        let result = unsafe {
            RegGetValueW(
                HKEY_CURRENT_USER,
                key.as_ptr(),
                self.name.as_ptr(),
                RRF_RT_REG_BINARY,
                null_mut(),
                approval.as_mut_ptr().cast(),
                &mut size,
            )
        };
        if result == ERROR_FILE_NOT_FOUND {
            return Ok(true);
        }
        registry_result(result)?;
        if size != size_of_val(&approval) as u32 || approval[1..4] != [0, 0, 0] {
            return Err(io::ErrorKind::InvalidData.into());
        }
        match approval[0] {
            2 | 6 => Ok(true),
            3 | 7 => Ok(false),
            _ => Err(io::ErrorKind::InvalidData.into()),
        }
    }

    pub(crate) fn enabled(&self) -> io::Result<bool> {
        self.is_enabled(self.read()?.as_deref())
    }

    pub(crate) fn set(&self, enabled: bool) -> io::Result<()> {
        if enabled && self.command.len() > 261 {
            return Err(io::ErrorKind::InvalidInput.into());
        }
        self.replace(enabled.then_some(self.command.as_slice()))
    }

    pub(crate) fn replace(&self, command: Option<&[u16]>) -> io::Result<()> {
        let key = wide(RUN);
        // SAFETY: names are terminated and the optional payload is readable for the supplied length.
        let result = unsafe {
            match command {
                Some(command) => RegSetKeyValueW(
                    HKEY_CURRENT_USER,
                    key.as_ptr(),
                    self.name.as_ptr(),
                    REG_SZ,
                    command.as_ptr().cast(),
                    size_of_val(command) as u32,
                ),
                None => RegDeleteKeyValueW(HKEY_CURRENT_USER, key.as_ptr(), self.name.as_ptr()),
            }
        };
        if command.is_none() && result == ERROR_FILE_NOT_FOUND {
            return Ok(());
        }
        registry_result(result)
    }
}

fn registry_result(result: WIN32_ERROR) -> io::Result<()> {
    if result == ERROR_SUCCESS {
        Ok(())
    } else {
        Err(io::Error::from_raw_os_error(result as i32))
    }
}
