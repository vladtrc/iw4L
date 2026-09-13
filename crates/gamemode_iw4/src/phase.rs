use crate::prematch;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum MatchPhaseKind {
    #[default]
    Playing = 0,

    Prematch = 1,

    RoundEnding = 2,

    SwitchingSides = 3,

    MatchEnding = 4,

    WaitingForPlayers = 5,
}

impl MatchPhaseKind {
    pub fn default_duration_ms(self, dem_style_end: bool) -> u32 {
        match self {
            Self::Playing => 0,
            Self::WaitingForPlayers => prematch::PLAYER_WAIT_MS,
            Self::Prematch => prematch::MATCH_START_MS,
            Self::RoundEnding => crate::dd::ROUND_END_MS,
            Self::SwitchingSides => crate::dd::SWITCH_SIDES_MS,
            Self::MatchEnding if dem_style_end => crate::dd::MATCH_END_MS,
            Self::MatchEnding => crate::dom::SCORE_MATCH_END_MS,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum RoundEndReason {
    #[default]
    None = 0,

    TargetDestroyed = 1,

    TimeLimit = 2,

    RoundLimit = 3,

    ScoreLimit = 4,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum Team {
    #[default]
    Free = 0,
    Axis = 1,
    Allies = 2,
}

impl Team {
    pub fn from_retail_u8(raw: u8) -> Option<Self> {
        match raw {
            0 => Some(Self::Free),
            1 => Some(Self::Axis),
            2 => Some(Self::Allies),
            _ => None,
        }
    }

    pub fn opposing(self) -> Self {
        match self {
            Self::Allies => Self::Axis,
            Self::Axis => Self::Allies,
            Self::Free => Self::Free,
        }
    }
}
