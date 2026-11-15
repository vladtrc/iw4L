#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ScriptCinematic {
    pub name: String,
    pub looped: bool,
    pub fullscreen: bool,
    pub start_ms: i32,
    pub end_ms: Option<i32>,
}

impl ScriptCinematic {
    pub fn playing(&self, now_ms: i32) -> bool {
        self.end_ms.is_none_or(|end| now_ms < end)
    }
}
