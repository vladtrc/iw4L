use crate::{dd, dom, ffa, kind::GameModeKind, prematch};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MatchLimits {
    pub score_limit: Option<i32>,

    pub win_limit: Option<u32>,

    pub round_limit: Option<u32>,

    pub time_limit_ms: Option<u32>,
}

impl MatchLimits {
    pub fn for_kind(kind: GameModeKind) -> Self {
        match kind {
            GameModeKind::FreeForAll => Self {
                score_limit: Some(ffa::SCORE_LIMIT),
                win_limit: None,
                round_limit: None,
                time_limit_ms: Some(ffa::TIME_LIMIT_MS),
            },
            GameModeKind::Domination => Self {
                score_limit: Some(dom::SCORE_LIMIT),
                win_limit: None,
                round_limit: None,
                time_limit_ms: Some(dom::TIME_LIMIT_MS),
            },
            GameModeKind::Demolition => Self {
                score_limit: None,
                win_limit: Some(dd::WIN_LIMIT),
                round_limit: Some(dd::ROUND_LIMIT),
                time_limit_ms: Some(dd::TIME_LIMIT_MS),
            },
        }
    }

    pub fn player_wait_ms(self) -> u32 {
        prematch::PLAYER_WAIT_MS
    }

    pub fn match_start_ms(self) -> u32 {
        prematch::MATCH_START_MS
    }
}

pub fn blurb(kind: GameModeKind) -> &'static str {
    match kind {
        GameModeKind::FreeForAll => "First to 1000 — or 10 minutes",
        GameModeKind::Domination => "Hold flags — 300 wins",
        GameModeKind::Demolition => "Plant both sites — first to 2",
    }
}

pub fn hud_round_limit(kind: GameModeKind) -> Option<u32> {
    MatchLimits::for_kind(kind).win_limit
}
