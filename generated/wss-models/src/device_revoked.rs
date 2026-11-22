// DeviceRevoked represents a DeviceRevoked model.
#[derive(Clone, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
pub struct DeviceRevoked {
    #[serde(rename="receiverId")]
    pub receiver_id: String,
    #[serde(rename="type")]
    pub reserved_type: String,
    #[serde(rename="version")]
    pub version: i64,
}
