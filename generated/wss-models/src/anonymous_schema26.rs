// AnonymousSchema26 represents a AnonymousSchema26 model.
#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
pub enum AnonymousSchema26 {
    #[serde(rename="new_receiver")]
    NewReceiver,
    #[serde(rename="add_sender")]
    AddSender,
    #[serde(rename="recover_receiver")]
    RecoverReceiver,
}
