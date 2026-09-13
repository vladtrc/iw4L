pub fn createfx_enabled(createfx_dvar: &str) -> bool {
    !createfx_dvar.is_empty()
}

pub fn load_already_started(started: bool) -> bool {
    started
}

pub const CREATEFX_SKIPPED_THREADS: &[&str] = &[
    "maps/mp/_minefields::minefields",
    "maps/mp/_radiation::radiation",
    "maps/mp/_shutter::main",
    "maps/mp/_destructables::init",
    "common_scripts/_elevator::init",
    "common_scripts/_dynamic_world::init",
    "common_scripts/_destructible::init",
    "common_scripts/_pipes::main",
];

pub const THERMAL_VISION_INVERT: &str = "thermal_snowlevel_mp";

pub const THERMAL_VISION_DEFAULT: &str = "thermal_mp";

pub const THERMAL_BODY_SNOW: &str = "thermalbody_snowlevel";

pub fn thermal_vision(invert: bool) -> &'static str {
    if invert {
        THERMAL_VISION_INVERT
    } else {
        THERMAL_VISION_DEFAULT
    }
}

pub const VISION_NIGHT: &str = "default_night_mp";

pub const VISION_MISSILECAM: &str = "missilecam";

pub const LANTERN_GLOW_TARGETNAME: &str = "lantern_glowFX_origin";

pub const LANTERN_LIGHT_FX: &str = "props/glow_latern";

pub const LANTERN_LOOP_INTERVAL: f32 = 0.3;

pub const LOAD_TRIGGER_CLASSNAMES: &[&str] = &[
    "trigger_multiple",
    "trigger_once",
    "trigger_use",
    "trigger_radius",
    "trigger_lookat",
    "trigger_damage",
];

pub const TRIGGER_HURT_CLASSNAME: &str = "trigger_hurt";

pub const HURT_THINK_WAIT: f32 = 0.5;

pub const EXPLODER_LOAD_RETRY_WAIT: f32 = 4.0;

pub const REQUIRED_MAP_ASPECT_RATIO_DEFAULT: f32 = 1.0;

pub const SM_SUN_SHADOW_SCALE: f32 = 1.0;
pub const R_SPECULAR_COLOR_SCALE: f32 = 2.5;
pub const R_DIFFUSE_COLOR_SCALE: f32 = 1.0;
pub const R_LIGHT_GRID_ENABLE_TWEAKS: i32 = 0;
pub const R_LIGHT_GRID_INTENSITY: f32 = 1.0;
pub const R_LIGHT_GRID_CONTRAST: f32 = 0.0;

pub const LEVEL_FUNC_SLOTS: &[&str] = &[
    "precacheMpAnim",
    "scriptModelPlayAnim",
    "scriptModelClearAnim",
    "damagefeedback",
    "setTeamHeadIcon",
];
