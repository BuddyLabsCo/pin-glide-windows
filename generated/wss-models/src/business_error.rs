// BusinessError represents a BusinessError model.
#[derive(Clone, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
pub struct BusinessError {
    #[serde(rename="code")]
    pub code: String,
    #[serde(rename="detail")]
    pub detail: String,
    #[serde(rename="errors", default, with = "::serde_with::rust::double_option", skip_serializing_if = "Option::is_none")]
    pub errors: Option<Option<Vec<crate::ErrorItem>>>,
}
