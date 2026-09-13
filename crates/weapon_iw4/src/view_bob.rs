use crate::placement::{
    VIEWHEIGHT_TARGET_CROUCH, VIEWHEIGHT_TARGET_PRONE, WEAPON_BOB_AMPLITUDE_BASE,
    WEAPON_BOB_AMPLITUDE_ROLL, WEAPON_BOB_LAG, WEAPON_BOB_UP_PHASE, WEAPON_IDLE_FACTOR_LERP,
    WEAPON_IDLE_PITCH_FREQ, WEAPON_IDLE_SIN_SCALE, WEAPON_IDLE_TIME_MS_SCALE, WEAPON_IDLE_YAW_FREQ,
    WeaponIdleInputs, WeaponPlacementPsInputs, bg_weapon_idle_amount_speed,
    weapon_bob_ads_attenuation,
};
use crate::sprint::PMF_SPRINTING;
use math_iw4::{angle_vectors, get_lean_fraction};
use playerstate_iw4::eflags;

pub const VIEW_BOB_MAX: f32 = 8.0;

pub const VIEW_BOB_AMP_STANDING: [f32; 2] = [0.007, 0.007];

pub const VIEW_BOB_AMP_STANDING_ADS: [f32; 2] = [0.007, 0.007];

pub const VIEW_BOB_AMP_DUCKED: [f32; 2] = [0.0075, 0.0075];

pub const VIEW_BOB_AMP_DUCKED_ADS: [f32; 2] = [0.0075, 0.0075];

pub const VIEW_BOB_AMP_PRONE: [f32; 2] = [0.02, 0.005];

pub const VIEW_BOB_AMP_SPRINTING: [f32; 2] = [0.02, 0.014];

pub const PERK_LIGHTWEIGHT_VIEW_BOB_SCALE: f32 = 0.75;

pub const PERK_LIGHTWEIGHT_VIEW_BOB_BIT: u32 = 0x0100_0000;

pub const VIEW_ORG_BOB_Z_MIN_OFS: f32 = 8.0;

pub const LAND_DEFLECT_MS: f32 = 150.0;

pub const LAND_RETURN_MS: f32 = 300.0;

pub const LAND_END_MS: f32 = 450.0;

pub const VIEWWEAPON_LAND_SCALE: f32 = 0.25;

pub const LAND_VIEW_DIP_FALL_IN: f32 = 12.0;

pub const LAND_VIEW_DIP_SPAN_IN: f32 = 26.0;

pub const LAND_VIEW_DIP_SCALE: f32 = 4.0;

pub const LAND_VIEW_DIP_MAX: i32 = 24;

const LAND_FALL_HALF: f32 = 0.5;

const LAND_FALL_FOUR: f32 = 4.0;

const LAND_FALL_TWO: f32 = 2.0;

const LAND_FALL_NEG: f32 = -1.0;

pub const EFLAGS_TURRET_VEHICLE: u32 = 0xc00;

const OVERLAY_BOB_SIGN: f32 = -1.0;

pub const BG_VIEW_KICK_SCALE: f32 = 0.2;

pub const BG_VIEW_KICK_MIN: f32 = 5.0;

pub const BG_VIEW_KICK_MAX: f32 = 90.0;

pub const VIEW_DAMAGE_DEFLECT_MS: f32 = 100.0;

pub const VIEW_DAMAGE_RETURN_MS: f32 = 400.0;

const VIEW_DAMAGE_ADS_HALF: f32 = 0.5;

const VIEW_DAMAGE_BYTE: f32 = 255.0;

const VIEW_DAMAGE_TURN: f32 = 360.0;

pub const VIEW_DAMAGE_UNDIRECTED: u32 = 255;

const BOB_CYCLE_DIV_F64: u64 = 0x406F_E000_0000_0000;

const PI_F64: u64 = 0x4009_21FB_6000_0000;

const TAU_F64: u64 = 0x4019_21FB_6000_0000;

const VERT_CYCLE_MUL_F64: u64 = 0x4010_0000_0000_0000;

const HALF_PI_F64: u64 = 0x3FF9_21FB_6000_0000;

const VERT_SIN_WEIGHT_F64: u64 = 0x3FC9_9999_A000_0000;

