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
fn deletes_only_the_unreadable_pairing_and_keeps_delivery_credentials() {
    let directory = tempdir().unwrap();
    let path = directory.path().join("registration.json");
    fs::write(
        &path,
        json!({
            "version": 4,
            "receiver_id": "receiver",
            "protected_receiver_token": dpapi::protect(b"receiver-token"),
            "active_pairings": {
                "broken": {"sender_id": "first", "protected_key": [1, 2], "name": "Broken"},
                "short": {"sender_id": "second", "protected_key": dpapi::protect(&[42]), "name": "Short"},
                "working": {"sender_id": "third", "protected_key": dpapi::protect(&[7; 32]), "name": "Working"}
            }
        })
        .to_string(),
    )
    .unwrap();
    let store = RegistrationStore::new(path.clone());
    assert!(store.delete_unreadable_pairing("working").is_err());
    assert_eq!(
        store
            .sender_devices()
            .unwrap()
            .iter()
            .map(|device| (device.pairing_id.as_str(), device.needs_repair))
            .collect::<Vec<_>>(),
        [("broken", true), ("short", true), ("working", false)]
    );
    for (id, sender) in [("broken", "first"), ("short", "second")] {
        assert!(store.delivery_target("receiver", id, sender).is_none());
        store.delete_unreadable_pairing(id).unwrap();
        assert!(store.delete_unreadable_pairing(id).is_err());
    }
    let reopened = RegistrationStore::new(path);
    let (receiver, token) = reopened.receiver_credential().unwrap();
    assert_eq!(receiver, "receiver");
    assert_eq!(token.as_str(), "receiver-token");
    assert_eq!(reopened.sender_devices().unwrap().len(), 1);
    let delivery = reopened
        .delivery_target("receiver", "working", "third")
        .unwrap();
    assert_eq!(*delivery.key, [7; 32]);
    reopened
        .name_sender("working", &" Working ".encode_utf16().collect::<Vec<_>>())
        .unwrap();
    reopened
        .name_sender("working", &" Travel ".encode_utf16().collect::<Vec<_>>())
        .unwrap();
    let delivery = reopened
        .delivery_target("receiver", "working", "third")
        .unwrap();
    assert_eq!(delivery.name, "Travel");
}

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
        assert_eq!(
            store.sender_name("second").unwrap().as_deref(),
            Some("未命名设备-2")
        );
    }
    let name = "🚀".repeat(40);
    store
        .name_sender(
            "second",
            &format!("　{name}\t").encode_utf16().collect::<Vec<_>>(),
        )
        .unwrap();
    assert_eq!(
        RegistrationStore::new(path).sender_name("second").unwrap(),
        Some(name)
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
            "active-pairing": {"sender_id": "active-sender", "protected_key": [55, 66]},
            "named-pairing": {"sender_id": "named-sender", "protected_key": [77, 88], "name": "未命名设备-2"},
            "other-pairing": {"sender_id": "other-sender", "protected_key": [99, 11]}
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
    let expected = [
        ("active-pairing", "未命名设备"),
        ("named-pairing", "未命名设备-2"),
        ("other-pairing", "未命名设备-3"),
    ];
    let devices = store.sender_devices().unwrap();
    assert_eq!(
        devices
            .iter()
            .map(|device| (device.pairing_id.as_str(), device.name.as_str()))
            .collect::<Vec<_>>(),
        expected
    );
    let locked = OpenOptions::new()
        .read(true)
        .share_mode(1)
        .open(&path)
        .unwrap();
    assert!(
        store
            .commit_provisional(&reference("first"), &[42; 32], false)
            .is_err()
    );
    assert_eq!(fs::read(&path).unwrap(), legacy_bytes);
    assert_eq!(fs::read_dir(directory.path()).unwrap().count(), 1);
    drop(locked);

    store
        .commit_provisional(&reference("first"), &[42; 32], false)
        .unwrap();
    let current: Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
    assert_eq!(current["version"], 4);
    for field in ["receiver_id", "protected_receiver_token"] {
        assert_eq!(current[field], legacy[field]);
    }
    for (id, name) in expected {
        let pairing = &mut legacy["active_pairings"][id];
        pairing["name"] = json!(name);
        assert_eq!(&current["active_pairings"][id], pairing);
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
                .commit_provisional(&reference("session"), &[42; 32], false)
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
