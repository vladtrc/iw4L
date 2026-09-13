pub const RETICLE_VIRTUAL_HALF_HEIGHT: f32 = 240.0;

pub const RETICLE_VIRTUAL_HEIGHT: f32 = 480.0;

pub const AIM_SPREAD_SCALE_MAX: f32 = 255.0;

pub const CG_CROSSHAIR_ALPHA_DEFAULT: f32 = 1.0;

pub const CG_CROSSHAIR_ALPHA_MIN_DEFAULT: f32 = 0.5;

pub const RETICLE_SIDES_ALPHA_DRAW_MIN: f32 = 0.01;

pub const EF_CROSSHAIR_TURRET_VEHICLE: u32 = 0xc00;

pub const EF_CROSSHAIR_SPECIAL_RETICLE: u32 = 0x100_000;

pub const OTHER_FLAGS_BLOCK_CROSSHAIR_HUD: u32 = 0x400;

const DEG2RAD: f32 = core::f32::consts::PI / 180.0;

const ADS_CROSSHAIR_FADE_WEIGHT: f32 = 0.5;

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct WeaponReticleFacts {
    pub i_reticle_min_ofs: i32,

    pub hip_reticle_side_pos: f32,

    pub i_reticle_side_size: i32,
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct WeaponAdsCrosshairFacts {
    pub ads_aim_pitch: f32,

    pub ads_crosshair_in_frac: f32,

    pub ads_crosshair_out_frac: f32,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CgHipCrosshairGate {
    pub rendering_third_person: bool,

    pub e_flags: u32,

    pub other_flags: u32,

    pub viewmodel_weapon_index: i32,

    pub flashbanged: bool,

    pub draw_hud: bool,

    pub dvars_allow: bool,

    pub f_weapon_pos_frac: f32,

    pub cg_draw_gun: bool,

    pub bob_gate: bool,

    pub weaponstate_primary: i32,

    pub weaponstate_secondary: i32,

    pub last_weapon_hand: i32,

    pub mantle_weapon_inactive: bool,
}

impl Default for CgHipCrosshairGate {
    fn default() -> Self {
        Self {
            rendering_third_person: false,
            e_flags: 0,
            other_flags: 0,
            viewmodel_weapon_index: 1,
            flashbanged: false,
            draw_hud: true,
            dvars_allow: true,
            f_weapon_pos_frac: 0.0,
            cg_draw_gun: true,
            bob_gate: true,
            weaponstate_primary: 0,
            weaponstate_secondary: 0,
            last_weapon_hand: 0,
            mantle_weapon_inactive: false,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CgAdsTransition {
    pub trans_scale: f32,

    pub trans_shift: f32,
}

#[must_use]
pub fn cg_reticle_draw_size(weap: &WeaponReticleFacts, trans_scale: f32) -> [f32; 2] {
    let s = weap.i_reticle_side_size as f32 * trans_scale;
    [s, s]
}

#[must_use]
pub fn cg_hip_crosshair_visible(g: &CgHipCrosshairGate) -> bool {
    if g.rendering_third_person {
        return false;
    }
    if (g.e_flags & EF_CROSSHAIR_TURRET_VEHICLE) != 0 {
        return false;
    }
    if (g.e_flags & EF_CROSSHAIR_SPECIAL_RETICLE) != 0 {
        return false;
    }
    if g.viewmodel_weapon_index == 0 {
        return false;
    }
    if g.flashbanged {
        return false;
    }
    if !g.draw_hud || (g.other_flags & OTHER_FLAGS_BLOCK_CROSSHAIR_HUD) != 0 {
        return false;
    }

    if g.mantle_weapon_inactive {
        return false;
    }

    if g.f_weapon_pos_frac == 1.0 && g.cg_draw_gun && !g.bob_gate {
        return false;
    }
    cg_should_draw_crosshair(
        g.dvars_allow,
        g.weaponstate_primary,
        g.weaponstate_secondary,
        g.last_weapon_hand,
    )
}

#[must_use]
pub fn cg_should_draw_crosshair(
    dvars_allow: bool,
    weaponstate_primary: i32,
    weaponstate_secondary: i32,
    last_weapon_hand: i32,
) -> bool {
    if !dvars_allow {
        return false;
    }
    if matches!(weaponstate_primary, 0xD..=0xF) {
        return false;
    }
    if weaponstate_primary == 0x8 {
        return last_weapon_hand == 1 && weaponstate_secondary != 0x8;
    }
    !matches!(weaponstate_primary, 0x1..=0x5)
}

#[must_use]
pub fn cg_transition_to_ads(
    f_weapon_pos_frac: f32,
    b_position_to_ads: bool,
    weap: &WeaponAdsCrosshairFacts,
    tan_half_fov_y: f32,
) -> Option<CgAdsTransition> {
    let window = if b_position_to_ads {
        weap.ads_crosshair_in_frac
    } else {
        weap.ads_crosshair_out_frac
    };
    if window == 0.0 {
        return None;
    }
    let fa = (f_weapon_pos_frac - (1.0 - window)) / window;
    if fa <= 0.0 {
        return None;
    }
    let trans_scale = 1.0 - fa * ADS_CROSSHAIR_FADE_WEIGHT;
    let tan_half = if tan_half_fov_y > 0.0 {
        tan_half_fov_y
    } else {
        1e-4
    };
    let pitch_tan = libm::tanf(weap.ads_aim_pitch * DEG2RAD);
    let trans_shift = (fa * RETICLE_VIRTUAL_HALF_HEIGHT / tan_half) * pitch_tan;
    Some(CgAdsTransition {
        trans_scale,
        trans_shift,
    })
}

#[must_use]
pub fn cg_hip_crosshair_trans_scale(
    f_weapon_pos_frac: f32,
    b_position_to_ads: bool,
    weap: &WeaponAdsCrosshairFacts,
    tan_half_fov_y: f32,
) -> f32 {
    if f_weapon_pos_frac == 0.0 {
        return 1.0;
    }
    cg_transition_to_ads(f_weapon_pos_frac, b_position_to_ads, weap, tan_half_fov_y)
        .map(|t| t.trans_scale)
        .unwrap_or(1.0)
}

#[must_use]
pub fn cg_calc_reticle_spread(
    cone_min: f32,
    cone_max: f32,
    aim_spread_scale: f32,
    trans_scale: f32,
    tan_half_fov_y: f32,
    weap: &WeaponReticleFacts,
    draw_size: [f32; 2],
) -> [f32; 2] {
    let norm = (aim_spread_scale / AIM_SPREAD_SCALE_MAX).clamp(0.0, 1.0);
    let angle_deg = (cone_min + (cone_max - cone_min) * norm) * trans_scale;
    let tan_half = if tan_half_fov_y > 0.0 {
        tan_half_fov_y
    } else {
        1e-4
    };
    let mut scale = libm::tanf(angle_deg * DEG2RAD) * RETICLE_VIRTUAL_HALF_HEIGHT / tan_half;
    let min_ofs = weap.i_reticle_min_ofs as f32;
    if scale < min_ofs {
        scale = min_ofs;
    }
    [
        scale - weap.hip_reticle_side_pos * draw_size[0],
        scale - weap.hip_reticle_side_pos * draw_size[1],
    ]
}

#[must_use]
pub fn cg_calc_reticle_alpha(
    base_alpha: f32,
    cg_crosshair_alpha: f32,
    cg_crosshair_alpha_min: f32,
    aim_spread_scale: f32,
) -> f32 {
    let faded = base_alpha
        * cg_crosshair_alpha
        * (1.0 - (aim_spread_scale / AIM_SPREAD_SCALE_MAX).clamp(0.0, 1.0));
    let a = if faded < cg_crosshair_alpha_min {
        cg_crosshair_alpha_min
    } else {
        faded
    };
    a.clamp(0.0, 1.0)
}

pub const CROSSHAIR_POS_X_SCALE: f32 = -320.0;

pub const CROSSHAIR_POS_Y_SCALE: f32 = -240.0;

#[must_use]
#[expect(
    clippy::too_many_arguments,
    reason = "preserves CG_CalcCrosshairPosition scalar and view-axis inputs"
)]
pub fn cg_calc_crosshair_position(
    gun_pitch: f32,
    gun_yaw: f32,
    view_roll: f32,
    view_forward: [f32; 3],
    view_right: [f32; 3],
    view_up: [f32; 3],
    tan_half_fov_x: f32,
    tan_half_fov_y: f32,
) -> [f32; 2] {
    let (gun_dir, _, _) = math_iw4::angle_vectors([gun_pitch, gun_yaw, view_roll]);
    let dot =
        view_forward[0] * gun_dir[0] + view_forward[1] * gun_dir[1] + view_forward[2] * gun_dir[2];
    if 0.0 < dot && 0.0 < tan_half_fov_x {
        let x_num =
            view_right[0] * gun_dir[0] + view_right[1] * gun_dir[1] + view_right[2] * gun_dir[2];
        let y_num = view_up[0] * gun_dir[0] + view_up[1] * gun_dir[1] + view_up[2] * gun_dir[2];
        [
            (x_num / (dot * tan_half_fov_x)) * CROSSHAIR_POS_X_SCALE,
            (y_num / (dot * tan_half_fov_y)) * CROSSHAIR_POS_Y_SCALE,
        ]
    } else {
        [0.0, 0.0]
    }
}
