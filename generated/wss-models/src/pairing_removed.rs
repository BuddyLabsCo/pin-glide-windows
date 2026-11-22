// PairingRemoved represents a PairingRemoved model.
#[derive(Clone, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
pub struct PairingRemoved {
    #[serde(rename="pairingId")]
    pub pairing_id: String,
    #[serde(rename="senderId")]
    pub sender_id: String,
    #[serde(rename="type")]
    pub reserved_type: String,
    #[serde(rename="version")]
    pub version: i64,
}
