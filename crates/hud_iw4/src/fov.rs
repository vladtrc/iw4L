use crate::iris::{WeaponAdsOverlayFacts, cg_calc_ads_overlay_zoom};

pub const CG_FOV_DEFAULT: f32 = 65.0;

pub const CG_FOV_MIN_DEFAULT: f32 = 10.0;

pub const CG_FOV_SCALE_DEFAULT: f32 = 1.0;

pub const PM_TYPE_INTERMISSION: i32 = 6;

pub const LINK_FLAGS_FORCE_ADS_ZOOM_FOV: u32 = 4;

pub const EFLAGS_TURRET_FOV: u32 = 0xc00;

pub const CG_TANHALF_FOV_Y_SCALE: f32 = 0.75;

pub const CG_TAN_HALF_FOV_65: f64 = 0.6370702385902405;

const DEG2RAD: f32 = core::f32::consts::PI / 180.0;
const RAD2DEG: f32 = 180.0 / core::f32::consts::PI;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CgCalcFovInputs {
    pub cg_fov: f32,

    pub pm_type: i32,

    pub link_flags: u32,

    pub e_flags: u32,

    pub weapon_index_nonzero: bool,

    pub aim_down_sight: bool,

    pub ads_zoom_fov: f32,

    pub overlay_zoom: f32,

    pub fov_scale: f32,

    pub fov_min: f32,
}

#[must_use]
pub fn com_fminf(a: f32, b: f32) -> f32 {
    if (a < b) != (a == b) { a } else { b }
}

#[must_use]
pub fn cg_calc_fov(i: &CgCalcFovInputs) -> f32 {
    let mut fov = if i.pm_type == PM_TYPE_INTERMISSION {
        i.cg_fov
    } else if (i.link_flags & LINK_FLAGS_FORCE_ADS_ZOOM_FOV) != 0 && i.weapon_index_nonzero {
        i.ads_zoom_fov
    } else {
        let mut local = i.cg_fov;
        if i.aim_down_sight {
            let ads_target = com_fminf(local, i.ads_zoom_fov);
            local = (1.0 - i.overlay_zoom) * local + ads_target * i.overlay_zoom;
        }
        local
    };

    if (i.e_flags & EFLAGS_TURRET_FOV) != 0 {}

    fov *= i.fov_scale;
    if (fov < i.fov_min) != (fov == i.fov_min) {
        fov = i.fov_min;
    }
    fov
}

#[must_use]
pub fn cg_calc_fov_from_ads(
    i: &CgCalcFovInputs,
    f_weapon_pos_frac: f32,
    b_position_to_ads: bool,
    overlay: &WeaponAdsOverlayFacts,
) -> (f32, f32) {
    let overlay_zoom = cg_calc_ads_overlay_zoom(f_weapon_pos_frac, b_position_to_ads, overlay);
    let mut inputs = *i;
    inputs.overlay_zoom = overlay_zoom;
    (cg_calc_fov(&inputs), overlay_zoom)
}

#[must_use]
pub fn cg_horizontal_to_vertical_fov_deg(horizontal_deg: f32) -> f32 {
    let tan_half_y = libm::tanf(horizontal_deg * DEG2RAD * 0.5) * CG_TANHALF_FOV_Y_SCALE;
    2.0 * libm::atanf(tan_half_y) * RAD2DEG
}

#[must_use]
pub fn cg_tan_half_fov(horizontal_deg: f32, view_aspect: f32) -> (f32, f32) {
    let tan_half_y = libm::tanf(horizontal_deg * DEG2RAD * 0.5) * CG_TANHALF_FOV_Y_SCALE;
    (tan_half_y * view_aspect, tan_half_y)
}

#[must_use]
pub fn cg_zoom_sensitivity(horizontal_deg: f32) -> f32 {
    libm::tanf(horizontal_deg * DEG2RAD * 0.5) / (CG_TAN_HALF_FOV_65 as f32)
}
