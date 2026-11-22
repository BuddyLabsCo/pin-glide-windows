use std::{
    collections::BTreeMap,
    fs::{self, File},
    io::{self, BufReader},
    path::PathBuf,
    sync::Mutex,
};

use serde::{Deserialize, Serialize};

mod enrollment;
pub(crate) use enrollment::Enrollment;
mod sender_names;
#[cfg(windows)]
pub(crate) use sender_names::SenderNameError;

#[cfg(all(test, windows))]
mod tests;
#[cfg(windows)]
mod windows;

const CURRENT_VERSION: u32 = 4;
static WRITER: Mutex<()> = Mutex::new(());

#[derive(Clone)]
pub(crate) struct RegistrationStore {
    path: PathBuf,
}

pub(crate) struct PairingReference<'a> {
    pub session_id: &'a str,
    pub receiver_id: &'a str,
    pub pairing_id: &'a str,
    pub sender_id: &'a str,
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct ProtectedPairing {
    sender_id: String,
    protected_key: Vec<u8>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    name: Option<String>,
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct ProvisionalPairing {
    session_id: String,
    receiver_id: String,
    pairing_id: String,
    sender_id: String,
    protected_key: Vec<u8>,
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Registration {
    version: u32,
    receiver_id: Option<String>,
    protected_receiver_token: Option<Vec<u8>>,
    active_pairings: BTreeMap<String, ProtectedPairing>,
    provisional_pairing: Option<ProvisionalPairing>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    enrollment: Option<Enrollment>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct LegacyRegistration {
    version: u32,
    receiver_id: Option<String>,
    protected_receiver_token: Option<Vec<u8>>,
    active_pairings: BTreeMap<String, ProtectedPairing>,
}

impl RegistrationStore {
    pub(crate) fn new(path: PathBuf) -> Self {
        Self { path }
    }

    pub(crate) fn for_current_user() -> io::Result<Self> {
        #[cfg(windows)]
        return Ok(Self::new(
            windows::local_app_data()?
                .join("Pin Glide")
                .join("registration.json"),
        ));
        #[cfg(not(windows))]
        Err(io::ErrorKind::Unsupported.into())
    }

    pub(crate) fn commit_provisional(
        &self,
        reference: &PairingReference<'_>,
        key: &[u8; 32],
    ) -> io::Result<()> {
        let pairing = ProvisionalPairing {
            session_id: reference.session_id.into(),
            receiver_id: reference.receiver_id.into(),
            pairing_id: reference.pairing_id.into(),
            sender_id: reference.sender_id.into(),
            protected_key: protect(key)?,
        };
        self.update(|next| {
            if next.enrollment.is_some() {
                return Err(io::ErrorKind::AlreadyExists.into());
            }
            next.provisional_pairing = Some(pairing);
            Ok(())
        })
    }

    fn update<Value, Error>(
        &self,
        change: impl FnOnce(&mut Registration) -> Result<Value, Error>,
    ) -> Result<Value, Error>
    where
        Error: From<io::Error>,
    {
        let _writer = WRITER
            .lock()
            .map_err(|_| io::Error::other("registration writer is unavailable"))?;
        let mut next = self.read()?;
        let value = change(&mut next)?;
        next.version = CURRENT_VERSION;
        let directory = self
            .path
            .parent()
            .ok_or_else(|| io::Error::from(io::ErrorKind::InvalidInput))?;
        fs::create_dir_all(directory)?;
        let mut stage = tempfile::NamedTempFile::new_in(directory)?;
        serde_json::to_writer(stage.as_file_mut(), &next).map_err(io::Error::from)?;
        stage.as_file().sync_all()?;
        stage.persist(&self.path).map_err(|error| error.error)?;
        Ok(value)
    }

    fn read(&self) -> io::Result<Registration> {
        let mut file = match File::open(&self.path) {
            Ok(file) => file,
            Err(error) if error.kind() == io::ErrorKind::NotFound => {
                return Ok(Registration {
                    version: CURRENT_VERSION,
                    receiver_id: None,
                    protected_receiver_token: None,
                    active_pairings: BTreeMap::new(),
                    provisional_pairing: None,
                    enrollment: None,
                });
            }
            Err(error) => return Err(error),
        };
        #[derive(Deserialize)]
        struct Version {
            version: u32,
        }
        let Version { version } = serde_json::from_reader(BufReader::new(&file))?;
        use io::Seek;
        file.rewind()?;
        let mut stored = match version {
            1 => {
                let legacy: LegacyRegistration = serde_json::from_reader(BufReader::new(file))?;
                if legacy.version != 1 {
                    return Err(io::ErrorKind::InvalidData.into());
                }
                Registration {
                    version: CURRENT_VERSION,
                    receiver_id: legacy.receiver_id,
                    protected_receiver_token: legacy.protected_receiver_token,
                    active_pairings: legacy.active_pairings,
                    provisional_pairing: None,
                    enrollment: None,
                }
            }
            2..=CURRENT_VERSION => {
                let stored: Registration = serde_json::from_reader(BufReader::new(file))?;
                if stored.version != version || (version == 2 && stored.enrollment.is_some()) {
                    return Err(io::ErrorKind::InvalidData.into());
                }
                stored
            }
            _ => return Err(io::ErrorKind::InvalidData.into()),
        };
        stored.backfill_sender_names();
        Ok(stored)
    }
}

#[cfg(windows)]
use windows::{protect, unprotect};

#[cfg(not(windows))]
fn protect(_: &[u8]) -> io::Result<Vec<u8>> {
    Err(io::ErrorKind::Unsupported.into())
}

#[cfg(not(windows))]
fn unprotect(_: &[u8]) -> io::Result<zeroize::Zeroizing<Vec<u8>>> {
    Err(io::ErrorKind::Unsupported.into())
}
