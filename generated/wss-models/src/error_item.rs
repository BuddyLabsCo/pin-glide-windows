// ErrorItem represents a ErrorItem model.
#[derive(Clone, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
pub struct ErrorItem {
    #[serde(rename="location", skip_serializing_if = "Option::is_none")]
    pub location: Option<String>,
    #[serde(rename="message")]
    pub message: String,
}
