pub const SCOREBAR_STATUS_CYCLE_MS: u32 = 30_000;

pub const SCOREBAR_GAMETYPE_WINDOW_MS: u32 = 4_000;

pub const SCOREBAR_LOC_WINNING: &str = "MPUI_WINNING_CAPS";

pub const SCOREBAR_LOC_LOSING: &str = "MPUI_LOSING_CAPS";

pub const SCOREBAR_LOC_TIED: &str = "MPUI_TIED_CAPS";

pub const SCOREBAR_LOC_GAMETYPE_DM: &str = "MPUI_DEATHMATCH";

pub const SCOREBAR_LOC_GAMETYPE_WAR: &str = "MPUI_WAR";

pub const SCOREBAR_LOC_GAMETYPE_DOM: &str = "MPUI_DOMINATION";

pub const SCOREBAR_BACKDROP_IMAGE: &str = "hud_scorebar";

pub const SCOREBAR_TOPBAR_BG_IMAGE: &str = "hud_scorebar_topbar_bg";
pub const SCOREBAR_TOPBAR_IMAGE: &str = "hud_scorebar_topbar";
pub const SCOREBAR_TOPCAP_BG_IMAGE: &str = "hud_scorebar_topcap_bg";
pub const SCOREBAR_TOPCAP_IMAGE: &str = "hud_scorebar_topcap";
pub const SCOREBAR_BOTTOMBAR_BG_IMAGE: &str = "hud_scorebar_bottombar_bg";
pub const SCOREBAR_BOTTOMBAR_IMAGE: &str = "hud_scorebar_bottombar";
pub const SCOREBAR_BOTTOMCAP_BG_IMAGE: &str = "hud_scorebar_bottomcap_bg";
pub const SCOREBAR_BOTTOMCAP_IMAGE: &str = "hud_scorebar_bottomcap";

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ScorebarRect {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
}

pub const SCOREBAR_BACKDROP: ScorebarRect = ScorebarRect {
    x: 0.0,
    y: -62.6667,
    w: 341.3333,
    h: 42.6667,
};

pub const SCOREBAR_TOP_BG: ScorebarRect = ScorebarRect {
    x: 88.0,
    y: -44.0,
    w: 128.0,
    h: 12.0,
};

pub const SCOREBAR_TOP_FILL: ScorebarRect = ScorebarRect {
    x: 88.0,
    y: -44.0,
    w: 114.6667,
    h: 12.0,
};

pub const SCOREBAR_TOP_CAP: ScorebarRect = ScorebarRect {
    x: 202.6667,
    y: -44.0,
    w: 10.6667,
    h: 12.0,
};

pub const SCOREBAR_BOTTOM_BG: ScorebarRect = ScorebarRect {
    x: 88.0,
    y: -30.6667,
    w: 128.0,
    h: 10.6667,
};

pub const SCOREBAR_BOTTOM_FILL: ScorebarRect = ScorebarRect {
    x: 88.0,
    y: -30.6667,
    w: 114.6667,
    h: 10.6667,
};

pub const SCOREBAR_BOTTOM_CAP: ScorebarRect = ScorebarRect {
    x: 202.6667,
    y: -30.6667,
    w: 10.6667,
    h: 10.6667,
};

pub const SCOREBAR_STATUS: ScorebarRect = ScorebarRect {
    x: 65.3333,
    y: -44.6667,
    w: 0.6667,
    h: 0.6667,
};

pub const SCOREBAR_LOCAL_SCORE: ScorebarRect = ScorebarRect {
    x: 60.0,
    y: -46.6667,
    w: 16.0,
    h: 16.0,
};

pub const SCOREBAR_LEAD_SCORE: ScorebarRect = ScorebarRect {
    x: 72.0,
    y: -32.6667,
    w: 0.6667,
    h: 0.6667,
};

pub const SCOREBAR_CLOCK: ScorebarRect = ScorebarRect {
    x: 5.3333,
    y: -58.6667,
    w: 54.6667,
    h: 54.6667,
};

pub const SCOREBAR_STATUS_TEXTSCALE: f32 = 0.3333;

pub const SCOREBAR_SCORE_TEXTSCALE: f32 = 0.5500;

pub const SCOREBAR_COLOR_GAMETYPE: [f32; 4] = [1.0, 0.8, 0.4, 0.85];

pub const SCOREBAR_COLOR_WINNING: [f32; 4] = [0.4, 1.0, 0.4, 1.0];

pub const SCOREBAR_COLOR_LOSING: [f32; 4] = [1.0, 0.4, 0.4, 1.0];

pub const SCOREBAR_COLOR_TIED: [f32; 4] = [1.0, 1.0, 0.5, 1.0];

pub const SCOREBAR_COLOR_TOP_FILL: [f32; 4] = [0.630, 0.860, 0.600, 1.0];

pub const SCOREBAR_COLOR_BOTTOM_FILL: [f32; 4] = [0.780, 0.278, 0.239, 1.0];

pub const SCOREBAR_COLOR_TRACK: [f32; 4] = [0.750, 0.750, 0.750, 1.0];

pub const SCOREBAR_COLOR_BACKDROP: [f32; 4] = [1.0, 1.0, 1.0, 0.650];

