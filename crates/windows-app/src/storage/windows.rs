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
    Security::Cryptography::{CRYPT_INTEGER_BLOB, CRYPTPROTECT_UI_FORBIDDEN, CryptProtectData},
    System::Com::CoTaskMemFree,
    UI::Shell::{FOLDERID_LocalAppData, SHGetKnownFolderPath},
};

pub(super) fn local_app_data() -> io::Result<PathBuf> {
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

pub(super) fn protect(key: &[u8; 32]) -> io::Result<Vec<u8>> {
    let input = CRYPT_INTEGER_BLOB {
        cbData: 32,
        pbData: key.as_ptr().cast_mut(),
    };
    let mut output = CRYPT_INTEGER_BLOB::default();
    // SAFETY: DPAPI reads the fixed-size input and writes to the provided output blob.
    let success = unsafe {
        CryptProtectData(
            &input,
            null(),
            null(),
            null(),
            null(),
            CRYPTPROTECT_UI_FORBIDDEN,
            &mut output,
        )
    };
    if success == 0 {
        return Err(io::Error::last_os_error());
    }
    // SAFETY: successful DPAPI protection returns a valid local allocation with the declared size.
    unsafe {
        let protected = slice::from_raw_parts(output.pbData, output.cbData as usize).to_vec();
        LocalFree(output.pbData.cast());
        Ok(protected)
    }
}
