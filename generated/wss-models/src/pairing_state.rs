// PairingState represents a PairingState model.
#[derive(Clone, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
pub struct PairingState {
    #[serde(rename="session")]
    pub session: Box<crate::PairingSessionView>,
    #[serde(rename="type")]
    pub reserved_type: String,
    #[serde(rename="version")]
    pub version: i64,
}
