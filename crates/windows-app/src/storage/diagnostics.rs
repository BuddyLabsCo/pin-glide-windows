use std::{
    fs::OpenOptions,
    io::{self, Write},
    sync::Mutex,
};

use receiver_core::delivery::ReceiveError;

use super::RegistrationStore;

const MAX_LOG_BYTES: u64 = 16 * 1024;
static LOG_WRITER: Mutex<()> = Mutex::new(());

pub(crate) enum DeliveryRejection {
    InvalidEnvelope,
    ReceiverMismatch,
    InboxUnavailable,
    Receive(ReceiveError),
}

impl DeliveryRejection {
    fn record(self) -> &'static [u8] {
        match self {
            Self::InvalidEnvelope => b"InvalidEnvelope\n",
            Self::ReceiverMismatch => b"ReceiverMismatch\n",
            Self::InboxUnavailable => b"InboxUnavailable\n",
            Self::Receive(ReceiveError::UnsupportedCrypto) => b"UnsupportedCrypto\n",
            Self::Receive(ReceiveError::PairingKeyUnavailable) => b"PairingKeyUnavailable\n",
            Self::Receive(ReceiveError::InvalidEncoding) => b"InvalidEncoding\n",
            Self::Receive(ReceiveError::AuthenticationFailed) => b"AuthenticationFailed\n",
            Self::Receive(ReceiveError::InvalidPlaintext) => b"InvalidPlaintext\n",
        }
    }
}

impl RegistrationStore {
    pub(crate) fn record_delivery_rejection(&self, rejection: DeliveryRejection) -> io::Result<()> {
        let _writer = LOG_WRITER
            .lock()
            .map_err(|_| io::Error::other("diagnostic writer unavailable"))?;
        let record = rejection.record();
        let path = self.path.with_file_name("delivery-rejections.log");
        let mut log = OpenOptions::new().create(true).append(true).open(&path)?;
        if log.metadata()?.len() > MAX_LOG_BYTES - record.len() as u64 {
            drop(log);
            log = OpenOptions::new().write(true).truncate(true).open(path)?;
        }
        log.write_all(record)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejection_log_stays_bounded_and_keeps_the_record_that_triggered_reset() {
        let dir = tempfile::tempdir().unwrap();
        let store = RegistrationStore::new(dir.path().join("registration.json"));
        let path = dir.path().join("delivery-rejections.log");
        std::fs::write(&path, vec![0; MAX_LOG_BYTES as usize]).unwrap();
        store
            .record_delivery_rejection(DeliveryRejection::InvalidEnvelope)
            .unwrap();
        store
            .record_delivery_rejection(DeliveryRejection::ReceiverMismatch)
            .unwrap();
        assert_eq!(
            std::fs::read(&path).unwrap(),
            b"InvalidEnvelope\nReceiverMismatch\n"
        );
    }
}
