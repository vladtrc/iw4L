use crate::scene_light::{LIGHT_PACK_ONE, color_srgb_to_linear};

pub const CONST_SRC_CODE_GLOW_SETUP: u16 = 0x2C;

pub const CONST_SRC_CODE_GLOW_APPLY: u16 = 0x2D;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct GlowBloomConsts {
    pub setup: [f32; 4],
    pub apply: [f32; 4],
}

pub const R_GLOW_DEFAULT: bool = true;

pub const R_FULLBRIGHT_DEFAULT: bool = false;

pub const R_GLOW_USE_TWEAKS_DEFAULT: bool = false;

pub const R_GLOW_TWEAK_ENABLE_DEFAULT: bool = false;

pub const R_GLOW_TWEAK_RADIUS0_DEFAULT: f32 = 5.0;

pub const R_GLOW_TWEAK_RADIUS0_MAX: f32 = 32.0;

pub const R_GLOW_TWEAK_INTENSITY0_DEFAULT: f32 = 1.0;

pub const R_GLOW_TWEAK_INTENSITY0_MAX: f32 = 20.0;

pub const R_GLOW_TWEAK_CUTOFF_DEFAULT: f32 = 0.5;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct GlowViewInfo {
    pub enable: bool,
    pub cutoff: f32,
    pub desaturation: f32,
    pub intensity: f32,
    pub radius: f32,
}

impl GlowViewInfo {
    #[must_use]
    pub const fn tweak_register_defaults() -> Self {
        Self {
            enable: R_GLOW_TWEAK_ENABLE_DEFAULT,
            cutoff: R_GLOW_TWEAK_CUTOFF_DEFAULT,
            desaturation: 0.0,
            intensity: R_GLOW_TWEAK_INTENSITY0_DEFAULT,
            radius: R_GLOW_TWEAK_RADIUS0_DEFAULT,
        }
    }
}

#[must_use]
pub fn r_select_glow_view_info(
    authored: GlowViewInfo,
    use_tweaks: bool,
    tweaks: GlowViewInfo,
) -> GlowViewInfo {
    if use_tweaks { tweaks } else { authored }
}

#[must_use]
pub fn r_using_glow(
    enable: bool,
    intensity: f32,
    radius: f32,
    r_glow: bool,
    r_fullbright: bool,
) -> bool {
    enable && !r_fullbright && r_glow && intensity != 0.0 && radius != 0.0
}

#[must_use]
pub fn r_set_glow_info(cutoff: f32, desaturation: f32, intensity: f32) -> Option<GlowBloomConsts> {
    if intensity == 0.0 {
        return None;
    }
    let lin = color_srgb_to_linear(cutoff);
    Some(GlowBloomConsts {
        setup: [
            lin,
            LIGHT_PACK_ONE / (LIGHT_PACK_ONE - lin),
            0.0,
            desaturation,
        ],
        apply: [0.0, 0.0, 0.0, intensity],
    })
}
