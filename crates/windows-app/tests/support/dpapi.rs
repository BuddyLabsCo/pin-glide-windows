use std::ptr::{null, null_mut};

use serde_json::Value;
use windows_sys::Win32::{
    Foundation::LocalFree,
    Security::Cryptography::{CRYPT_INTEGER_BLOB, CRYPTPROTECT_UI_FORBIDDEN, CryptUnprotectData},
};

pub fn unprotect(blob: &Value) -> Vec<u8> {
    let mut bytes: Vec<u8> = serde_json::from_value(blob.clone()).unwrap();
    let input = CRYPT_INTEGER_BLOB {
        cbData: bytes.len().try_into().unwrap(),
        pbData: bytes.as_mut_ptr(),
    };
    let mut output = CRYPT_INTEGER_BLOB::default();
    // SAFETY: the input is readable for its declared size and output is writable.
    let ok = unsafe {
        CryptUnprotectData(
            &input,
            null_mut(),
            null(),
            null(),
            null(),
            CRYPTPROTECT_UI_FORBIDDEN,
            &mut output,
        )
    };
    assert_ne!(ok, 0);
    // SAFETY: successful DPAPI unprotection returns a valid allocation owned by this call.
    unsafe {
        let plaintext = std::slice::from_raw_parts(output.pbData, output.cbData as usize).to_vec();
        LocalFree(output.pbData.cast());
        plaintext
    }
}

#[allow(dead_code)]
pub fn protect(plaintext: &[u8]) -> Vec<u8> {
    use windows_sys::Win32::Security::Cryptography::CryptProtectData;

    let input = CRYPT_INTEGER_BLOB {
        cbData: plaintext.len().try_into().unwrap(),
        pbData: plaintext.as_ptr().cast_mut(),
    };
    let mut output = CRYPT_INTEGER_BLOB::default();
    // SAFETY: DPAPI reads the input and allocates the output.
    let ok = unsafe {
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
    assert_ne!(ok, 0);
    // SAFETY: the successful call supplied an allocation of the declared size.
    unsafe {
        let protected = std::slice::from_raw_parts(output.pbData, output.cbData as usize).to_vec();
        LocalFree(output.pbData.cast());
        protected
    }
}
