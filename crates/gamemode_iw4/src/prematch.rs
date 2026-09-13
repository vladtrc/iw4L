pub const PLAYER_WAIT_DVAR: &str = "scr_game_playerwaittime";

pub const PLAYER_WAIT_MS: u32 = 15_000;

pub const MATCH_START_DVAR: &str = "scr_game_matchstarttime";

pub const MATCH_START_MS: u32 = 5_000;

pub const HUD_WAITING_FOR_PLAYERS: &str = "MP_WAITING_FOR_PLAYERS";

pub const HUD_WAITING_FOR_MORE_PLAYERS: &str = "MP_WAITING_FOR_MORE_PLAYERS";

pub const HUD_MATCH_STARTING_IN: &str = "MP_MATCH_STARTING_IN";

pub const MATCH_START_COMBINED_MS: u32 = PLAYER_WAIT_MS + MATCH_START_MS;

pub const MATCH_START_TEXT_FONT_SCALE: f32 = 1.5;

pub const MATCH_START_TEXT_FONT: i32 = 3;

pub const MATCH_START_TEXT_Y: f32 = -40.0;

pub const MATCH_START_VALUE_FONT_SCALE: f32 = 1.0;

pub const MATCH_START_VALUE_FONT: i32 = 6;

pub const MATCH_START_VALUE_Y: f32 = 0.0;

pub const MATCH_START_VALUE_RGB: [f32; 3] = [1.0, 1.0, 0.0];

pub const MATCH_START_SORT: f32 = 1001.0;

pub const MATCH_START_VALUE_MAX_FONT_SCALE: f32 = 2.0;

pub const MATCH_START_PULSE_IN_MS: i32 = 100;

pub const MATCH_START_PULSE_OUT_MS: i32 = 200;

pub const HUD_VICTORY: &str = "MP_VICTORY";

pub const HUD_DEFEAT: &str = "MP_DEFEAT";

pub const HUD_MATCH_TIE: &str = "MP_MATCH_TIE";

pub const HUD_TIME_LIMIT_REACHED: &str = "MP_TIME_LIMIT_REACHED";

pub const HUD_SCORE_LIMIT_REACHED: &str = "MP_SCORE_LIMIT_REACHED";

pub const OUTCOME_TITLE_Y: f32 = 20.0;

pub const OUTCOME_TITLE_FONT_SCALE: f32 = 3.0;

pub const OUTCOME_REASON_Y: f32 = 80.0;

pub const OUTCOME_FIRST_Y: f32 = 140.0;

pub const OUTCOME_FIRST_FONT_SCALE: f32 = 2.0;

pub const OUTCOME_SECOND_Y: f32 = 200.0;

pub const OUTCOME_OTHER_FONT_SCALE: f32 = 1.5;

pub const OUTCOME_THIRD_Y: f32 = 248.0;

pub const HUD_FIRSTPLACE_NAME: &str = "MP_FIRSTPLACE_NAME";
pub const HUD_SECONDPLACE_NAME: &str = "MP_SECONDPLACE_NAME";
pub const HUD_THIRDPLACE_NAME: &str = "MP_THIRDPLACE_NAME";

pub const DEFAULT_ALLIES_CHARSET: &str = "us_army";

pub const DEFAULT_AXIS_CHARSET: &str = "opforce_composite";

pub const FACTION_TABLE: &str = "mp/factionTable.csv";

pub const FACTION_ICON_COL: i32 = 5;

pub const FACTION_VOICE_PREFIX_COL: i32 = 7;

pub const LABEL_WAITING_FOR_TEAMS: i32 = 1;
pub const LABEL_MATCH_STARTING_IN: i32 = 2;
pub const LABEL_VICTORY: i32 = 3;
pub const LABEL_DEFEAT: i32 = 4;
pub const LABEL_MATCH_TIE: i32 = 5;
pub const LABEL_TIME_LIMIT_REACHED: i32 = 6;
pub const LABEL_SCORE_LIMIT_REACHED: i32 = 7;
pub const LABEL_FIRSTPLACE_NAME: i32 = 8;
pub const LABEL_SECONDPLACE_NAME: i32 = 9;
pub const LABEL_THIRDPLACE_NAME: i32 = 10;

pub const LABEL_OBJECTIVE_HINT: i32 = 11;

pub const HUD_OBJECTIVE_HINT: &str = "OBJECTIVES_DM_HINT";

