pub const MSEC_TO_SEC: f32 = 0.001;

pub const HUD_BLOOD_OVERLAY_LERP_RATE_DEFAULT: f32 = 0.3;

pub const HEALTH_FRAC_PM_TYPE_NONE: i32 = 8;

const BLOOD_OVERLAY_HIDDEN_PM_TYPES: [i32; 6] = [2, 3, 5, 6, 8, 9];

pub fn cg_get_health_fraction(health: i32, max_health: i32, pm_type: i32) -> f32 {
    if health == 0 || max_health == 0 || pm_type == HEALTH_FRAC_PM_TYPE_NONE {
        return 0.0;
    }
    let raw = health as f32 / max_health as f32;
    raw.clamp(0.0, 1.0)
}

pub fn cg_blood_overlay_lerp(
    mut intensity: f32,
    health_frac: f32,
    frametime_ms: i32,
    lerp_rate: f32,
) -> f32 {
    let target = 1.0 - health_frac;
    if target < intensity {
        if lerp_rate <= 0.0 {
            intensity = target;
        } else {
            intensity -= frametime_ms as f32 * MSEC_TO_SEC * lerp_rate;
        }
    }
    if intensity < target {
        intensity = target;
    }
    intensity
}

pub fn cg_should_draw_blood_overlay(
    blood_enabled: bool,
    should_draw_hud: bool,
    in_killcam_hud_gate: bool,
    pm_type: i32,
) -> bool {
    if !blood_enabled || !should_draw_hud || in_killcam_hud_gate {
        return false;
    }
    !BLOOD_OVERLAY_HIDDEN_PM_TYPES.contains(&pm_type)
}

pub fn cg_splatter_envelope(t: f32, fade_in_end: f32, full_in_end: f32, fade_out_end: f32) -> f32 {
    if t < fade_in_end {
        return t / fade_in_end;
    }
    if t < full_in_end {
        return 1.0;
    }
    if t < fade_out_end {
        return 1.0 - (t - full_in_end) / (fade_out_end - full_in_end);
    }
    0.0
}

pub const SPLATTER_HEALTH_INTENSITY_SCALE: f32 = 2.0;

pub const SPLATTER_GRID_WIDTH: i32 = 0x20;
pub const SPLATTER_GRID_HEIGHT: i32 = 0x14;

pub const PAIN_VISION_TRIGGER_HEALTH_DEFAULT: f32 = 0.55;

pub const PAIN_VISION_LERP_OUT_RATE_DEFAULT: f32 = 0.3;

pub fn cg_pain_vision_must_clear(
    health_frac: f32,
    pm_type: i32,
    ps_block: bool,
    in_killcam_hud_gate: bool,
) -> bool {
    health_frac == 0.0 || pm_type == 5 || ps_block || in_killcam_hud_gate
}

pub fn cg_pain_vision_wants_armed(health_frac: f32, trigger: f32, currently_active: bool) -> bool {
    if currently_active {
        health_frac != 1.0
    } else {
        health_frac <= trigger
    }
}

pub fn cg_pain_vision_lerp_intensity(
    mut intensity: f32,
    health_frac: f32,
    frametime_ms: i32,
    lerp_out_rate: f32,
) -> f32 {
    let target = 1.0 - health_frac;
    if target < intensity && lerp_out_rate != 0.0 {
        intensity -= lerp_out_rate * frametime_ms as f32 * MSEC_TO_SEC;
    }
    if intensity < target {
        intensity = target;
    }
    intensity
}
