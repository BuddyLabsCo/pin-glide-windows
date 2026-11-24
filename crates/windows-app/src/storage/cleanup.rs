use std::{
    collections::BTreeMap,
    fs::File,
    io::{self, BufReader},
    path::PathBuf,
    sync::{LazyLock, Mutex, MutexGuard, PoisonError},
    thread,
    time::Duration,
};

use serde::{Deserialize, Serialize};

use super::{
    Registration, RegistrationStore, registration_writer, remove_file_if_present, replace_json,
};

#[derive(Clone, Debug, Eq, PartialEq, Deserialize, Serialize)]
pub(crate) enum Cleanup {
    PairingRemoved {
        receiver_id: String,
        pairing_id: String,
        sender_id: String,
    },
    Revoked {
        receiver_id: String,
    },
}

type Pending = BTreeMap<PathBuf, Vec<Cleanup>>;
static PENDING: LazyLock<Mutex<Pending>> = LazyLock::new(|| Mutex::new(BTreeMap::new()));

fn pending() -> MutexGuard<'static, Pending> {
    PENDING.lock().unwrap_or_else(PoisonError::into_inner)
}

impl Cleanup {
    pub(super) fn revokes(&self, receiver_id: &str) -> bool {
        matches!(self, Self::Revoked { receiver_id: revoked } if revoked == receiver_id)
    }

    pub(super) fn apply(&self, registration: &mut Registration) -> bool {
        let Some(active_receiver) = registration.receiver_id.as_deref() else {
            return false;
        };
        if self.revokes(active_receiver) {
            *registration = Registration {
                receiver_revoked: true,
                ..Registration::default()
            };
            return true;
        }
        let Self::PairingRemoved {
            receiver_id,
            pairing_id,
            sender_id,
        } = self
        else {
            return false;
        };
        if receiver_id != active_receiver {
            return false;
        }
        let active = registration
            .active_pairings
            .get(pairing_id)
            .is_some_and(|pairing| &pairing.sender_id == sender_id);
        if active {
            registration.active_pairings.remove(pairing_id);
        }
        let provisional = registration
            .provisional_pairing
            .as_ref()
            .is_some_and(|pairing| {
                &pairing.receiver_id == receiver_id
                    && &pairing.pairing_id == pairing_id
                    && &pairing.sender_id == sender_id
            });
        if provisional {
            registration.provisional_pairing = None;
            registration.enrollment = None;
        }
        active || provisional
    }
}

impl RegistrationStore {
    pub(crate) fn record_cleanup(&self, event: Cleanup) {
        self.queue_cleanup(Some(event));
    }

    pub(crate) fn resume_cleanup(&self) {
        if self.cleanup_path().exists() {
            self.queue_cleanup(None);
        }
    }

    fn queue_cleanup(&self, event: Option<Cleanup>) {
        let mut pending = pending();
        if let Some(entries) = pending.get_mut(&self.path) {
            if let Some(event) = event
                && !entries.contains(&event)
            {
                entries.push(event);
            }
            return;
        }
        pending.insert(self.path.clone(), event.into_iter().collect());
        let store = self.clone();
        thread::spawn(move || store.retry_cleanup());
    }

    fn retry_cleanup(&self) {
        loop {
            let snapshot = pending()[&self.path].clone();
            if let Err(error) = self.flush_cleanup(&snapshot) {
                eprintln!(
                    "Could not persist Receiver cleanup. Will retry: {}",
                    error.kind()
                );
                thread::sleep(Duration::from_secs(1));
                continue;
            }
            let mut pending = pending();
            if pending[&self.path] == snapshot {
                pending.remove(&self.path);
                return;
            }
        }
    }

    fn cleanup_path(&self) -> PathBuf {
        self.path.with_extension("cleanup.json")
    }

    fn read_cleanup(&self) -> io::Result<Vec<Cleanup>> {
        match File::open(self.cleanup_path()) {
            Ok(file) => Ok(serde_json::from_reader(BufReader::new(file))?),
            Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(Vec::new()),
            Err(error) => Err(error),
        }
    }

    pub(super) fn known_cleanup(&self) -> io::Result<Vec<Cleanup>> {
        let mut durable = self.read_cleanup()?;
        if let Some(entries) = pending().get(&self.path) {
            merge(&mut durable, entries);
        }
        Ok(durable)
    }

    fn flush_cleanup(&self, events: &[Cleanup]) -> io::Result<()> {
        let _writer = registration_writer()?;
        let journal = self.cleanup_path();
        let mut durable = self.read_cleanup()?;
        if merge(&mut durable, events) {
            replace_json(&journal, &durable)?;
        }
        let mut registration = Self::read_file(&self.path)?;
        let mut changed = false;
        for event in &durable {
            changed |= event.apply(&mut registration);
        }
        if changed {
            registration.version = super::CURRENT_VERSION;
            replace_json(&self.path, &registration)?;
        }
        let staged = Self::read_file(&self.enrollment_path())?;
        if staged.provisional_pairing.is_some_and(|pairing| {
            durable
                .iter()
                .any(|event| event.revokes(&pairing.receiver_id))
        }) {
            remove_file_if_present(&self.enrollment_path())?;
        }
        remove_file_if_present(&journal)
    }
}

fn merge(durable: &mut Vec<Cleanup>, entries: &[Cleanup]) -> bool {
    let before = durable.len();
    for entry in entries {
        if !durable.contains(entry) {
            durable.push(entry.clone());
        }
    }
    durable.len() != before
}
