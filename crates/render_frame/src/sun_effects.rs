//! Authored sun sprite/flare and fullscreen blind/glare.
//!
//! Installed with the world; camera-dependent input travels with the existing
//! published frame. Missing data turns only this effect off.

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct SunEffectsDef {
    pub sprite_material: Option<u32>,
    pub flare_material: Option<u32>,
    pub sprite_image: Option<u32>,
    pub flare_image: Option<u32>,
    pub sprite_size: f32,
    pub flare_min_size: f32,
    pub flare_min_dot: f32,
    pub flare_max_size: f32,
    pub flare_max_dot: f32,
    pub flare_max_alpha: f32,
    pub flare_fade_in_ms: i32,
    pub flare_fade_out_ms: i32,
    pub blind_min_dot: f32,
    pub blind_max_dot: f32,
    pub blind_max_darken: f32,
    pub blind_fade_in_ms: i32,
    pub blind_fade_out_ms: i32,
    pub glare_min_dot: f32,
    pub glare_max_dot: f32,
    pub glare_max_lighten: f32,
    pub glare_fade_in_ms: i32,
    pub glare_fade_out_ms: i32,
    pub direction: [f32; 3],
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct SunEffectsFrame {
    pub world_generation: Option<u64>,
    pub camera_key: u64,
    pub camera_cut: bool,
    pub dt_ms: i32,
    pub view_dot: f32,
    pub clip: [f32; 4],
    pub viewport_w: f32,
    pub viewport_h: f32,
    pub sprite_size: f32,
    pub flare_size: f32,
    pub flare_alpha: f32,
    pub flare_fade_in_ms: i32,
    pub flare_fade_out_ms: i32,
    pub blind_goal: f32,
    pub blind_fade_in_ms: i32,
    pub blind_fade_out_ms: i32,
    pub glare_goal: f32,
    pub glare_fade_in_ms: i32,
    pub glare_fade_out_ms: i32,
    pub behind_camera: bool,
}

#[must_use]
pub fn angular_lerp(dot: f32, min_dot: f32, max_dot: f32) -> f32 {
    if !dot.is_finite() || dot <= min_dot {
        return 0.0;
    }
    if max_dot <= min_dot || dot >= max_dot {
        return 1.0;
    }
    ((dot - min_dot) / (max_dot - min_dot)).clamp(0.0, 1.0)
}
