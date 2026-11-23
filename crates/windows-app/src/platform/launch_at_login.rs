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
    run: Vec<u16>,
    approval: Vec<u16>,
    check_command_limit: bool,
}

impl LaunchAtLogin {
    pub(crate) fn for_current_executable() -> io::Result<Self> {
        let executable = std::env::current_exe()?;
        let command = [b'"' as u16]
            .into_iter()
            .chain(executable.as_os_str().encode_wide())
            .chain(wide("\" --startup"))
            .collect();
        let mut startup = Self {
            name: wide("Pin Glide"),
            command,
            run: wide(RUN),
            approval: wide(STARTUP_APPROVED),
            check_command_limit: true,
        };
        if let Some(directory) = crate::test_environment::directory()
            && let Some(name) = crate::test_environment::name(&directory)
        {
            startup.run = wide(&format!(r"Software\BuddyLabsCo\{name}\Run"));
            startup.approval = wide(&format!(r"Software\BuddyLabsCo\{name}\StartupApproved"));
            startup.check_command_limit = false;
        }
        Ok(startup)
    }

    #[cfg(test)]
    pub(crate) fn for_test(name: &str) -> Self {
        Self {
            name: wide(name),
            command: wide("\"C:\\Pin Glide test\\pin-glide.exe\" --startup"),
            run: wide(r"Software\BuddyLabsCo\PinGlideUnitTests\Run"),
            approval: wide(r"Software\BuddyLabsCo\PinGlideUnitTests\StartupApproved"),
            check_command_limit: true,
        }
    }

    #[cfg(test)]
    pub(crate) fn set_test_approval(&self, enabled: bool) {
        use windows_sys::Win32::System::Registry::REG_BINARY;
        let denied = [3u8, 0, 0, 0, 1, 2, 3, 4, 5, 6, 7, 8];
        // SAFETY: names are terminated and the test uses a unique value outside Windows startup.
        let result = unsafe {
            if enabled {
                RegDeleteKeyValueW(
                    HKEY_CURRENT_USER,
                    self.approval.as_ptr(),
                    self.name.as_ptr(),
                )
            } else {
                RegSetKeyValueW(
                    HKEY_CURRENT_USER,
                    self.approval.as_ptr(),
                    self.name.as_ptr(),
                    REG_BINARY,
                    denied.as_ptr().cast(),
                    denied.len() as u32,
                )
            }
        };
        assert!(result == ERROR_SUCCESS || (enabled && result == ERROR_FILE_NOT_FOUND));
    }

    pub(crate) fn read(&self) -> io::Result<Option<Vec<u16>>> {
        let mut size = 0;
        // SAFETY: names are terminated and the call writes only the required buffer size.
        let result = unsafe {
            RegGetValueW(
                HKEY_CURRENT_USER,
                self.run.as_ptr(),
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
                self.run.as_ptr(),
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

    pub(crate) fn approved(&self) -> io::Result<bool> {
        let mut approval = [0u8; 12];
        let mut size = size_of_val(&approval) as u32;
        // SAFETY: names are terminated and approval has the buffer capacity passed in size.
        let result = unsafe {
            RegGetValueW(
                HKEY_CURRENT_USER,
                self.approval.as_ptr(),
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
        Ok(self.read()?.as_deref() == Some(self.command.as_slice()) && self.approved()?)
    }

    pub(crate) fn command_for(&self, enabled: bool) -> io::Result<Option<&[u16]>> {
        if enabled && self.check_command_limit && self.command.len() > 261 {
            return Err(io::ErrorKind::InvalidInput.into());
        }
        Ok(enabled.then_some(self.command.as_slice()))
    }

    #[cfg(test)]
    pub(crate) fn set(&self, enabled: bool) -> io::Result<()> {
        self.replace(self.command_for(enabled)?)
    }

    pub(crate) fn replace(&self, command: Option<&[u16]>) -> io::Result<()> {
        // SAFETY: names are terminated and the optional payload is readable for the supplied length.
        let result = unsafe {
            match command {
                Some(command) => RegSetKeyValueW(
                    HKEY_CURRENT_USER,
                    self.run.as_ptr(),
                    self.name.as_ptr(),
                    REG_SZ,
                    command.as_ptr().cast(),
                    size_of_val(command) as u32,
                ),
                None => {
                    RegDeleteKeyValueW(HKEY_CURRENT_USER, self.run.as_ptr(), self.name.as_ptr())
                }
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
