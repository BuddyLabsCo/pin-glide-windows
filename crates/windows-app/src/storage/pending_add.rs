use std::{collections::BTreeMap, path::PathBuf, sync::LazyLock};

use tokio::sync::watch;

use super::{PairingReference, RegistrationStore};

#[derive(PartialEq)]
pub(crate) struct PendingAdd {
    session_id: String,
    receiver_id: String,
    sender_id: String,
    pairing_id: String,
}

type PendingAdds = BTreeMap<PathBuf, PendingAdd>;
static PENDING: LazyLock<watch::Sender<PendingAdds>> =
    LazyLock::new(|| watch::channel(BTreeMap::new()).0);

pub(crate) struct AddSenderGuard<'a> {
    store: &'a RegistrationStore,
    session_id: &'a str,
}

impl RegistrationStore {
    pub(crate) fn watch_pending_adds(&self) -> watch::Receiver<PendingAdds> {
        PENDING.subscribe()
    }

    pub(crate) fn is_pending_add(
        &self,
        pending: &PendingAdds,
        receiver_id: &str,
        sender_id: &str,
        pairing_id: &str,
    ) -> bool {
        pending.get(&self.path).is_some_and(|entry| {
            entry.receiver_id == receiver_id
                && entry.sender_id == sender_id
                && entry.pairing_id == pairing_id
        })
    }

    pub(crate) fn track_pending_add(&self, reference: &PairingReference<'_>) {
        PENDING.send_modify(|pending| {
            pending.insert(
                self.path.clone(),
                PendingAdd {
                    session_id: reference.session_id.into(),
                    receiver_id: reference.receiver_id.into(),
                    sender_id: reference.sender_id.into(),
                    pairing_id: reference.pairing_id.into(),
                },
            );
        });
    }

    pub(crate) fn pending_add_guard<'a>(&'a self, session_id: &'a str) -> AddSenderGuard<'a> {
        AddSenderGuard {
            store: self,
            session_id,
        }
    }
}

impl Drop for AddSenderGuard<'_> {
    fn drop(&mut self) {
        PENDING.send_if_modified(|pending| {
            let current = pending
                .get(&self.store.path)
                .is_some_and(|entry| entry.session_id == self.session_id);
            if current {
                pending.remove(&self.store.path);
            }
            current
        });
    }
}