const VERT_MIX_F64: u64 = 0x3FE8_0000_0000_0000;

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct ViewOrgBobInputs {
    pub bob_cycle: u8,

    pub xyspeed: f32,

    pub view_height_target: i32,

    pub pm_flags: u32,

    pub weapon_pos_frac: f32,

    pub perks0: u32,
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct ViewOrgBob {
    pub vertical: f32,

    pub horizontal: f32,
}

#[inline]
fn f64_as_f32(bits: u64) -> f32 {
    f64::from_bits(bits) as f32
}

#[inline]
pub fn bg_view_bob_cycle(bob_cycle: u8) -> f32 {
    let bob = (f32::from(bob_cycle) / f64_as_f32(BOB_CYCLE_DIV_F64)) * f64_as_f32(PI_F64);
    bob + bob + f64_as_f32(TAU_F64)
}

#[inline]
fn view_bob_sinf(x: f32) -> f32 {
    libm::sinf(x)
}

fn view_bob_helper_amplitude(
    view_height_target: i32,
    pm_flags: u32,
    weapon_pos_frac: f32,
    xyspeed: f32,
    perks0: u32,
    horizontal: bool,
) -> f32 {
    let idx = if horizontal { 0 } else { 1 };
    let ads = weapon_pos_frac;
    let scale = if view_height_target == VIEWHEIGHT_TARGET_PRONE {
        VIEW_BOB_AMP_PRONE[idx]
    } else if view_height_target == VIEWHEIGHT_TARGET_CROUCH {
        (1.0 - ads) * VIEW_BOB_AMP_DUCKED[idx] + ads * VIEW_BOB_AMP_DUCKED_ADS[idx]
    } else if (pm_flags & PMF_SPRINTING) == 0 {
        (1.0 - ads) * VIEW_BOB_AMP_STANDING[idx] + ads * VIEW_BOB_AMP_STANDING_ADS[idx]
    } else {
        VIEW_BOB_AMP_SPRINTING[idx]
    };
    let mut amp = scale * xyspeed;
    if (perks0 & PERK_LIGHTWEIGHT_VIEW_BOB_BIT) != 0 {
        amp *= PERK_LIGHTWEIGHT_VIEW_BOB_SCALE;
    }
    if amp > VIEW_BOB_MAX {
        amp = VIEW_BOB_MAX;
    }
    amp
}

pub fn bg_calc_view_bob_pitch(cycle: f32, inputs: ViewOrgBobInputs) -> f32 {
    let amp = view_bob_helper_amplitude(
        inputs.view_height_target,
        inputs.pm_flags,
        inputs.weapon_pos_frac,
        inputs.xyspeed,
        inputs.perks0,
        false,
    );
    (view_bob_sinf(cycle * f64_as_f32(VERT_CYCLE_MUL_F64) + f64_as_f32(HALF_PI_F64))
        * f64_as_f32(VERT_SIN_WEIGHT_F64)
        + view_bob_sinf(cycle + cycle))
        * f64_as_f32(VERT_MIX_F64)
        * amp
}

pub fn bg_calc_view_bob_roll(cycle: f32, inputs: ViewOrgBobInputs) -> f32 {
    let amp = view_bob_helper_amplitude(
        inputs.view_height_target,
        inputs.pm_flags,
        inputs.weapon_pos_frac,
        inputs.xyspeed,
        inputs.perks0,
        true,
    );
    view_bob_sinf(cycle) * amp
}

pub fn bg_crash_land_fall_height(
    gravity: i32,
    previous_origin_z: f32,
    origin_z: f32,
    previous_velocity_z: f32,
) -> Option<f32> {
    if gravity == 0 {
        return None;
    }
    let dist = previous_origin_z - origin_z;
    let vel = previous_velocity_z;
    let acc = -(gravity as f32);
    let a = acc * LAND_FALL_HALF;
    let den = vel * vel - LAND_FALL_FOUR * a * dist;
    if den < 0.0 {
        return None;
    }
    let two_a = a * LAND_FALL_TWO;
    if two_a == 0.0 {
        return None;
    }
    let t = (-vel - libm::sqrtf(den)) / two_a;
    let land_vel = (t * acc + vel) * LAND_FALL_NEG;
    Some((land_vel * land_vel) / ((gravity as f32) * LAND_FALL_TWO))
}

pub fn bg_crash_land_view_dip(fall_height: f32) -> i32 {
    if !(fall_height > LAND_VIEW_DIP_FALL_IN) {
        return 0;
    }
    let raw = (fall_height - LAND_VIEW_DIP_FALL_IN) / LAND_VIEW_DIP_SPAN_IN * LAND_VIEW_DIP_SCALE
        + LAND_VIEW_DIP_SCALE;
    let n = libm::roundf(raw) as i32;
    if n > LAND_VIEW_DIP_MAX {
        LAND_VIEW_DIP_MAX
    } else if n < 0 {
        0
    } else {
        n
    }
}

pub fn bg_land_origin_weight(delta_ms: f32) -> Option<f32> {
    if !(delta_ms > 0.0) {
        return None;
    }
    if delta_ms < LAND_DEFLECT_MS {
        return Some(delta_ms / LAND_DEFLECT_MS);
    }
    if delta_ms >= LAND_END_MS {
        return None;
    }
    Some(1.0 - (delta_ms - LAND_DEFLECT_MS) / LAND_RETURN_MS)
}

pub fn bg_land_origin_z(delta_ms: f32, land_change: f32) -> f32 {
    match bg_land_origin_weight(delta_ms) {
        Some(weight) => weight * land_change,
        None => 0.0,
    }
}

pub fn bg_viewweapon_land_origin_z(delta_ms: i32, land_change: f32) -> f32 {
    if delta_ms < LAND_DEFLECT_MS as i32 {
        return land_change * VIEWWEAPON_LAND_SCALE * (delta_ms as f32) / LAND_DEFLECT_MS;
    }
    if delta_ms < LAND_END_MS as i32 {
        return land_change * VIEWWEAPON_LAND_SCALE * ((LAND_END_MS as i32 - delta_ms) as f32)
            / LAND_RETURN_MS;
    }
    0.0
}

pub fn bg_view_org_bob(inputs: ViewOrgBobInputs) -> ViewOrgBob {
    if !(inputs.xyspeed > 0.0) {
        return ViewOrgBob::default();
    }
    let cycle = bg_view_bob_cycle(inputs.bob_cycle);
    ViewOrgBob {
        vertical: bg_calc_view_bob_pitch(cycle, inputs),
        horizontal: bg_calc_view_bob_roll(cycle, inputs),
    }
}

const VIEW_ORG_BOB_GATE_OTHER_FLAGS: u32 = 2;

const VIEW_ORG_BOB_GATE_LINK_FLAGS: u32 = 4;

const VIEW_ORG_BOB_GATE_EFLAGS: u32 = 0x100_000;

const VIEW_ORG_BOB_GATE_PM_TYPE_A: i32 = 5;
const VIEW_ORG_BOB_GATE_PM_TYPE_B: i32 = 6;

#[must_use]
pub fn bg_should_apply_view_org_bob(
    camera_third_person: bool,
    pm_type: i32,
    other_flags: u32,
    link_flags: u32,
    e_flags: u32,
    f_weapon_pos_frac: f32,
    overlay_reticle: i32,
) -> bool {
    if !camera_third_person {
        return false;
    }
    if pm_type == VIEW_ORG_BOB_GATE_PM_TYPE_A || pm_type == VIEW_ORG_BOB_GATE_PM_TYPE_B {
        return false;
    }
    if (other_flags & VIEW_ORG_BOB_GATE_OTHER_FLAGS) != 0 {
        return false;
    }
    if (link_flags & VIEW_ORG_BOB_GATE_LINK_FLAGS) != 0 {
        return false;
    }
    if (e_flags & VIEW_ORG_BOB_GATE_EFLAGS) != 0 {
        return false;
    }

    if overlay_reticle != 0 && f_weapon_pos_frac > 0.0 && f_weapon_pos_frac == 1.0 {
        return false;
    }
    true
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct ViewAngleBob {
    pub pitch: f32,

    pub yaw: f32,

    pub roll: f32,

    pub cam_idle_pitch: f32,
    pub cam_idle_yaw: f32,

    pub weap_idle_time: i32,

    pub view_last_idle_factor: f32,
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct ViewAngleBobInputs {
    pub org: ViewOrgBobInputs,

    pub e_flags: u32,

    pub overlay_reticle: i32,

    pub ads_bob_factor_at_0x330: f32,

    pub ads_view_bob_mult_at_0x334: f32,

    pub time: i32,

    pub damage_time: i32,

    pub v_dmg_pitch: f32,

    pub v_dmg_roll: f32,

    pub aim_down_sight: bool,

    pub idle: WeaponIdleInputs,

    pub frametime: f32,

    pub hold_breath_scale: f32,

    pub weap_idle_time: i32,

    pub view_last_idle_factor: f32,
}

#[inline]
fn overlay_view_bob_cycle(bob_cycle: u8) -> f32 {
    bg_view_bob_cycle(bob_cycle) + f64_as_f32(TAU_F64) + WEAPON_BOB_LAG * f64_as_f32(PI_F64)
}

fn org_with_speed(org: ViewOrgBobInputs, xyspeed: f32) -> ViewOrgBobInputs {
    ViewOrgBobInputs { xyspeed, ..org }
}

fn bg_view_overlay_bob_angles(angles: &mut ViewAngleBob, inputs: ViewAngleBobInputs) {
    if inputs.overlay_reticle == 0 {
        return;
    }
    let cycle = overlay_view_bob_cycle(inputs.org.bob_cycle);
    let speed = WEAPON_BOB_AMPLITUDE_BASE * inputs.org.xyspeed;
    let scaled = org_with_speed(inputs.org, speed);
    let mut pitch = bg_calc_view_bob_pitch(cycle, scaled) * OVERLAY_BOB_SIGN;
    let mut yaw = bg_calc_view_bob_roll(cycle, scaled) * OVERLAY_BOB_SIGN;
    let mut roll = bg_calc_view_bob_roll(
        cycle - WEAPON_BOB_UP_PHASE,
        org_with_speed(inputs.org, WEAPON_BOB_AMPLITUDE_ROLL * speed),
    );

    if (roll < 0.0) == (roll == 0.0) {
        roll = 0.0;
    }
    let frac = inputs.org.weapon_pos_frac;
    if frac != 0.0 {
        let atten = weapon_bob_ads_attenuation(frac, inputs.ads_bob_factor_at_0x330);
        pitch *= atten;
        yaw *= atten;
        roll *= atten;
    }
    angles.pitch += frac * pitch;
    angles.yaw += frac * yaw;
    angles.roll += frac * roll;
}

fn bg_view_ads_bob_angles(angles: &mut ViewAngleBob, inputs: ViewAngleBobInputs) {
    let frac = inputs.org.weapon_pos_frac;
    if !(frac > 0.0) {
        return;
    }
    if (inputs.e_flags & EFLAGS_TURRET_VEHICLE) != 0 {
        return;
    }
    if !(inputs.ads_view_bob_mult_at_0x334 > 0.0) {
        return;
    }
    let scale = inputs.ads_view_bob_mult_at_0x334 * frac;
    let cycle = bg_view_bob_cycle(inputs.org.bob_cycle);
    angles.pitch -= bg_calc_view_bob_pitch(cycle, inputs.org) * scale;
    angles.yaw -= bg_calc_view_bob_roll(cycle, inputs.org) * scale;
}

pub fn bg_view_kick_amplitude(damage_count: i32) -> f32 {
    let scaled = damage_count as f32 * BG_VIEW_KICK_SCALE;
    if scaled < BG_VIEW_KICK_MIN {
        BG_VIEW_KICK_MIN
    } else if scaled > BG_VIEW_KICK_MAX {
        BG_VIEW_KICK_MAX
    } else {
        scaled
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct ViewDamageFeedback {
    pub v_dmg_pitch: f32,
    pub v_dmg_roll: f32,
}

pub fn cg_damage_feedback_kick(
    yaw_byte: u32,
    pitch_byte: u32,
    damage_count: i32,
    viewangles: [f32; 3],
) -> ViewDamageFeedback {
    let kick = bg_view_kick_amplitude(damage_count);
    if yaw_byte == VIEW_DAMAGE_UNDIRECTED && pitch_byte == VIEW_DAMAGE_UNDIRECTED {
        return ViewDamageFeedback {
            v_dmg_pitch: -kick,
            v_dmg_roll: 0.0,
        };
    }
    let pitch = (pitch_byte as f32 / VIEW_DAMAGE_BYTE) * VIEW_DAMAGE_TURN;
    let yaw = (yaw_byte as f32 / VIEW_DAMAGE_BYTE) * VIEW_DAMAGE_TURN;
    let (dir, _, _) = angle_vectors([pitch, yaw, 0.0]);
    let (forward, right, _) = angle_vectors(viewangles);
    let side = dir[0] * right[0] + dir[1] * right[1] + dir[2] * right[2];
    let fwd = dir[0] * forward[0] + dir[1] * forward[1] + dir[2] * forward[2];
    ViewDamageFeedback {
        v_dmg_pitch: kick * fwd,
        v_dmg_roll: -kick * side,
    }
}

fn bg_view_damage_kick(angles: &mut ViewAngleBob, inputs: ViewAngleBobInputs) {
    if inputs.damage_time == 0 {
        return;
    }
    let frac = inputs.org.weapon_pos_frac;
    let mut factor = 1.0 - frac * VIEW_DAMAGE_ADS_HALF;
    if frac != 0.0 && inputs.overlay_reticle != 0 {
        factor = (frac * VIEW_DAMAGE_ADS_HALF + 1.0) * factor;
    }
    let delta = inputs.time.wrapping_sub(inputs.damage_time) as f32;
    if delta < VIEW_DAMAGE_DEFLECT_MS {
        let s = get_lean_fraction(delta / VIEW_DAMAGE_DEFLECT_MS);
        let scale = s * factor;
        angles.pitch += scale * inputs.v_dmg_pitch;
        angles.roll += scale * inputs.v_dmg_roll;
        return;
    }
    let f = 1.0 - (delta - VIEW_DAMAGE_DEFLECT_MS) / VIEW_DAMAGE_RETURN_MS;
    if f > 0.0 {
        let s = get_lean_fraction(1.0 - f);
        let scale = (1.0 - s) * factor;
        angles.pitch += scale * inputs.v_dmg_pitch;
        angles.roll += scale * inputs.v_dmg_roll;
    }
}

pub fn bg_view_damage_angles(inputs: ViewAngleBobInputs) -> ViewAngleBob {
    let mut angles = ViewAngleBob::default();
    bg_view_damage_kick(&mut angles, inputs);
    angles
}

fn bg_view_camera_idle(angles: &mut ViewAngleBob, inputs: ViewAngleBobInputs) {
    if inputs.overlay_reticle == 0 || inputs.org.weapon_pos_frac == 0.0 {
        return;
    }
    let ps = WeaponPlacementPsInputs {
        e_flags: inputs.e_flags,
        weapon_pos_frac: inputs.org.weapon_pos_frac,
        aim_down_sight: inputs.aim_down_sight,
        overlay_reticle: inputs.overlay_reticle,
        ..WeaponPlacementPsInputs::default()
    };
    let (amount, speed) = bg_weapon_idle_amount_speed(ps, inputs.idle);
    let add = libm::roundf(speed * WEAPON_IDLE_TIME_MS_SCALE * inputs.frametime) as i32;
    angles.weap_idle_time = angles.weap_idle_time.wrapping_add(add);

    let target = if inputs.e_flags & eflags::PRONE != 0 {
        inputs.idle.idle_prone_factor_at_0x380
    } else if inputs.e_flags & eflags::DUCK != 0 {
        inputs.idle.idle_crouch_factor_at_0x37c
    } else {
        1.0
    };
    let last = angles.view_last_idle_factor;
    if last != target {
        let step = inputs.frametime * WEAPON_IDLE_FACTOR_LERP;
        angles.view_last_idle_factor = if target <= last {
            let next = last - step;
            if next < target { target } else { next }
        } else {
            let next = last + step;
            if next > target { target } else { next }
        };
    }
    let scale = angles.view_last_idle_factor
        * amount
        * inputs.org.weapon_pos_frac
        * inputs.hold_breath_scale
        * WEAPON_IDLE_SIN_SCALE;
    let t = angles.weap_idle_time as f32;
    let yaw = libm::sinf(t * WEAPON_IDLE_YAW_FREQ) * scale;
    let pitch = libm::sinf(t * WEAPON_IDLE_PITCH_FREQ) * scale;
    angles.cam_idle_yaw = yaw;
    angles.cam_idle_pitch = pitch;
    angles.yaw += yaw;
    angles.pitch += pitch;
}

pub fn bg_view_angle_bob(inputs: ViewAngleBobInputs) -> ViewAngleBob {
    let mut angles = ViewAngleBob {
        weap_idle_time: inputs.weap_idle_time,
        view_last_idle_factor: inputs.view_last_idle_factor,
        ..ViewAngleBob::default()
    };
    bg_view_damage_kick(&mut angles, inputs);
    bg_view_camera_idle(&mut angles, inputs);
    bg_view_overlay_bob_angles(&mut angles, inputs);
    bg_view_ads_bob_angles(&mut angles, inputs);
    angles
}
