// Receipt represents a Receipt model.
#[derive(Clone, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
pub struct Receipt {
    #[serde(rename="messageId")]
    pub message_id: String,
    #[serde(rename="senderId")]
    pub sender_id: String,
    #[serde(rename="type")]
    pub reserved_type: String,
    #[serde(rename="version")]
    pub version: i64,
}
