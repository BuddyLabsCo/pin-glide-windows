// BusinessError represents a BusinessError model.
#[derive(Clone, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
pub struct BusinessError {
    #[serde(rename="code")]
    pub code: String,
    #[serde(rename="detail")]
    pub detail: String,
    #[serde(rename="errors", skip_serializing_if = "Option::is_none")]
    pub errors: Option<Vec<crate::ErrorItem>>,
}
