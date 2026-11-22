use super::Registration;

pub(super) const UNNAMED: &str = "未命名设备";

impl Registration {
    pub(super) fn first_available_name(&self) -> String {
        std::iter::once(UNNAMED.to_owned())
            .chain((2..).map(|suffix| format!("{UNNAMED}-{suffix}")))
            .find(|candidate| !self.has_sender_name("", candidate))
            .expect("the finite list of Pairings cannot use every name")
    }

    fn has_sender_name(&self, except: &str, name: &str) -> bool {
        let key = match_key(name);
        self.active_pairings.iter().any(|(id, pairing)| {
            id != except && match_key(pairing.name.as_deref().unwrap_or(UNNAMED)) == key
        })
    }
}

fn match_key(name: &str) -> String {
    name.chars()
        .flat_map(|character| {
            let mut upper = character.to_uppercase();
            let first = upper.next().unwrap_or(character);
            let simple = if upper.next().is_none() {
                first
            } else {
                character
            };
            if simple == 'İ' { 'i' } else { simple }.to_lowercase()
        })
        .collect()
}

#[cfg(windows)]
mod windows_names {
    use std::{collections::BTreeMap, io};

    use super::{super::RegistrationStore, UNNAMED};

    #[derive(Debug, PartialEq, Eq)]
    pub(crate) enum SenderNameError {
        Empty,
        TooLong,
        InvalidCharacter,
        Duplicate,
        Storage,
    }

    impl From<io::Error> for SenderNameError {
        fn from(_: io::Error) -> Self {
            Self::Storage
        }
    }

    impl RegistrationStore {
        pub(crate) fn sender_names(&self) -> io::Result<BTreeMap<String, String>> {
            Ok(self
                .read()?
                .active_pairings
                .into_iter()
                .map(|(id, pairing)| (id, pairing.name.unwrap_or_else(|| UNNAMED.into())))
                .collect())
        }

        pub(crate) fn name_sender(&self, id: &str, input: &[u16]) -> Result<(), SenderNameError> {
            let name = String::from_utf16(input).map_err(|_| SenderNameError::InvalidCharacter)?;
            let name = name.trim_matches(|character: char| {
                character.is_whitespace() && character != '\u{0085}'
                    || ('\u{001c}'..='\u{001f}').contains(&character)
            });
            if name.is_empty() {
                return Err(SenderNameError::Empty);
            }
            if name.chars().count() > 40 {
                return Err(SenderNameError::TooLong);
            }
            self.update(|next| {
                if next.has_sender_name(id, name) {
                    return Err(SenderNameError::Duplicate);
                }
                let pairing = next
                    .active_pairings
                    .get_mut(id)
                    .ok_or(SenderNameError::Storage)?;
                pairing.name = Some(name.into());
                Ok(())
            })
        }
    }
}

#[cfg(windows)]
pub(crate) use windows_names::SenderNameError;
