// PairingSessionView represents a PairingSessionView model.
#[derive(Clone, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
pub struct PairingSessionView {
    #[serde(rename="expiresAt")]
    pub expires_at: String,
    #[serde(rename="mode")]
    pub mode: Box<crate::AnonymousSchema26>,
    #[serde(rename="pairingId", skip_serializing_if = "Option::is_none")]
    pub pairing_id: Option<String>,
    #[serde(rename="receiverConfirmed")]
    pub receiver_confirmed: bool,
    #[serde(rename="receiverId")]
    pub receiver_id: String,
    #[serde(rename="senderConfirmed")]
    pub sender_confirmed: bool,
    #[serde(rename="senderId", skip_serializing_if = "Option::is_none")]
    pub sender_id: Option<String>,
    #[serde(rename="sessionId")]
    pub session_id: String,
    #[serde(rename="state")]
    pub state: Box<crate::AnonymousSchema33>,
}
