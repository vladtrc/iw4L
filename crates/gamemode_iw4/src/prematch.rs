pub(crate) const PLAYER_WAIT_MS: u32 = 15_000;

pub const MATCH_START_MS: u32 = 5_000;

pub const DEFAULT_ALLIES_CHARSET: &str = "us_army";

pub const DEFAULT_AXIS_CHARSET: &str = "opforce_composite";

pub const FACTION_TABLE: &str = "mp/factionTable.csv";

pub const FACTION_ICON_COL: i32 = 5;

pub const FACTION_VOICE_PREFIX_COL: i32 = 7;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PrematchStep {
    Waiting { elapsed_ms: u32 },

    Starting { elapsed_ms: u32 },

    Done,
}

impl Default for PrematchStep {
    fn default() -> Self {
        Self::Waiting { elapsed_ms: 0 }
    }
}

impl PrematchStep {
    #[must_use]
    pub fn advance(self, dt_ms: u32, max_alive: u32) -> Self {
        match self {
            Self::Done => Self::Done,
            Self::Waiting { elapsed_ms } => {
                let elapsed = elapsed_ms.saturating_add(dt_ms);
                if max_alive >= 2 || elapsed >= PLAYER_WAIT_MS {
                    Self::Starting { elapsed_ms: 0 }
                } else {
                    Self::Waiting {
                        elapsed_ms: elapsed,
                    }
                }
            }
            Self::Starting { elapsed_ms } => {
                let elapsed = elapsed_ms.saturating_add(dt_ms);
                if elapsed >= MATCH_START_MS {
                    Self::Done
                } else {
                    Self::Starting {
                        elapsed_ms: elapsed,
                    }
                }
            }
        }
    }

    #[must_use]
    pub const fn dump_token(self) -> &'static str {
        match self {
            Self::Waiting { .. } => "waiting",
            Self::Starting { .. } => "starting",
            Self::Done => "done",
        }
    }
}