pub const HINT_TEXT_Y: f32 = 50.0;

pub const HINT_FONT_SCALE: f32 = 1.75;

pub const HINT_GLOW_RGB: [f32; 3] = [0.3, 0.6, 0.3];

pub const HINT_DURATION_MS: i32 = 4_000;

#[must_use]
pub const fn loc_key_from_label(label: i32) -> Option<&'static str> {
    match label {
        LABEL_WAITING_FOR_TEAMS => Some(HUD_WAITING_FOR_MORE_PLAYERS),
        LABEL_MATCH_STARTING_IN => Some(HUD_MATCH_STARTING_IN),
        LABEL_VICTORY => Some(HUD_VICTORY),
        LABEL_DEFEAT => Some(HUD_DEFEAT),
        LABEL_MATCH_TIE => Some(HUD_MATCH_TIE),
        LABEL_TIME_LIMIT_REACHED => Some(HUD_TIME_LIMIT_REACHED),
        LABEL_SCORE_LIMIT_REACHED => Some(HUD_SCORE_LIMIT_REACHED),
        LABEL_FIRSTPLACE_NAME => Some(HUD_FIRSTPLACE_NAME),
        LABEL_SECONDPLACE_NAME => Some(HUD_SECONDPLACE_NAME),
        LABEL_THIRDPLACE_NAME => Some(HUD_THIRDPLACE_NAME),
        LABEL_OBJECTIVE_HINT => Some(HUD_OBJECTIVE_HINT),
        _ => None,
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MatchStartKind {
    WaitingForTeams,

    MatchStartingIn,
}

impl MatchStartKind {
    #[must_use]
    pub const fn loc_key(self) -> &'static str {
        match self {
            Self::WaitingForTeams => HUD_WAITING_FOR_MORE_PLAYERS,
            Self::MatchStartingIn => HUD_MATCH_STARTING_IN,
        }
    }

    #[must_use]
    pub const fn label(self) -> i32 {
        match self {
            Self::WaitingForTeams => LABEL_WAITING_FOR_TEAMS,
            Self::MatchStartingIn => LABEL_MATCH_STARTING_IN,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MatchStartDisplay {
    pub kind: MatchStartKind,

    pub count: i32,
}

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
    pub fn display(self) -> Option<MatchStartDisplay> {
        match self {
            Self::Done => None,
            Self::Waiting { elapsed_ms } => {
                let remaining = MATCH_START_COMBINED_MS.saturating_sub(elapsed_ms);
                Some(MatchStartDisplay {
                    kind: MatchStartKind::WaitingForTeams,
                    count: countdown_value(remaining),
                })
            }
            Self::Starting { elapsed_ms } => {
                let remaining = MATCH_START_MS.saturating_sub(elapsed_ms);
                Some(MatchStartDisplay {
                    kind: MatchStartKind::MatchStartingIn,
                    count: countdown_value(remaining),
                })
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

#[must_use]
pub fn countdown_value(remaining_ms: u32) -> i32 {
    if remaining_ms == 0 {
        return 1;
    }
    ((remaining_ms + 999) / 1000) as i32
}

pub fn player_wait_remaining_ms(elapsed_ms: u32) -> u32 {
    PLAYER_WAIT_MS.saturating_sub(elapsed_ms)
}

pub fn match_start_remaining_ms(elapsed_ms: u32) -> u32 {
    MATCH_START_MS.saturating_sub(elapsed_ms)
}

#[must_use]
pub fn match_start_value_font_scale(age_in_second_ms: i32) -> f32 {
    let age = if age_in_second_ms < 0 {
        0
    } else {
        age_in_second_ms
    };
    if age < MATCH_START_PULSE_IN_MS {
        let t = age as f32 / MATCH_START_PULSE_IN_MS as f32;
        return MATCH_START_VALUE_FONT_SCALE
            + t * (MATCH_START_VALUE_MAX_FONT_SCALE - MATCH_START_VALUE_FONT_SCALE);
    }
    let shrink = age - MATCH_START_PULSE_IN_MS;
    if shrink < MATCH_START_PULSE_OUT_MS {
        let t = shrink as f32 / MATCH_START_PULSE_OUT_MS as f32;
        return MATCH_START_VALUE_MAX_FONT_SCALE
            + t * (MATCH_START_VALUE_FONT_SCALE - MATCH_START_VALUE_MAX_FONT_SCALE);
    }
    MATCH_START_VALUE_FONT_SCALE
}
