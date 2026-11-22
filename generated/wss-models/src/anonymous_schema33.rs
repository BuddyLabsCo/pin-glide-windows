// AnonymousSchema33 represents a AnonymousSchema33 model.
#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
pub enum AnonymousSchema33 {
    #[serde(rename="awaiting_authorization")]
    AwaitingAuthorization,
    #[serde(rename="awaiting_confirmation")]
    AwaitingConfirmation,
    #[serde(rename="credential_ready")]
    CredentialReady,
    #[serde(rename="credential_issued")]
    CredentialIssued,
    #[serde(rename="completed")]
    Completed,
    #[serde(rename="canceled")]
    Canceled,
    #[serde(rename="expired")]
    Expired,
    #[serde(rename="superseded")]
    Superseded,
}