pub const SCOREBAR_COLOR_CLOCK_OK: [f32; 4] = [1.0, 1.0, 1.0, 1.0];

pub const SCOREBAR_COLOR_CLOCK_WARN: [f32; 4] = [0.850, 0.500, 0.000, 1.0];

pub const SCOREBAR_COLOR_CLOCK_CRIT: [f32; 4] = [0.850, 0.400, 0.400, 1.0];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ScorebarStatus {
    Gametype,
    Winning,
    Losing,
    Tied,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ScorebarCycleSlot {
    Blank,
    Gametype,
    Standing,
}

#[must_use]
pub fn scorebar_cycle_slot(sys_milliseconds: u32) -> ScorebarCycleSlot {
    let t = sys_milliseconds % SCOREBAR_STATUS_CYCLE_MS;
    if t > 0 && t < SCOREBAR_GAMETYPE_WINDOW_MS {
        ScorebarCycleSlot::Gametype
    } else if t >= SCOREBAR_GAMETYPE_WINDOW_MS {
        ScorebarCycleSlot::Standing
    } else {
        ScorebarCycleSlot::Blank
    }
}

#[must_use]
pub fn scorebar_ffa_standing(local_score: i32, lead_score: i32) -> ScorebarStatus {
    if local_score > lead_score {
        ScorebarStatus::Winning
    } else if local_score < lead_score {
        ScorebarStatus::Losing
    } else {
        ScorebarStatus::Tied
    }
}

#[must_use]
pub fn scorebar_ffa_status(
    sys_milliseconds: u32,
    local_score: i32,
    lead_score: i32,
) -> Option<ScorebarStatus> {
    match scorebar_cycle_slot(sys_milliseconds) {
        ScorebarCycleSlot::Blank => None,
        ScorebarCycleSlot::Gametype => Some(ScorebarStatus::Gametype),
        ScorebarCycleSlot::Standing => Some(scorebar_ffa_standing(local_score, lead_score)),
    }
}

pub fn scorebar_status_from_vis(
    rows: &[(ScorebarStatus, &str)],
    host: &impl crate::expr::ExprHost,
) -> Result<Option<ScorebarStatus>, crate::expr::ExprError> {
    let mut found = None;
    for (status, dump) in rows {
        if crate::expr::is_expression_true(dump, host)? {
            found = Some(*status);
        }
    }
    Ok(found)
}

#[must_use]
pub fn scorebar_status_loc_key(
    status: ScorebarStatus,
    gametype_token: &str,
) -> Option<&'static str> {
    match status {
        ScorebarStatus::Winning => Some(SCOREBAR_LOC_WINNING),
        ScorebarStatus::Losing => Some(SCOREBAR_LOC_LOSING),
        ScorebarStatus::Tied => Some(SCOREBAR_LOC_TIED),
        ScorebarStatus::Gametype => scorebar_gametype_loc_key(gametype_token),
    }
}

#[must_use]
pub fn scorebar_gametype_loc_key(token: &str) -> Option<&'static str> {
    match token.as_bytes() {
        b"dm" => Some(SCOREBAR_LOC_GAMETYPE_DM),
        b"war" => Some(SCOREBAR_LOC_GAMETYPE_WAR),
        b"dom" => Some(SCOREBAR_LOC_GAMETYPE_DOM),
        _ => None,
    }
}

#[must_use]
pub fn scorebar_status_forecolor(status: ScorebarStatus) -> [f32; 4] {
    match status {
        ScorebarStatus::Gametype => SCOREBAR_COLOR_GAMETYPE,
        ScorebarStatus::Winning => SCOREBAR_COLOR_WINNING,
        ScorebarStatus::Losing => SCOREBAR_COLOR_LOSING,
        ScorebarStatus::Tied => SCOREBAR_COLOR_TIED,
    }
}

#[must_use]
pub fn scorebar_clock_forecolor(remaining_s: i32) -> Option<[f32; 4]> {
    if remaining_s >= 60 {
        Some(SCOREBAR_COLOR_CLOCK_OK)
    } else if remaining_s >= 30 {
        Some(SCOREBAR_COLOR_CLOCK_WARN)
    } else if remaining_s > 0 {
        Some(SCOREBAR_COLOR_CLOCK_CRIT)
    } else {
        None
    }
}

#[must_use]
pub fn scorebar_track_frac(score: i32, limit: i32) -> f32 {
    if limit <= 0 {
        0.0
    } else {
        (score as f32 / limit as f32).clamp(0.0, 1.0)
    }
}

#[must_use]
pub fn scorebar_fill_cap_x(fill: ScorebarRect, frac: f32) -> f32 {
    fill.x + fill.w * frac
}

#[must_use]
pub fn match_time_remaining_ms(time_limit_ms: u32, match_elapsed_ms: u32) -> i32 {
    time_limit_ms as i32 - match_elapsed_ms as i32
}

#[must_use]
pub fn mm_ss_nonneg(remaining_ms: i32) -> (u32, u32) {
    let secs = remaining_ms.max(0) as u32 / 1000;
    (secs / 60, secs % 60)
}

#[must_use]
pub fn ffa_scorebar_lead(other_scores: &[i32]) -> i32 {
    let mut lead = 0;
    for score in other_scores {
        if *score > lead {
            lead = *score;
        }
    }
    lead
}
