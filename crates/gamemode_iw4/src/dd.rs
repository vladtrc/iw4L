use crate::anchors;
use crate::visuals::{BombExplodeVisualChannel, BombSiteDestroyChannel};

pub const GAMETYPE_TOKEN: &str = "dd";

pub const DISPLAY_NAME: &str = "DEMOLITION";

pub const WIN_LIMIT: u32 = 2;

pub const ROUND_LIMIT: u32 = 3;

pub const ROUND_SWITCH: u32 = 1;

pub const TIME_LIMIT_MS: u32 = 180_000;

pub const PLANT_MS: u32 = 5_000;

pub const DEFUSE_MS: u32 = 5_000;

pub const BOMB_FUSE_MS: u32 = 45_000;

pub const ADD_TIME_MS: u32 = 120_000;

pub const ADD_TIME_DVAR: &str = "scr_dd_addtime";

pub const ROUND_END_MS: u32 = 4_000;

pub const SWITCH_SIDES_MS: u32 = 4_000;

pub const MATCH_END_MS: u32 = 4_000;

pub const BRIEFCASE_PLANT: &str = "briefcase_bomb_mp";

pub const BRIEFCASE_DEFUSE: &str = "briefcase_bomb_defuse_mp";

pub const PLANTED_MODEL: &str = "prop_suitcase_bomb";

pub const DESTROYED_MODEL: &str = "com_bomb_objective_d";

pub const IDLE_OBJECTIVE_MODEL: &str = "com_bomb_objective";

pub const SPAWN_ATTACKER: &str = "mp_dd_spawn_attacker";

pub const SPAWN_ATTACKER_A: &str = "mp_dd_spawn_attacker_a";

pub const SPAWN_ATTACKER_B: &str = "mp_dd_spawn_attacker_b";

pub const START_SPAWN_ATTACKER: &str = "mp_dd_spawn_attacker_start";

pub const SPAWN_DEFENDER: &str = "mp_dd_spawn_defender";

pub const SPAWN_DEFENDER_A: &str = "mp_dd_spawn_defender_a";

pub const SPAWN_DEFENDER_B: &str = "mp_dd_spawn_defender_b";

pub const START_SPAWN_DEFENDER: &str = "mp_dd_spawn_defender_start";

pub const BOMBZONE_GAMEOBJECT: &str = anchors::BOMBZONE_GAMEOBJECT;

pub const DD_BOMBZONE_TAG: &str = anchors::DD_BOMBZONE_TAG;

pub const SUITCASE_TIMER_ALIAS: &str = anchors::SUITCASE_BOMB_TIMER_ALIAS;

pub fn is_briefcase_use_weapon(name: &str) -> bool {
    name == BRIEFCASE_PLANT || name == BRIEFCASE_DEFUSE
}

pub fn fuse_remaining_ms(elapsed_ms: u32) -> u32 {
    BOMB_FUSE_MS.saturating_sub(elapsed_ms)
}

pub fn plant_progress(held_ms: u32) -> f32 {
    progress_01(held_ms, PLANT_MS)
}

pub fn defuse_progress(held_ms: u32) -> f32 {
    progress_01(held_ms, DEFUSE_MS)
}

fn progress_01(held_ms: u32, total_ms: u32) -> f32 {
    if total_ms == 0 {
        return 1.0;
    }
    (held_ms as f32 / total_ms as f32).clamp(0.0, 1.0)
}

pub fn site_model_for_phase(planted: bool, destroyed: bool) -> &'static str {
    if destroyed {
        DESTROYED_MODEL
    } else if planted {
        PLANTED_MODEL
    } else {
        IDLE_OBJECTIVE_MODEL
    }
}

pub fn site_brush_solid(destroyed: bool) -> bool {
    !destroyed
}

pub const BOMB_SITE_DESTROY_CHANNELS: &[BombSiteDestroyChannel] = &[
    BombSiteDestroyChannel::WorldModel,
    BombSiteDestroyChannel::UnlinkBrushSolid,
    BombSiteDestroyChannel::ExplodeFx,
    BombSiteDestroyChannel::RadiusDamage,
];

pub const BOMB_EXPLODE_VISUAL_CHANNELS: &[BombExplodeVisualChannel] =
    &[BombExplodeVisualChannel::ScriptSpawnFx];

pub const PLANTED_BOMB_EXPLODE_FX_PATH: Option<&str> = Some("explosions/tanker_explosion");

pub const EXPLODE_RADIUS: f32 = 512.0;

pub const EXPLODE_INNER: f32 = 200.0;

pub const EXPLODE_OUTER: f32 = 20.0;

pub const EXPLODE_FX_Z: f32 = 50.0;

pub const EXPLODE_SOUND_ALIAS: &str = "exp_suitcase_bomb_main";

pub const BRUSH_CLASSNAME: &str = crate::visuals::SCRIPT_BRUSHMODEL_CLASSNAME;
