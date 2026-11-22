use std::{
    fs::{self, OpenOptions},
    os::windows::fs::OpenOptionsExt,
    ptr::{null, null_mut},
};

use serde_json::{Value, json};
use tempfile::tempdir;
use windows_sys::Win32::{
    Foundation::LocalFree,
    Security::Cryptography::{CRYPT_INTEGER_BLOB, CRYPTPROTECT_UI_FORBIDDEN, CryptUnprotectData},
};

use super::{PairingReference, RegistrationStore};

#[test]
fn migrates_an_older_registration_and_preserves_it_if_replacement_fails() {
    let directory = tempdir().unwrap();
    let path = directory.path().join("registration.json");
    let legacy = json!({
        "version": 1,
        "receiver_id": "active-receiver",
        "protected_receiver_token": [66, 77],
        "active_pairings": {
            "active-pairing": {"sender_id": "active-sender", "protected_key": [55, 66]}
        }
    });
    let legacy_bytes = serde_json::to_vec(&legacy).unwrap();
    fs::write(&path, &legacy_bytes).unwrap();
    let store = RegistrationStore::new(path.clone());
    let locked = OpenOptions::new()
        .read(true)
        .share_mode(1)
        .open(&path)
        .unwrap();
    assert!(
        store
            .commit_provisional(&reference("first"), &[42; 32])
            .is_err()
    );
    assert_eq!(fs::read(&path).unwrap(), legacy_bytes);
    assert_eq!(fs::read_dir(directory.path()).unwrap().count(), 1);
    drop(locked);

    store
        .commit_provisional(&reference("first"), &[42; 32])
        .unwrap();
    let current: Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
    assert_eq!(current["version"], 2);
    for field in ["receiver_id", "protected_receiver_token", "active_pairings"] {
        assert_eq!(current[field], legacy[field]);
    }
    let pending = &current["provisional_pairing"];
    assert_eq!(pending["session_id"], "first");
    assert_eq!(pending["receiver_id"], "provisional-receiver");
    assert_eq!(pending["pairing_id"], "provisional-pairing");
    assert_eq!(pending["sender_id"], "provisional-sender");
    assert_eq!(unprotect(&pending["protected_key"]), [42; 32]);
}

#[test]
fn refuses_unreadable_and_newer_registration_files_without_overwriting_them() {
    let directory = tempdir().unwrap();
    let path = directory.path().join("registration.json");
    let store = RegistrationStore::new(path.clone());
    for content in [
        "broken JSON",
        r#"{"version":3,"active_pairings":{}}"#,
        r#"{"version":2,"active_pairings":{"unknown":{"protected_key":"invalid"}}}"#,
    ] {
        fs::write(&path, content).unwrap();
        assert!(
            store
                .commit_provisional(&reference("session"), &[42; 32])
                .is_err()
        );
        assert_eq!(fs::read_to_string(&path).unwrap(), content);
    }
}

fn reference(session_id: &str) -> PairingReference<'_> {
    PairingReference {
        session_id,
        receiver_id: "provisional-receiver",
        pairing_id: "provisional-pairing",
        sender_id: "provisional-sender",
    }
}

fn unprotect(blob: &Value) -> Vec<u8> {
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
