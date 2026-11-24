use std::{io, time::SystemTime};

use receiver_core::pairing::EnrollmentPhase;
use serde::{Deserialize, Serialize};
use zeroize::Zeroizing;

use super::{
    ProtectedPairing, ProvisionalPairing, Registration, RegistrationStore, protect,
    registration_writer, remove_file_if_present, replace_json, unprotect,
};

#[derive(Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Enrollment {
    pub origin: String,
    pub session_id: String,
    pub receiver_id: String,
    pub operation_id: String,
    pub expires_at: SystemTime,
    pub phase: EnrollmentPhase<Vec<u8>>,
}

impl Enrollment {
    pub(crate) fn new(
        origin: String,
        session_id: String,
        receiver_id: String,
        operation_id: String,
        expires_at: SystemTime,
        pairing_token: &str,
    ) -> io::Result<Self> {
        Ok(Self {
            origin,
            session_id,
            receiver_id,
            operation_id,
            expires_at,
            phase: EnrollmentPhase::Attempt0(protect(pairing_token.as_bytes())?),
        })
    }

    pub(crate) fn token(&self) -> io::Result<Zeroizing<String>> {
        let (EnrollmentPhase::Attempt0(blob)
        | EnrollmentPhase::Attempt1(blob)
        | EnrollmentPhase::Issued(blob)) = &self.phase;
        let bytes = unprotect(blob)?;
        let token = std::str::from_utf8(&bytes).map_err(io::Error::other)?;
        Ok(Zeroizing::new(token.to_owned()))
    }

    pub(crate) fn protect_token(token: &str) -> io::Result<Vec<u8>> {
        protect(token.as_bytes())
    }

    fn matches(&self, pairing: &ProvisionalPairing) -> bool {
        pairing.session_id == self.session_id && pairing.receiver_id == self.receiver_id
    }
}

impl RegistrationStore {
    pub(super) fn enrollment_path(&self) -> std::path::PathBuf {
        self.path.with_extension("pending.json")
    }

    pub(super) fn stage_enrollment_key(&self, pairing: ProvisionalPairing) -> io::Result<()> {
        let _writer = registration_writer()?;
        if let Err(error) = self.read()
            && error.kind() == io::ErrorKind::Unsupported
        {
            return Err(error);
        }
        if self.enrollment()?.is_some() {
            return Err(io::ErrorKind::AlreadyExists.into());
        }
        replace_json(
            &self.enrollment_path(),
            &Registration {
                provisional_pairing: Some(pairing),
                ..Registration::default()
            },
        )
    }

    pub(crate) fn discard_enrollment_key(&self, session_id: &str) -> io::Result<()> {
        let _writer = registration_writer()?;
        let path = self.enrollment_path();
        let staged = Self::read_file(&path)?;
        if staged.enrollment.is_none()
            && staged
                .provisional_pairing
                .is_some_and(|pairing| pairing.session_id == session_id)
        {
            remove_file_if_present(&path)?;
        }
        Ok(())
    }

    fn staged_enrollment(&self) -> io::Result<Registration> {
        let cleanup = self.known_cleanup()?;
        let mut staged = Self::read_file(&self.enrollment_path())?;
        if staged.provisional_pairing.as_ref().is_some_and(|pairing| {
            cleanup
                .iter()
                .any(|event| event.revokes(&pairing.receiver_id))
        }) {
            staged = Registration::default();
        }
        Ok(staged)
    }

    pub(crate) fn enrollment(&self) -> io::Result<Option<Enrollment>> {
        let staged = self.staged_enrollment()?;
        let active = self.read();
        if let Some(pending) = staged.checked_enrollment()? {
            if let Ok(active) = active
                && active.receiver_id.as_ref() == Some(&pending.receiver_id)
                && matches!(&pending.phase, EnrollmentPhase::Issued(token)
                    if active.protected_receiver_token.as_ref() == Some(token))
            {
                return Ok(None);
            }
            return Ok(Some(pending));
        }
        match active {
            Ok(active) if staged.provisional_pairing.is_none() => active.checked_enrollment(),
            _ => Ok(None),
        }
    }

