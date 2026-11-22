// PairingRelayIn represents a PairingRelayIn model.
#[derive(Clone, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
pub struct PairingRelayIn {
    #[serde(rename="payload")]
    pub payload: String,
    #[serde(rename="sessionId")]
    pub session_id: String,
    #[serde(rename="type")]
    pub reserved_type: String,
    #[serde(rename="version")]
    pub version: i64,
}
