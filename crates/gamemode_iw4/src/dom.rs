use crate::anchors;
use crate::phase::Team;
use crate::visuals::{DomFlagVisualChannel, team_color_dvar};

pub const GAMETYPE_TOKEN: &str = "dom";

pub const DISPLAY_NAME: &str = "DOMINATION";

pub const SCORE_LIMIT: i32 = 300;

pub const TIME_LIMIT_MS: u32 = 1_800_000;

pub const ROUND_LIMIT: i32 = 1;

pub const WIN_LIMIT: i32 = 1;

pub const NUM_LIVES: i32 = 0;

pub const HALF_TIME: i32 = 0;

pub const TEAM_BASED: bool = true;

pub const GAMETYPE_DIALOG_LINE: &str = "domination";

pub const OBJECTIVE_DIALOG_LINE: &str = "capture_objs";

pub const BACKGROUND_MAPNAME: &str = "mp_background";

pub const CAPTURE_MS: u32 = 10_000;

pub const SCORE_INTERVAL_MS: u32 = 5_000;

pub const POINTS_PER_FLAG: i32 = 1;

pub const SPAWN_CLASSNAME: &str = "mp_dom_spawn";

pub const START_SPAWN_ALLIES: &str = "mp_dom_spawn_allies_start";

pub const START_SPAWN_AXIS: &str = "mp_dom_spawn_axis_start";

pub const FLAG_PRIMARY: &str = anchors::FLAG_PRIMARY;

pub const FLAG_MODEL_NEUTRAL: &str = "prop_flag_neutral";

pub const FLAG_MODEL_ALLIES: &str = "prop_flag_allies";

pub const FLAG_MODEL_AXIS: &str = "prop_flag_axis";

pub const SCORE_MATCH_END_MS: u32 = 12_000;

pub fn flag_model_for_owner(team: Team) -> &'static str {
    match team {
        Team::Free => FLAG_MODEL_NEUTRAL,
        Team::Allies => FLAG_MODEL_ALLIES,
        Team::Axis => FLAG_MODEL_AXIS,
    }
}

pub const FLAG_OWNER_VISUAL_CHANNELS: &[DomFlagVisualChannel] = &[
    DomFlagVisualChannel::WorldModel,
    DomFlagVisualChannel::ObjectiveIcon,
    DomFlagVisualChannel::TeamColorDvar,
    DomFlagVisualChannel::ScriptPlayFx,
];

pub fn flag_owner_team_color_dvar(team: Team) -> &'static str {
    team_color_dvar(team)
}

pub const FLAG_AMBIENT_FX_PATH: Option<&str> = None;

pub fn capture_progress(held_ms: u32) -> f32 {
    if CAPTURE_MS == 0 {
        return 1.0;
    }
    (held_ms as f32 / CAPTURE_MS as f32).clamp(0.0, 1.0)
}

pub fn score_ticks_from_accum(owned_accum_ms: u32) -> (u32, u32) {
    if SCORE_INTERVAL_MS == 0 {
        return (0, owned_accum_ms);
    }
    let ticks = owned_accum_ms / SCORE_INTERVAL_MS;
    let rem = owned_accum_ms % SCORE_INTERVAL_MS;
    (ticks, rem)
}

pub fn points_for_ticks(flag_count: u32, ticks: u32) -> i32 {
    (flag_count as i32) * (ticks as i32) * POINTS_PER_FLAG
}
