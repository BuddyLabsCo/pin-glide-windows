// Delivery represents a Delivery model.
#[derive(Clone, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
pub struct Delivery {
    #[serde(rename="ciphertext")]
    pub ciphertext: String,
    #[serde(rename="cryptoVersion")]
    pub crypto_version: i64,
    #[serde(rename="messageId")]
    pub message_id: String,
    #[serde(rename="nonce")]
    pub nonce: String,
    #[serde(rename="pairingId")]
    pub pairing_id: String,
    #[serde(rename="receiverId")]
    pub receiver_id: String,
    #[serde(rename="senderId")]
    pub sender_id: String,
    #[serde(rename="type")]
    pub reserved_type: String,
    #[serde(rename="version")]
    pub version: i64,
}
