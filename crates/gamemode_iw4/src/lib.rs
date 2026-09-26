#![no_std]
#![forbid(unsafe_code)]

pub mod anchors;
pub mod animated_models;
pub mod damage_feedback;
pub mod dd;
pub mod dom;
pub mod end_game;
pub mod ffa;
pub mod gamelogic;
pub mod gaps;
pub mod globallogic;
pub mod kind;
pub mod load;
mod parse_scores;
pub mod perks;
pub mod phase;
pub mod playerlogic;
pub mod prematch;
pub mod radius_damage;
mod score;
pub mod stuck_in_client;
pub mod suicide;
pub mod teams;
pub mod visuals;

pub use animated_models::{
    FAN_BLADE_ROTATE_TIME, FanBladeRotateChannel, fan_blade_dots, fan_blade_right,
    fan_blade_rotate_channel, fan_blade_rotate_delta,
};

pub use damage_feedback::{DAMAGE_FEEDBACK_SHADER, HIT_ALERT_ALIAS};
pub use ffa::{
    GAMETYPE_DIALOG_LINE, ffa_highest_scoring_index, ffa_player_is_better, match_end_cause,
};
pub use gamelogic::{GAME_STATE_PLAYING, USE_START_SPAWNS_AT_START};
pub use gaps::{ScriptGap, ScriptGapCause};
pub use globallogic::{OBJECTIVE_BASED, POST_ROUND_TIME_MS};

pub use kind::GameModeKind;

pub use parse_scores::{PARSE_SCORES_CAP, ParsedScores, parse_scores};
pub use perks::copycat_weapnext_bind_active;
pub use phase::Team;
pub use playerlogic::{
    HUD_STATUS_DEAD, MP_CONNECTED, MP_GLOBAL_INTERMISSION, is_last_round, time_until_spawn,
};
pub use prematch::{
    DEFAULT_ALLIES_CHARSET, DEFAULT_AXIS_CHARSET, FACTION_ICON_COL, FACTION_TABLE,
    FACTION_VOICE_PREFIX_COL, MATCH_START_MS, PrematchStep,
};

pub use radius_damage::{
    G_CAN_DAMAGE_CONTENTS_MASK, g_can_damage_player_vis_scale, g_radius_damage_amount,
    g_radius_damage_area_half_extent, radius_damage_distance_to_aabb,
};
pub use score::Score;

pub use stuck_in_client::{G_PLAYER_COLLISION_EJECT_SPEED_DEFAULT, StuckClient, stuck_in_client};
pub use suicide::is_really_alive;
pub use teams::{TEAM_COLOR_ENEMY_TEAM, TEAM_COLOR_MY_TEAM};
