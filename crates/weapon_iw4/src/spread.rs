pub const VIEWHEIGHT_CROUCH_SEAM: f32 = 40.0;

pub const VIEWHEIGHT_PRONE: f32 = 11.0;

pub const VIEWHEIGHT_STAND_SPAN: f32 = 20.0;

pub const VIEWHEIGHT_PRONE_SPAN: f32 = 29.0;

pub const AIM_SPREAD_SCALE_MAX: f32 = 255.0;

pub const AIM_SPREAD_AIR_DECAY: f32 = 0.5;

pub const SHORT2ANGLE: f32 = 0.005_493_164_062_5;

pub const AIM_SPREAD_TURN_SCALE: f32 = 0.01;

pub const AIM_SPREAD_AIR_VIEWCHANGE: f32 = 1.28;

pub const AIM_SPREAD_MOVE_SPEED_THRESHOLD_DEFAULT: f32 = 11.0;

pub const PM_TYPE_NORMAL_LINKED: i32 = 1;

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct WeaponSpreadFacts {
    pub stand_min: f32,
    pub ducked_min: f32,
    pub prone_min: f32,
    pub stand_max: f32,
    pub ducked_max: f32,
    pub prone_max: f32,
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct WeaponAimSpreadDecayFacts {
    pub decay_rate: f32,
    pub fire_add: f32,
    pub turn_add: f32,
    pub move_add: f32,
    pub ducked_decay: f32,
    pub prone_decay: f32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SpreadOverrideState {
    None = 0,

    ForceMax = 1,

    ForceBoth = 2,
}

impl SpreadOverrideState {
    pub fn from_i32(v: i32) -> Self {
        match v {
            1 => Self::ForceMax,
            2 => Self::ForceBoth,
            _ => Self::None,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SpreadCone {
    pub min: f32,
    pub max: f32,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct AimSpreadState {
    pub aim_spread_scale: f32,
    pub spread_override: i32,
    pub spread_override_state: i32,
}

#[derive(Clone, Copy, Debug, Default)]
pub struct AimSpreadMotion {
    pub frametime: f32,
    pub cmd_angles: [i32; 3],
    pub old_angles: [i32; 3],
    pub forwardmove: i8,
    pub rightmove: i8,
    pub velocity_xy: [f32; 2],
    pub speed: i32,

    pub move_speed_threshold: f32,
}

pub const PERK_BULLETACCURACY: u32 = 2;

pub const PERK_WEAP_SPREAD_MULTIPLIER_DEFAULT: f32 = 0.65;

#[must_use]
pub fn perk_weap_spread_multiplier(perks0: u32) -> f32 {
    if perks0 & PERK_BULLETACCURACY != 0 {
        PERK_WEAP_SPREAD_MULTIPLIER_DEFAULT
    } else {
        1.0
    }
}

pub fn bg_get_spread_for_weapon(
    view_height_current: f32,
    spread_override: i32,
    spread_override_state: SpreadOverrideState,
    weap: &WeaponSpreadFacts,
    perk_extra_spread: f32,
) -> SpreadCone {
    let (mut min, mut max) = match spread_override_state {
        SpreadOverrideState::ForceBoth => {
            let v = spread_override as f32;
            (v, v)
        }
        _ => {
            let (min, max) = blend_hip_spread(view_height_current, weap);
            if spread_override_state == SpreadOverrideState::ForceMax {
                (min, spread_override as f32)
            } else {
                (min, max)
            }
        }
    };
    if perk_extra_spread != 1.0 {
        min *= perk_extra_spread;
        max *= perk_extra_spread;
    }
    SpreadCone { min, max }
}

pub fn fire_weapon_spread_degrees(
    cone: SpreadCone,
    ads_spread: f32,
    f_weapon_pos_frac: f32,
    aim_spread_scale: f32,
) -> f32 {
    let norm = (aim_spread_scale / AIM_SPREAD_SCALE_MAX).clamp(0.0, 1.0);
    if f_weapon_pos_frac == 1.0 {
        ads_spread + (cone.max - ads_spread) * norm
    } else {
        cone.min + (cone.max - cone.min) * norm
    }
}

pub fn pm_add_aim_spread_fire(aim_spread_scale: &mut f32, f_weapon_pos_frac: f32, fire_add: f32) {
    if f_weapon_pos_frac == 1.0 {
        return;
    }
    let mut v = *aim_spread_scale + fire_add * AIM_SPREAD_SCALE_MAX;
    if v > AIM_SPREAD_SCALE_MAX {
        v = AIM_SPREAD_SCALE_MAX;
    }
    *aim_spread_scale = v;
}

pub fn pm_adjust_aim_spread_scale(
    state: &mut AimSpreadState,
    weap: &WeaponSpreadFacts,
    decay: &WeaponAimSpreadDecayFacts,
    ground_entity_num: i32,
    pm_type: i32,
    e_flags: u32,
    f_weapon_pos_frac: f32,
    motion: &AimSpreadMotion,
) {
    let airborne =
        ground_entity_num == playerstate_iw4::ENTITYNUM_NONE && pm_type != PM_TYPE_NORMAL_LINKED;

    let mut spread_override_scale = 1.0_f32;
    let (decrease, increase) = if decay.decay_rate == 0.0 {
        (1.0_f32, 0.0_f32)
    } else {
        let mut wpn_scale = decay.decay_rate;
        spread_override_scale =
            override_scale(state.spread_override, weap.stand_min, weap.stand_max);
        if airborne {
            wpn_scale *= AIM_SPREAD_AIR_DECAY;
        } else if e_flags & playerstate_iw4::eflags::PRONE != 0 {
            wpn_scale *= decay.prone_decay;
            spread_override_scale =
                override_scale(state.spread_override, weap.prone_min, weap.prone_max);
        } else if e_flags & playerstate_iw4::eflags::DUCK != 0 {
            wpn_scale *= decay.ducked_decay;
            spread_override_scale =
                override_scale(state.spread_override, weap.ducked_min, weap.ducked_max);
        }

        if state.spread_override_state == SpreadOverrideState::ForceMax as i32 {
            let denom = if spread_override_scale == 0.0 {
                1.0
            } else {
                spread_override_scale
            };
            (wpn_scale * motion.frametime / denom, 0.0)
        } else {
            let decrease = wpn_scale * motion.frametime;
            let increase = if f_weapon_pos_frac == 1.0 {
                0.0
            } else {
                viewchange_increase(decay, motion, airborne) * motion.frametime
            };
            (decrease, increase)
        }
    };

    let mut scale = if increase <= 0.0 {
        state.aim_spread_scale - decrease * AIM_SPREAD_SCALE_MAX
    } else {
        state.aim_spread_scale + increase * AIM_SPREAD_SCALE_MAX
    };

    if state.spread_override_state == SpreadOverrideState::ForceMax as i32
        && scale * spread_override_scale < AIM_SPREAD_SCALE_MAX
    {
        state.spread_override_state = 0;
        scale *= spread_override_scale;
    }

    state.aim_spread_scale = scale.clamp(0.0, AIM_SPREAD_SCALE_MAX);
}

fn override_scale(spread_override: i32, min: f32, max: f32) -> f32 {
    let span = max - min;
    if span == 0.0 {
        1.0
    } else {
        (spread_override as f32 - min) / span
    }
}

fn viewchange_increase(
    decay: &WeaponAimSpreadDecayFacts,
    motion: &AimSpreadMotion,
    airborne: bool,
) -> f32 {
    let mut viewchange = 0.0_f32;
    if decay.turn_add != 0.0 && motion.frametime != 0.0 {
        for i in 0..2 {
            let a1 = motion.cmd_angles[i] as f32 * SHORT2ANGLE;
            let a2 = motion.old_angles[i] as f32 * SHORT2ANGLE;
            let delta = angle_delta_abs(a1, a2);
            viewchange += delta * AIM_SPREAD_TURN_SCALE * decay.turn_add / motion.frametime;
        }
    }
    if decay.move_add != 0.0 && (motion.forwardmove != 0 || motion.rightmove != 0) {
        let speed_sq = motion.velocity_xy[0] * motion.velocity_xy[0]
            + motion.velocity_xy[1] * motion.velocity_xy[1];
        let thr = if motion.move_speed_threshold > 0.0 {
            motion.move_speed_threshold
        } else {
            AIM_SPREAD_MOVE_SPEED_THRESHOLD_DEFAULT
        };
        if speed_sq > thr * thr {
            let speed = libm::sqrtf(speed_sq);
            let denom = if motion.speed > 0 {
                motion.speed as f32
            } else {
                1.0
            };
            viewchange += decay.move_add * speed / denom;
        }
    }
    if airborne {
        viewchange += AIM_SPREAD_AIR_VIEWCHANGE * 2.0;
    }
    viewchange
}

fn angle_delta_abs(a: f32, b: f32) -> f32 {
    let mut d = a - b;
    while d > 180.0 {
        d -= 360.0;
    }
    while d < -180.0 {
        d += 360.0;
    }
    d.abs()
}

fn blend_hip_spread(view_height_current: f32, weap: &WeaponSpreadFacts) -> (f32, f32) {
    if view_height_current <= VIEWHEIGHT_CROUCH_SEAM {
        let t = (view_height_current - VIEWHEIGHT_PRONE) / VIEWHEIGHT_PRONE_SPAN;
        let min = weap.prone_min + t * (weap.ducked_min - weap.prone_min);
        let max = (weap.ducked_max - weap.prone_max) * t + weap.prone_max;
        (min, max)
    } else {
        let t = (view_height_current - VIEWHEIGHT_CROUCH_SEAM) / VIEWHEIGHT_STAND_SPAN;
        let min = weap.ducked_min + t * (weap.stand_min - weap.ducked_min);
        let max = (weap.stand_max - weap.ducked_max) * t + weap.ducked_max;
        (min, max)
    }
}
