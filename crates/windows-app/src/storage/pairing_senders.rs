use std::{
    collections::BTreeMap,
    io,
    path::PathBuf,
    sync::{Arc, LazyLock, Mutex, PoisonError, Weak},
};

use super::{PairingReference, RegistrationStore};

type Senders = BTreeMap<String, String>;
static WATCHED: LazyLock<Mutex<BTreeMap<PathBuf, Weak<PairingSenders>>>> =
    LazyLock::new(|| Mutex::new(BTreeMap::new()));

pub(crate) struct PairingSenders {
    receiver_id: String,
    senders: Mutex<Senders>,
}

impl PairingSenders {
    pub(crate) fn matches(&self, pairing_id: &str, sender_id: &str) -> io::Result<Option<bool>> {
        let senders = self
            .senders
            .lock()
            .map_err(|_| io::Error::other("Pairing sender identities are unavailable"))?;
        Ok(senders.get(pairing_id).map(|sender| sender == sender_id))
    }
}

impl RegistrationStore {
    pub(crate) fn watch_pairing_senders(
        &self,
        receiver_id: &str,
    ) -> io::Result<Arc<PairingSenders>> {
        let mut watched = WATCHED.lock().unwrap_or_else(PoisonError::into_inner);
        let registration = self.read()?;
        let mut senders = Senders::new();
        if registration.receiver_id.as_deref() == Some(receiver_id) {
            senders.extend(
                registration
                    .active_pairings
                    .into_iter()
                    .map(|(id, pairing)| (id, pairing.sender_id)),
            );
            if let Some(pairing) = registration.provisional_pairing
                && pairing.receiver_id == receiver_id
            {
                senders
                    .entry(pairing.pairing_id)
                    .or_insert(pairing.sender_id);
            }
        }
        let view = Arc::new(PairingSenders {
            receiver_id: receiver_id.into(),
            senders: Mutex::new(senders),
        });
        watched.retain(|_, view| view.strong_count() > 0);
        watched.insert(self.path.clone(), Arc::downgrade(&view));
        Ok(view)
    }

    pub(super) fn remember_pairing_sender(&self, reference: &PairingReference<'_>) {
        let watched = WATCHED.lock().unwrap_or_else(PoisonError::into_inner);
        if let Some(view) = watched.get(&self.path).and_then(Weak::upgrade)
            && view.receiver_id == reference.receiver_id
        {
            view.senders
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .insert(reference.pairing_id.into(), reference.sender_id.into());
        }
    }
}
