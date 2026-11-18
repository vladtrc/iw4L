#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ScriptCinematic {
    pub request_id: u64,
    pub completion_client: crate::ClientId,
    pub name: String,
    pub looped: bool,
    pub fullscreen: bool,
}
