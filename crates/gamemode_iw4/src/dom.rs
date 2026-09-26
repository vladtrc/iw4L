use crate::anchors;

pub const GAMETYPE_TOKEN: &str = "dom";

pub const DISPLAY_NAME: &str = "DOMINATION";

pub const SCORE_LIMIT: i32 = 300;

pub const TIME_LIMIT_MS: u32 = 1_800_000;

pub const ROUND_LIMIT: i32 = 1;

pub const TEAM_BASED: bool = true;

pub const GAMETYPE_DIALOG_LINE: &str = "domination";

pub const OBJECTIVE_DIALOG_LINE: &str = "capture_objs";

pub const CAPTURE_MS: u32 = 10_000;

pub const SPAWN_CLASSNAME: &str = "mp_dom_spawn";

pub(crate) const START_SPAWN_ALLIES: &str = "mp_dom_spawn_allies_start";

pub(crate) const START_SPAWN_AXIS: &str = "mp_dom_spawn_axis_start";

pub const FLAG_PRIMARY: &str = anchors::FLAG_PRIMARY;

pub const FLAG_MODEL_NEUTRAL: &str = "prop_flag_neutral";
