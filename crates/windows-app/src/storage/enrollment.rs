use std::{io, time::SystemTime};

use serde::{Deserialize, Serialize};
use zeroize::Zeroizing;

use super::{ProtectedPairing, RegistrationStore, protect, unprotect};

#[derive(Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Enrollment {
    pub origin: String,
    pub session_id: String,
    pub receiver_id: String,
    pub operation_id: String,
    pub expires_at: SystemTime,
    pub phase: EnrollmentPhase,
}

#[derive(Clone, Deserialize, Serialize)]
pub(crate) enum EnrollmentPhase {
    Attempt0(Vec<u8>),
    Attempt1(Vec<u8>),
    Issued(Vec<u8>),
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

    pub(crate) fn retry(&mut self) -> io::Result<()> {
        let EnrollmentPhase::Attempt0(blob) = &mut self.phase else {
            return Err(io::ErrorKind::InvalidData.into());
        };
        self.phase = EnrollmentPhase::Attempt1(std::mem::take(blob));
        Ok(())
    }

    pub(crate) fn issue(&mut self, token: &str) -> io::Result<()> {
        self.phase = EnrollmentPhase::Issued(protect(token.as_bytes())?);
        Ok(())
    }
}

impl RegistrationStore {
    pub(crate) fn enrollment(&self) -> io::Result<Option<Enrollment>> {
        let registration = self.read()?;
        if let Some(enrollment) = &registration.enrollment
            && !registration
                .provisional_pairing
                .as_ref()
                .is_some_and(|pairing| {
                    pairing.session_id == enrollment.session_id
                        && pairing.receiver_id == enrollment.receiver_id
                })
        {
            return Err(io::ErrorKind::InvalidData.into());
        }
        Ok(registration.enrollment)
    }

    pub(crate) fn save_enrollment(&self, enrollment: &Enrollment) -> io::Result<()> {
        self.update(|next| {
            if !next.provisional_pairing.as_ref().is_some_and(|pairing| {
                pairing.session_id == enrollment.session_id
                    && pairing.receiver_id == enrollment.receiver_id
            }) || next
                .enrollment
                .as_ref()
                .is_some_and(|stored| stored.operation_id != enrollment.operation_id)
            {
                return Err(io::ErrorKind::InvalidData.into());
            }
            next.enrollment = Some(enrollment.clone());
            Ok(())
        })
    }

    pub(crate) fn finish_enrollment(&self, operation_id: &str, completed: bool) -> io::Result<()> {
        self.update(|next| {
            let enrollment = next
                .enrollment
                .take()
                .filter(|pending| pending.operation_id == operation_id)
                .ok_or(io::ErrorKind::InvalidData)?;
            if completed {
                let EnrollmentPhase::Issued(token) = enrollment.phase else {
                    return Err(io::ErrorKind::InvalidData.into());
                };
                let pairing = next
                    .provisional_pairing
                    .take()
                    .filter(|pairing| {
                        pairing.session_id == enrollment.session_id
                            && pairing.receiver_id == enrollment.receiver_id
                    })
                    .ok_or(io::ErrorKind::InvalidData)?;
                if next.receiver_id.as_ref() != Some(&enrollment.receiver_id) {
                    next.active_pairings.clear();
                }
                next.receiver_id = Some(enrollment.receiver_id);
                next.protected_receiver_token = Some(token);
                next.active_pairings.insert(
                    pairing.pairing_id,
                    ProtectedPairing {
                        sender_id: pairing.sender_id,
                        protected_key: pairing.protected_key,
                    },
                );
            } else {
                next.provisional_pairing = None;
            }
            Ok(())
        })
    }
}
