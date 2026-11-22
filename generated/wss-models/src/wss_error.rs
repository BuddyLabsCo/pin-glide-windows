// WssError represents a WssError model.
#[derive(Clone, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
pub struct WssError {
    #[serde(rename="error")]
    pub error: Box<crate::BusinessError>,
    #[serde(rename="requestId")]
    pub request_id: String,
    #[serde(rename="sessionId", skip_serializing_if = "Option::is_none")]
    pub session_id: Option<String>,
    #[serde(rename="type")]
    pub reserved_type: String,
    #[serde(rename="version")]
    pub version: i64,
}
