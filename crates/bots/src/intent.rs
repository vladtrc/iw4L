#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PathOutcome {
    None,
    Clear,
    Blocked,
    Unreachable,
    BudgetExhausted,
    ProgressLost,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MoveMode {
    Hold,
    Walk,
    Drop,
    Mantle,
    Ladder,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BotIntent {
    pub look_at: Option<[f32; 3]>,
    pub move_goal: Option<[f32; 3]>,
    pub desired_range: Option<f32>,
    pub move_mode: MoveMode,
    pub fire: bool,
    pub use_button: bool,
    pub reload: bool,
    pub crouch: bool,
    pub path: PathOutcome,
}

impl Default for BotIntent {
    fn default() -> Self {
        Self {
            look_at: None,
            move_goal: None,
            desired_range: None,
            move_mode: MoveMode::Hold,
            fire: false,
            use_button: false,
            reload: false,
            crouch: false,
            path: PathOutcome::None,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum MotorReport {
    #[default]
    Executing,
    Arrived,
    Blocked,
    Unsupported,
}
