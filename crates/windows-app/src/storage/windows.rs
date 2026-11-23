use std::{
    ffi::OsString,
    io,
    os::windows::ffi::OsStringExt,
    path::PathBuf,
    ptr::{null, null_mut},
    slice,
};

use windows_sys::Win32::{
    Foundation::LocalFree,
    Security::Cryptography::{
        CRYPT_INTEGER_BLOB, CRYPTPROTECT_UI_FORBIDDEN, CryptProtectData, CryptUnprotectData,
    },
    System::Com::CoTaskMemFree,
    UI::Shell::{FOLDERID_LocalAppData, SHGetKnownFolderPath},
};
use zeroize::{Zeroize, Zeroizing};

pub(super) fn app_data() -> io::Result<PathBuf> {
    if let Some(path) = crate::test_environment::directory() {
        return Ok(path);
    }
    local_app_data().map(|path| path.join("Pin Glide"))
}

fn local_app_data() -> io::Result<PathBuf> {
    let mut path = null_mut();
    // SAFETY: the known-folder ID and output pointer are valid and the null token selects this user.
    let result = unsafe { SHGetKnownFolderPath(&FOLDERID_LocalAppData, 0, null_mut(), &mut path) };
    if result < 0 {
        // SAFETY: Shell requires freeing its output on success and failure, including null.
        unsafe { CoTaskMemFree(path.cast()) };
        return Err(io::Error::from_raw_os_error(result));
    }
    // SAFETY: on success Shell allocates a NUL-terminated UTF-16 path.
    let output = unsafe {
        let length = (0..).find(|&index| *path.add(index) == 0).unwrap();
        let output = OsString::from_wide(slice::from_raw_parts(path, length));
        CoTaskMemFree(path.cast());
        output
    };
    Ok(output.into())
}

pub(super) fn protect(secret: &[u8]) -> io::Result<Vec<u8>> {
    // SAFETY: DPAPI reads the input and writes to the provided output blob.
    dpapi(secret, |input, output| unsafe {
        CryptProtectData(
            input,
            null(),
            null(),
            null(),
            null(),
            CRYPTPROTECT_UI_FORBIDDEN,
            output,
        )
    })
}

pub(super) fn unprotect(blob: &[u8]) -> io::Result<Zeroizing<Vec<u8>>> {
    // SAFETY: DPAPI reads the input and writes to a local allocation.
    dpapi(blob, |input, output| unsafe {
        CryptUnprotectData(
            input,
            null_mut(),
            null(),
            null(),
            null(),
            CRYPTPROTECT_UI_FORBIDDEN,
            output,
        )
    })
    .map(Zeroizing::new)
}

fn dpapi(
    blob: &[u8],
    transform: impl FnOnce(&CRYPT_INTEGER_BLOB, &mut CRYPT_INTEGER_BLOB) -> i32,
) -> io::Result<Vec<u8>> {
    if blob.is_empty() {
        return Err(io::ErrorKind::InvalidData.into());
    }
    let input = CRYPT_INTEGER_BLOB {
        cbData: blob.len().try_into().map_err(io::Error::other)?,
        pbData: blob.as_ptr().cast_mut(),
    };
    let mut output = CRYPT_INTEGER_BLOB::default();
    if transform(&input, &mut output) == 0 {
        return Err(io::Error::last_os_error());
    }
    if output.pbData.is_null() {
        return Err(io::ErrorKind::InvalidData.into());
    }
    // SAFETY: the successful output allocation is writable, and is freed after copying and clearing it.
    unsafe {
        let bytes = slice::from_raw_parts_mut(output.pbData, output.cbData as usize);
        let result = bytes.to_vec();
        bytes.zeroize();
        LocalFree(output.pbData.cast());
        Ok(result)
    }
}
