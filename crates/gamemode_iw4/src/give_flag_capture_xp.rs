use crate::lead_swing::OBJECTIVE_POINTS_MOD_DEFAULT;
use crate::use_prox::{GameObjectTeam, ProxClaimTeam};

pub const RANK_INIT_CAPTURE_POINTS: i32 = 300;

pub const SCORE_INFO_KILL: i32 = 50;

pub const SCORE_INFO_ASSIST: i32 = 10;

pub const SCORE_CAPTURE_POINTS: i32 = 150;

pub const SPLASH_CAPTURE_KEY: &str = "capture";

pub const CALLOUT_SECURED_POSITION: &str = "callout_securedposition";

pub fn claim_team_from_owner(owner: GameObjectTeam) -> Option<ProxClaimTeam> {
    match owner {
        GameObjectTeam::Axis => Some(ProxClaimTeam::Axis),
        GameObjectTeam::Allies => Some(ProxClaimTeam::Allies),
        GameObjectTeam::Neutral | GameObjectTeam::None => None,
    }
}

pub fn capture_player_score_delta(objective_points_mod: i32) -> i32 {
    SCORE_CAPTURE_POINTS.saturating_mul(objective_points_mod)
}

pub fn capture_player_score_points() -> i32 {
    capture_player_score_delta(OBJECTIVE_POINTS_MOD_DEFAULT)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TouchCredit {
    pub client: u32,
    pub team: ProxClaimTeam,
    pub start_time_ms: i32,
    pub alive: bool,
}

pub fn is_capture_touch(row: TouchCredit, team: ProxClaimTeam) -> bool {
    row.team == team
}

pub fn earliest_claim_player(
    claim_player: Option<u32>,
    claim_alive: bool,
    touch: &[TouchCredit],
) -> Option<u32> {
    let mut earliest = if claim_alive { claim_player } else { None };
    if touch.is_empty() {
        return earliest;
    }
    let mut earliest_time: Option<i32> = None;
    for row in touch {
        if !row.alive {
            continue;
        }
        if earliest_time.is_none_or(|t| row.start_time_ms < t) {
            earliest = Some(row.client);
            earliest_time = Some(row.start_time_ms);
        }
    }
    earliest
}

pub fn cap_xp_scale(cpm: f32) -> f32 {
    if cpm < 4.0 { 1.0 } else { 0.25 }
}

pub fn minutes_passed(time_passed_ms: i32) -> i32 {
    time_passed_ms.saturating_div(1000).saturating_div(60)
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct CapturePace {
    pub num_caps: i32,
    pub cpm: i32,
}

pub fn update_cpm(pace: CapturePace, time_passed_ms: i32) -> CapturePace {
    let num_caps = pace.num_caps.saturating_add(1);
    let minutes = minutes_passed(time_passed_ms);
    let cpm = if minutes < 1 {
        pace.cpm
    } else {
        num_caps / minutes
    };
    CapturePace { num_caps, cpm }
}

pub fn capture_splash_optional() -> i32 {
    SCORE_CAPTURE_POINTS
}

pub fn capture_rank_xp_amount(cpm: f32) -> i32 {
    (SCORE_CAPTURE_POINTS as f32 * cap_xp_scale(cpm)) as i32
}

pub fn teambased_rank_xp_allowed(allies: u32, axis: u32) -> bool {
    allies > 0 && axis > 0
}