    pub(crate) fn save_enrollment(&self, enrollment: &Enrollment) -> io::Result<()> {
        let _writer = registration_writer()?;
        let mut staged = self.staged_enrollment()?;
        if staged.provisional_pairing.is_some() {
            staged.save_enrollment(enrollment)?;
            return replace_json(&self.enrollment_path(), &staged);
        }
        let mut active = self.read()?;
        active.save_enrollment(enrollment)?;
        replace_json(&self.path, &active)
    }

    pub(crate) fn finish_enrollment(
        &self,
        operation_id: &str,
        completed: bool,
    ) -> io::Result<Option<String>> {
        let _writer = registration_writer()?;
        let mut staged = self.staged_enrollment()?;
        if staged.enrollment.is_some() {
            let (enrollment, pairing) = staged.take_enrollment(operation_id)?;
            let path = self.enrollment_path();
            if !completed {
                remove_file_if_present(&path)?;
                return Ok(None);
            }
            let mut active = match self.read() {
                Ok(active) => active,
                Err(error)
                    if matches!(
                        error.kind(),
                        io::ErrorKind::InvalidData | io::ErrorKind::UnexpectedEof
                    ) =>
                {
                    Registration::default()
                }
                Err(error) => return Err(error),
            };
            let pairing_id = active.promote(enrollment, pairing)?;
            replace_json(&self.path, &active)?;
            let _ = remove_file_if_present(&path);
            return Ok(Some(pairing_id));
        }
        let mut active = self.read()?;
        let (enrollment, pairing) = active.take_enrollment(operation_id)?;
        let pairing_id = completed
            .then(|| active.promote(enrollment, pairing))
            .transpose()?;
        replace_json(&self.path, &active)?;
        Ok(pairing_id)
    }
}

impl Registration {
    fn checked_enrollment(&self) -> io::Result<Option<Enrollment>> {
        if let Some(enrollment) = &self.enrollment
            && !self
                .provisional_pairing
                .as_ref()
                .is_some_and(|pairing| enrollment.matches(pairing))
        {
            return Err(io::ErrorKind::InvalidData.into());
        }
        Ok(self.enrollment.clone())
    }

    fn save_enrollment(&mut self, enrollment: &Enrollment) -> io::Result<()> {
        if !self
            .provisional_pairing
            .as_ref()
            .is_some_and(|pairing| enrollment.matches(pairing))
            || self
                .enrollment
                .as_ref()
                .is_some_and(|stored| stored.operation_id != enrollment.operation_id)
        {
            return Err(io::ErrorKind::InvalidData.into());
        }
        self.version = super::CURRENT_VERSION;
        self.enrollment = Some(enrollment.clone());
        Ok(())
    }

    fn take_enrollment(
        &mut self,
        operation_id: &str,
    ) -> io::Result<(Enrollment, ProvisionalPairing)> {
        let enrollment = self
            .enrollment
            .take()
            .filter(|pending| pending.operation_id == operation_id)
            .ok_or(io::ErrorKind::InvalidData)?;
        let pairing = self
            .provisional_pairing
            .take()
            .filter(|pairing| enrollment.matches(pairing))
            .ok_or(io::ErrorKind::InvalidData)?;
        Ok((enrollment, pairing))
    }

    fn promote(
        &mut self,
        enrollment: Enrollment,
        pairing: ProvisionalPairing,
    ) -> io::Result<String> {
        let EnrollmentPhase::Issued(token) = enrollment.phase else {
            return Err(io::ErrorKind::InvalidData.into());
        };
        if self.receiver_id.as_ref() != Some(&enrollment.receiver_id) {
            self.active_pairings.clear();
        }
        self.version = super::CURRENT_VERSION;
        self.receiver_id = Some(enrollment.receiver_id);
        self.protected_receiver_token = Some(token);
        self.receiver_revoked = false;
        self.provisional_pairing = None;
        self.enrollment = None;
        let name = self
            .active_pairings
            .get(&pairing.pairing_id)
            .and_then(|stored| stored.name.clone())
            .unwrap_or_else(|| self.first_available_name());
        self.active_pairings.insert(
            pairing.pairing_id.clone(),
            ProtectedPairing {
                sender_id: pairing.sender_id,
                protected_key: pairing.protected_key,
                name: Some(name),
            },
        );
        Ok(pairing.pairing_id)
    }
}
