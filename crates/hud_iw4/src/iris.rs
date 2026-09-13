pub const LOW_RES_VIEWPORT_MAX_HEIGHT: f32 = 480.0;

pub const ADS_IRIS_ZOOM_ACTIVE_MIN: f32 = 0.01;

pub const ADS_OVERLAY_ONE_QUAD_MIN_WIDTH: f32 = 320.0;

pub const ADS_OVERLAY_ONE_QUAD_MIN_HEIGHT: f32 = 240.0;

pub const ADS_OVERLAY_ONE_QUAD_HALF: f32 = 0.5;

pub const ADS_OVERLAY_FOUR_QUAD_LETTERBOX_SCALE: f32 = 2.0;

pub const OTHER_FLAGS_EMP_OVERLAY_MATERIAL: u32 = 0x400;

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct WeaponAdsOverlayFacts {
    pub ads_zoom_in_frac: f32,

    pub ads_zoom_out_frac: f32,

    pub overlay_material: u32,

    pub overlay_material_low_res: u32,

    pub overlay_material_emp: u32,

    pub overlay_material_emp_low_res: u32,

    pub overlay_reticle: i32,

    pub ads_overlay_width: f32,

    pub ads_overlay_height: f32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AdsOverlayMaterialSlot {
    None,

    OverlayMaterial,

    OverlayMaterialLowRes,

    OverlayMaterialEmp,

    OverlayMaterialEmpLowRes,
}

impl AdsOverlayMaterialSlot {
    #[must_use]
    pub fn material_handle(self, weap: &WeaponAdsOverlayFacts) -> u32 {
        match self {
            Self::None => 0,
            Self::OverlayMaterial => weap.overlay_material,
            Self::OverlayMaterialLowRes => weap.overlay_material_low_res,
            Self::OverlayMaterialEmp => weap.overlay_material_emp,
            Self::OverlayMaterialEmpLowRes => weap.overlay_material_emp_low_res,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CgWeapReticleZoom {
    pub active: bool,

    pub zoom: f32,
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct AdsOverlayQuad {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
    pub flip_s: bool,
    pub flip_t: bool,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CgDrawAdsOverlayLayout {
    pub quads: [AdsOverlayQuad; 4],
    pub quad_count: u8,

    pub inner_x: f32,
    pub inner_y: f32,
    pub inner_w: f32,
    pub inner_h: f32,
}

impl CgDrawAdsOverlayLayout {
    #[must_use]
    pub fn live_quads(&self) -> &[AdsOverlayQuad] {
        &self.quads[..usize::from(self.quad_count)]
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CgDrawWeapReticle {
    pub hip_reticle_alpha: f32,

    pub overlay_alpha: Option<f32>,

    pub material: AdsOverlayMaterialSlot,
}

#[must_use]
pub fn cg_ads_overlay_material(
    weap: &WeaponAdsOverlayFacts,
    viewport_height: f32,
    other_flags: u32,
) -> AdsOverlayMaterialSlot {
    let low_res = viewport_height <= LOW_RES_VIEWPORT_MAX_HEIGHT;
    let emp_branch =
        (other_flags & OTHER_FLAGS_EMP_OVERLAY_MATERIAL) != 0 && weap.overlay_material_emp != 0;

    if emp_branch {
        pick_low_res_or_default(
            low_res,
            weap.overlay_material_emp_low_res,
            weap.overlay_material_emp,
            AdsOverlayMaterialSlot::OverlayMaterialEmpLowRes,
            AdsOverlayMaterialSlot::OverlayMaterialEmp,
        )
    } else {
        pick_low_res_or_default(
            low_res,
            weap.overlay_material_low_res,
            weap.overlay_material,
            AdsOverlayMaterialSlot::OverlayMaterialLowRes,
            AdsOverlayMaterialSlot::OverlayMaterial,
        )
    }
}

fn pick_low_res_or_default(
    low_res: bool,
    low_res_handle: u32,
    default_handle: u32,
    low_res_slot: AdsOverlayMaterialSlot,
    default_slot: AdsOverlayMaterialSlot,
) -> AdsOverlayMaterialSlot {
    if low_res && low_res_handle != 0 {
        low_res_slot
    } else if default_handle != 0 {
        default_slot
    } else {
        AdsOverlayMaterialSlot::None
    }
}

#[must_use]
pub fn cg_iris_overlay_configured(weap: &WeaponAdsOverlayFacts) -> bool {
    weap.overlay_material != 0 || weap.overlay_reticle != 0
}

#[must_use]
pub fn cg_calc_ads_overlay_zoom(
    f_weapon_pos_frac: f32,
    b_position_to_ads: bool,
    weap: &WeaponAdsOverlayFacts,
) -> f32 {
    if f_weapon_pos_frac == 0.0 {
        return 0.0;
    }

    let window = if b_position_to_ads {
        weap.ads_zoom_in_frac
    } else {
        weap.ads_zoom_out_frac
    };

    if window <= 0.0 {
        if f_weapon_pos_frac >= 1.0 {
            return 1.0;
        }
        return 0.0;
    }

    com_clamp((f_weapon_pos_frac - (1.0 - window)) / window, 0.0, 1.0)
}

#[must_use]
fn com_clamp(val: f32, min: f32, max: f32) -> f32 {
    let mut out = val;
    if (out - max) > 0.0 {
        out = max;
    }
    if (min - val) > 0.0 { min } else { out }
}

#[must_use]
pub fn cg_get_weap_reticle_zoom(
    f_weapon_pos_frac: f32,
    b_position_to_ads: bool,
    weap: &WeaponAdsOverlayFacts,
) -> CgWeapReticleZoom {
    if !cg_iris_overlay_configured(weap) {
        return CgWeapReticleZoom {
            active: false,
            zoom: 0.0,
        };
    }

    let zoom = cg_calc_ads_overlay_zoom(f_weapon_pos_frac, b_position_to_ads, weap);
    let active = zoom > ADS_IRIS_ZOOM_ACTIVE_MIN;
    CgWeapReticleZoom {
        active,
        zoom: if active { zoom } else { 0.0 },
    }
}

#[must_use]
pub fn cg_draw_weap_reticle_hip_alpha(
    f_weapon_pos_frac: f32,
    b_position_to_ads: bool,
    weap: &WeaponAdsOverlayFacts,
) -> f32 {
    let gate = cg_get_weap_reticle_zoom(f_weapon_pos_frac, b_position_to_ads, weap);
    if gate.active { 1.0 - gate.zoom } else { 1.0 }
}

#[must_use]
pub fn cg_viewweapon_drawgun(
    cubemap_shot: bool,
    cg_draw_gun: bool,
    iris: CgWeapReticleZoom,
) -> bool {
    !(cubemap_shot || !cg_draw_gun || iris.active)
}

#[must_use]
pub fn cg_viewweapon_drawgun_skip(
    cubemap_shot: bool,
    cg_draw_gun: bool,
    iris: CgWeapReticleZoom,
) -> Option<&'static str> {
    if cubemap_shot {
        Some("cubemap")
    } else if !cg_draw_gun {
        Some("cg_drawgun")
    } else if iris.active {
        Some("iris")
    } else {
        None
    }
}

#[must_use]
pub fn cg_ads_overlay_uses_four_quads(width: f32, height: f32) -> bool {
    width <= ADS_OVERLAY_ONE_QUAD_MIN_WIDTH && height <= ADS_OVERLAY_ONE_QUAD_MIN_HEIGHT
}

fn overlay_quad(x: f32, y: f32, w: f32, h: f32, flip_s: bool, flip_t: bool) -> AdsOverlayQuad {
    AdsOverlayQuad {
        x,
        y,
        w,
        h,
        flip_s,
        flip_t,
    }
}

#[must_use]
pub fn cg_draw_ads_overlay_layout(width: f32, height: f32) -> CgDrawAdsOverlayLayout {
    if cg_ads_overlay_uses_four_quads(width, height) {
        let quads = [
            overlay_quad(-width, -height, width, height, false, false),
            overlay_quad(0.0, -height, width, height, true, false),
            overlay_quad(-width, 0.0, width, height, false, true),
            overlay_quad(0.0, 0.0, width, height, true, true),
        ];
        CgDrawAdsOverlayLayout {
            quads,
            quad_count: 4,
            inner_x: -width,
            inner_y: -height,
            inner_w: width * ADS_OVERLAY_FOUR_QUAD_LETTERBOX_SCALE,
            inner_h: height * ADS_OVERLAY_FOUR_QUAD_LETTERBOX_SCALE,
        }
    } else {
        let x = -width * ADS_OVERLAY_ONE_QUAD_HALF;
        let y = -height * ADS_OVERLAY_ONE_QUAD_HALF;
        let mut quads = [AdsOverlayQuad::default(); 4];
        quads[0] = overlay_quad(x, y, width, height, false, false);
        CgDrawAdsOverlayLayout {
            quads,
            quad_count: 1,
            inner_x: x,
            inner_y: y,
            inner_w: width,
            inner_h: height,
        }
    }
}

#[must_use]
pub fn cg_draw_weap_reticle(
    f_weapon_pos_frac: f32,
    b_position_to_ads: bool,
    viewport_height: f32,
    other_flags: u32,
    weap: &WeaponAdsOverlayFacts,
) -> CgDrawWeapReticle {
    let gate = cg_get_weap_reticle_zoom(f_weapon_pos_frac, b_position_to_ads, weap);
    let material = cg_ads_overlay_material(weap, viewport_height, other_flags);
    if gate.active {
        CgDrawWeapReticle {
            hip_reticle_alpha: 1.0 - gate.zoom,
            overlay_alpha: Some(gate.zoom),
            material,
        }
    } else {
        CgDrawWeapReticle {
            hip_reticle_alpha: 1.0,
            overlay_alpha: None,
            material,
        }
    }
}
