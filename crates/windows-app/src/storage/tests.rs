use std::{
    fs::{self, OpenOptions},
    os::windows::fs::OpenOptionsExt,
};

use serde_json::{Value, json};
use tempfile::tempdir;
#[path = "../../tests/support/dpapi.rs"]
mod dpapi;

use super::{PairingReference, RegistrationStore};

#[test]
fn saves_only_a_valid_unique_sender_name_and_preserves_it_on_failure() {
    use super::SenderNameError::{Duplicate, Empty, InvalidCharacter, TooLong};

    let directory = tempdir().unwrap();
    let path = directory.path().join("registration.json");
    fs::write(
        &path,
        json!({
            "version": 4,
            "receiver_id": "receiver",
            "active_pairings": {
                "first": {"sender_id": "sender-1", "protected_key": [], "name": "Σender"},
                "second": {"sender_id": "sender-2", "protected_key": [], "name": "未命名设备-2"}
            }
        })
        .to_string(),
    )
    .unwrap();
    let store = RegistrationStore::new(path.clone());
    for (input, expected) in [
        ("\u{3000}\t".encode_utf16().collect::<Vec<_>>(), Empty),
        ("ςENDER".encode_utf16().collect(), Duplicate),
        ("🚀".repeat(41).encode_utf16().collect(), TooLong),
        (vec![0xd800], InvalidCharacter),
    ] {
        assert!(matches!(store.name_sender("second", &input), Err(error) if error == expected));
        assert_eq!(store.sender_names().unwrap()["second"], "未命名设备-2");
    }
    let name = "🚀".repeat(40);
    store
        .name_sender(
            "second",
            &format!("　{name}\t").encode_utf16().collect::<Vec<_>>(),
        )
        .unwrap();
    assert_eq!(
        RegistrationStore::new(path).sender_names().unwrap()["second"],
        name
    );
}

#[test]
fn migrates_an_older_registration_and_preserves_it_if_replacement_fails() {
    for version in [1, 2, 3] {
        verify_migration(version);
    }
}

fn verify_migration(version: u32) {
    let directory = tempdir().unwrap();
    let path = directory.path().join("registration.json");
    let mut legacy = json!({
        "version": version,
        "receiver_id": "active-receiver",
        "protected_receiver_token": [66, 77],
        "active_pairings": {
            "active-pairing": {"sender_id": "active-sender", "protected_key": [55, 66]}
        }
    });
    if version == 2 {
        legacy["provisional_pairing"] = json!({
            "session_id": "older-session",
            "receiver_id": "older-receiver",
            "pairing_id": "older-pairing",
            "sender_id": "older-sender",
            "protected_key": [44, 55],
        });
    }
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
    assert_eq!(current["version"], 4);
    for field in ["receiver_id", "protected_receiver_token", "active_pairings"] {
        assert_eq!(current[field], legacy[field]);
    }
    let pending = &current["provisional_pairing"];
    assert_eq!(pending["session_id"], "first");
    assert_eq!(pending["receiver_id"], "provisional-receiver");
    assert_eq!(pending["pairing_id"], "provisional-pairing");
    assert_eq!(pending["sender_id"], "provisional-sender");
    assert_eq!(dpapi::unprotect(&pending["protected_key"]), [42; 32]);
}

#[test]
fn refuses_unreadable_and_newer_registration_files_without_overwriting_them() {
    let directory = tempdir().unwrap();
    let path = directory.path().join("registration.json");
    let store = RegistrationStore::new(path.clone());
    for content in [
        "broken JSON",
        r#"{"version":5,"active_pairings":{}}"#,
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
