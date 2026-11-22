// CredentialReady represents a CredentialReady model.
#[derive(Clone, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
pub struct CredentialReady {
    #[serde(rename="expiresAt")]
    pub expires_at: String,
    #[serde(rename="receiverId")]
    pub receiver_id: String,
    #[serde(rename="sessionId")]
    pub session_id: String,
    #[serde(rename="type")]
    pub reserved_type: String,
    #[serde(rename="version")]
    pub version: i64,
}
