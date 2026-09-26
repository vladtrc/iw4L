use gamemode_iw4::Team;

#[derive(Clone, Debug, Default, PartialEq)]
pub struct ObjectiveMatch {
    pub scores: [i32; 3],
    pub compass: Vec<CompassObjective>,
    pub server_info: Vec<(String, String)>,
    pub game_end_time: i32,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[repr(u8)]
pub enum ObjectiveState {
    #[default]
    Empty = 0,
    Active = 1,
    Invisible = 2,
    Done = 3,
    Current = 4,
    Failed = 5,
}

impl ObjectiveState {
    pub fn from_script(name: &str) -> Option<Self> {
        Some(match name {
            "empty" => Self::Empty,
            "active" => Self::Active,
            "invisible" => Self::Invisible,
            "done" => Self::Done,
            "current" => Self::Current,
            "failed" => Self::Failed,
            _ => return None,
        })
    }

    pub fn from_u8(raw: u8) -> Option<Self> {
        Some(match raw {
            0 => Self::Empty,
            1 => Self::Active,
            2 => Self::Invisible,
            3 => Self::Done,
            4 => Self::Current,
            5 => Self::Failed,
            _ => return None,
        })
    }

    pub fn drawn(self) -> bool {
        matches!(self, Self::Active | Self::Current)
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct CompassObjective {
    pub index: u8,
    pub state: ObjectiveState,
    pub origin: [f32; 3],
    pub team: Team,
    pub icon: String,
}

impl ObjectiveMatch {
    pub fn server_info(&self, name: &str) -> Option<&str> {
        self.server_info
            .iter()
            .find(|(key, _)| key.eq_ignore_ascii_case(name))
            .map(|(_, value)| value.as_str())
    }

    pub fn time_left_ms(&self, now_ms: i32) -> i32 {
        self.game_end_time.saturating_sub(now_ms)
    }

    pub fn server_info_int(&self, name: &str) -> Option<i32> {
        let value = self.server_info(name)?.trim();
        value
            .parse::<i32>()
            .ok()
            .or_else(|| value.parse::<f32>().ok().map(|v| v as i32))
    }
}

impl CompassObjective {
    pub fn shows_to(&self, team: Team) -> bool {
        self.state.drawn() && (self.team == Team::Free || self.team == team)
    }
}
