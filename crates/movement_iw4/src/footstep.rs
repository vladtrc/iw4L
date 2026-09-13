use playerstate_iw4::{ENTITYNUM_NONE, PlayerState};

use crate::{
    ANIM_MT_FLINCH_FORWARD, CmdScaleWalkContext, LADDER_JUMP_BLOCK_MS,
    PLAYER_DMGTIMER_FLINCH_TIME_MS, PLAYER_DMGTIMER_STUMBLE_TIME_MS, PMF_CROUCH, PMF_LADDER,
    PMF_PRONE, add_predictable_event, pm_damage_window_open, stance_speed_scale,
};

pub const SURFACE_TYPE_NAMES: [&str; 31] = [
    "default",
    "bark",
    "brick",
    "carpet",
    "cloth",
    "concrete",
    "dirt",
    "flesh",
    "foliage",
    "glass",
    "grass",
    "gravel",
    "ice",
    "metal",
    "mud",
    "paper",
    "plaster",
    "rock",
    "sand",
    "snow",
    "water",
    "wood",
    "asphalt",
    "ceramic",
    "plastic",
    "rubber",
    "cushion",
    "fruit",
    "paintedmetal",
    "riotshield",
    "slush",
];

pub const LADDER_SURFACE_TYPE: u32 = 13;
pub const LADDER_SURFACE_FLAGS: u32 = LADDER_SURFACE_TYPE << 20;

#[inline]
pub fn surface_type_index(surface_flags: u32) -> usize {
    ((surface_flags >> 20) & 0x1f) as usize
}

pub fn surface_type_to_name(index: usize) -> &'static str {
    if (1..=30).contains(&index) {
        SURFACE_TYPE_NAMES[index]
    } else {
        "default"
    }
}

pub fn surface_type_name(surface_flags: u32) -> &'static str {
    surface_type_to_name(surface_type_index(surface_flags))
}

#[inline]
pub fn bob_cycle_wrapped(old: u8, new: u8) -> bool {
    ((old.wrapping_add(0x40) ^ new.wrapping_add(0x40)) as i8) < 0
}

const EV_FOOTSTEP_SPRINT: i32 = 0x6b;
const EV_FOOTSTEP_RUN: i32 = 0x6c;
const EV_FOOTSTEP_WALK: i32 = 0x6d;
const EV_FOOTSTEP_PRONE: i32 = 0x6e;

const SURF_NOSTEPS: u32 = 0x2000;

const LADDER_CLIMB_REF: f32 = 95.25;

const LADDER_CLIMB_BOB: f32 = 0.45;

const LADDER_STRAFE_REF: f32 = 38.1;

const LADDER_STRAFE_BOB: f32 = 0.35;

const BOB_FACTOR_TABLE: [[f32; 2]; 6] = [
    [0.335, 0.305],
    [0.25, 0.24],
    [0.34, 0.315],
    [0.25, 0.24],
    [0.36, 0.325],
    [0.25, 0.24],
];

const PLAYER_MOVE_THRESHHOLD: f32 = 10.0;

const PLAYER_SPRINT_CAMERA_BOB: f32 = 0.5;

const PMF_WALKING: u32 = 0x40;

const EFLAGS_TURRET: u32 = 0xc00;

const PMF_BACKWARDS_RUN: u32 = 0x20;

const ANIM_MT_IDLE: u8 = 1;
const ANIM_MT_IDLECR: u8 = 2;
const ANIM_MT_IDLEPRONE: u8 = 3;
const ANIM_MT_SPRINT: u8 = 20;
const ANIM_MT_STUMBLE_SPRINT_FORWARD: u8 = 42;
const ANIM_MT_IDLELASTSTAND: u8 = 51;

const PM_MOVE_ANIM_TABLE: [u8; 32] = [
    10, 36, 4, 38, 8, 8, 8, 8, 12, 40, 6, 40, 52, 52, 52, 52, 11, 37, 5, 39, 9, 9, 9, 9, 13, 41, 7,
    41, 53, 53, 53, 53,
];

pub fn pm_get_bob_max_speed(
    ps: &PlayerState,
    forwardmove: i8,
    rightmove: i8,
    walking: bool,
    sprinting: bool,
    server_time: i32,
    scales: CmdScaleWalkContext,
) -> f32 {
    let mut max_speed = ps.speed as f32;

    const STRAFE_BLEND: f32 = 0.75;
    const DIAG_SCALE: f32 = 0.5;
    let strafe = scales.player_strafe_speed_scale;
    let back = scales.player_back_speed_scale;
    if forwardmove == 0 {
        if rightmove != 0 {
            max_speed *= (strafe - 1.0) * STRAFE_BLEND + 1.0;
        }
    } else if rightmove == 0 {
        if forwardmove < 0 {
            max_speed *= back;
        }
    } else {
        max_speed *= ((strafe - 1.0) * STRAFE_BLEND + 1.0 + 1.0) * DIAG_SCALE;
        if forwardmove < 0 {
            max_speed = DIAG_SCALE * (back + 1.0) * max_speed;
        }
    }
    if walking {
        max_speed *= 0.4;
    } else if sprinting {
        max_speed *= scales.player_sprint_speed_scale;
    }
    if ps.weapon != 0 {
        let lean_flag = (ps.pm_flags & PMF_WALKING) != 0;
        if scales.weapon_move_speed_scale > 0.0 && !lean_flag {
            max_speed *= scales.weapon_move_speed_scale;
        } else if scales.weapon_ads_move_speed_scale > 0.0 {
            max_speed *= scales.weapon_ads_move_speed_scale;
        }
    }
    max_speed * stance_speed_scale(ps, server_time, scales.player_last_stand_crawl_speed_scale)
}

pub fn pm_footsteps_bob_cycle(
    ps: &mut PlayerState,
    msec: i32,
    forwardmove: i8,
    rightmove: i8,
    almost_ground_plane: bool,
    server_time: i32,
    scales: CmdScaleWalkContext,
) {
    if ps.pm_type >= 8 {
        return;
    }
    let xyspeed = libm::sqrtf(ps.velocity[0] * ps.velocity[0] + ps.velocity[1] * ps.velocity[1]);
    if (ps.e_flags & EFLAGS_TURRET) != 0 {
        return;
    }
    let airborne = ps.ground_entity_num == ENTITYNUM_NONE
        && ps.pm_type != 1
        && ps.pm_type != 7
        && !almost_ground_plane;
    if airborne {
        return;
    }
    if xyspeed <= PLAYER_MOVE_THRESHHOLD || ps.pm_type == 1 {
        return;
    }
    if forwardmove == 0 && rightmove == 0 {
        return;
    }

    let stance = match ps.view_height_target {
        0x16 => 3,
        0x28 => 2,
        0x0b => 1,
        _ => 0,
    };
    let walking = (ps.pm_flags & PMF_WALKING) != 0 || ps.leanf != 0.0;
    let sprinting = (ps.pm_flags & crate::PMF_SPRINTING) != 0;
    let bob_factor = if stance == 0 && sprinting {
        PLAYER_SPRINT_CAMERA_BOB
    } else {
        BOB_FACTOR_TABLE[stance][usize::from(walking)]
    };
    let max_speed = pm_get_bob_max_speed(
        ps,
        forwardmove,
        rightmove,
        walking,
        sprinting,
        server_time,
        scales,
    );
    if max_speed <= 0.0 {
        return;
    }
    let bobmove = (xyspeed / max_speed) * bob_factor;
    let old = ps.bob_cycle as u8;
    let new = libm::roundf(old as f32 + msec as f32 * bobmove) as i32 as u8;
    ps.bob_cycle = i32::from(new);
}

pub fn pm_footsteps_anim_move_type(
    ps: &PlayerState,
    forwardmove: i8,
    rightmove: i8,
    almost_ground_plane: bool,
) -> Option<u8> {
    if ps.pm_type >= 8 {
        return None;
    }
    if (ps.e_flags & EFLAGS_TURRET) != 0 {
        return Some(stance_idle_anim(ps.view_height_target));
    }
    let airborne = ps.ground_entity_num == ENTITYNUM_NONE
        && ps.pm_type != 1
        && ps.pm_type != 7
        && !almost_ground_plane;
    if airborne {
        return None;
    }
    let xyspeed = libm::sqrtf(ps.velocity[0] * ps.velocity[0] + ps.velocity[1] * ps.velocity[1]);
    let stance = stance_from_height_target(ps.view_height_target);
    let stumbling = pm_damage_window_open(
        ps.damage_timer,
        ps.damage_duration,
        PLAYER_DMGTIMER_STUMBLE_TIME_MS,
    );
    if xyspeed <= PLAYER_MOVE_THRESHHOLD || ps.pm_type == 1 {
        let flinching = pm_damage_window_open(
            ps.damage_timer,
            ps.damage_duration,
            PLAYER_DMGTIMER_FLINCH_TIME_MS,
        );
        return Some(match stance {
            1 => ANIM_MT_IDLEPRONE,
            2 => ANIM_MT_IDLECR,
            3 => ANIM_MT_IDLELASTSTAND,
            _ if flinching => ANIM_MT_FLINCH_FORWARD.saturating_add(ps.flinch_yaw_anim as u8),
            _ => ANIM_MT_IDLE,
        });
    }
    if forwardmove == 0 && rightmove == 0 {
        return Some(not_trying_to_move_anim(ps));
    }

    let walking = (ps.pm_flags & PMF_WALKING) != 0 || ps.leanf != 0.0;
    let sprinting = (ps.pm_flags & crate::PMF_SPRINTING) != 0;
    let backward = (ps.pm_flags & PMF_BACKWARDS_RUN) != 0;
    if stance == 0 && !backward && sprinting {
        return Some(if stumbling {
            ANIM_MT_STUMBLE_SPRINT_FORWARD
        } else {
            ANIM_MT_SPRINT
        });
    }
    let index = usize::from(stumbling)
        + 2 * usize::from(walking)
        + 4 * usize::from(stance)
        + 16 * usize::from(backward);
    Some(PM_MOVE_ANIM_TABLE[index])
}

fn stance_from_height_target(view_height_target: i32) -> u8 {
    match view_height_target {
        0x0b => 1,
        0x28 => 2,
        0x16 => 3,
        _ => 0,
    }
}

fn stance_idle_anim(view_height_target: i32) -> u8 {
    match stance_from_height_target(view_height_target) {
        1 => ANIM_MT_IDLEPRONE,
        2 => ANIM_MT_IDLECR,
        3 => ANIM_MT_IDLELASTSTAND,
        _ => ANIM_MT_IDLE,
    }
}

fn not_trying_to_move_anim(ps: &PlayerState) -> u8 {
    match ps.view_height_target {
        0x0b => ANIM_MT_IDLEPRONE,
        0x28 => ANIM_MT_IDLECR,
        0x16 => ANIM_MT_IDLELASTSTAND,
        _ => ANIM_MT_IDLE,
    }
}

fn pm_is_sprinting_timestamps(ps: &PlayerState) -> bool {
    ps.last_sprint_start != 0 && ps.last_sprint_end < ps.last_sprint_start
}

pub fn pm_should_make_footsteps(ps: &PlayerState) -> bool {
    match ps.view_height_target {
        0x16 | 0x28 | 0x0b => false,
        _ => (ps.pm_flags & PMF_WALKING) == 0,
    }
}

pub fn pm_footstep_event_type(ps: &PlayerState, surface_flags: u32) -> i32 {
    if (surface_flags & SURF_NOSTEPS) != 0 || surface_type_index(surface_flags) == 0 {
        return 0;
    }
    if (ps.pm_flags & PMF_PRONE) != 0 {
        return EV_FOOTSTEP_PRONE;
    }
    if (ps.pm_flags & (PMF_WALKING | PMF_CROUCH)) == 0 && ps.leanf == 0.0 {
        if pm_is_sprinting_timestamps(ps) {
            return EV_FOOTSTEP_SPRINT;
        }
        return EV_FOOTSTEP_RUN;
    }
    EV_FOOTSTEP_WALK
}

pub fn pm_footstep_event(
    ps: &mut PlayerState,
    old: u8,
    new: u8,
    surface_flags: u32,
    b_footstep: bool,
) -> bool {
    if !bob_cycle_wrapped(old, new) {
        return false;
    }
    if ps.ground_entity_num == ENTITYNUM_NONE {
        return false;
    }
    if !b_footstep {
        return false;
    }
    let event = pm_footstep_event_type(ps, surface_flags);
    if event == 0 {
        return false;
    }
    add_predictable_event(ps, event, surface_type_index(surface_flags) as i32);
    true
}

pub fn pm_ladder_footsteps(ps: &mut PlayerState, msec: i32, server_time: i32) -> bool {
    if (ps.pm_flags & PMF_LADDER) == 0 {
        return false;
    }
    if server_time.wrapping_sub(ps.jump_time) < LADDER_JUMP_BLOCK_MS {
        return false;
    }

    let vz = libm::fabsf(ps.velocity[2]);
    let vxy = libm::sqrtf(ps.velocity[0] * ps.velocity[0] + ps.velocity[1] * ps.velocity[1]);
    let bobmove = if vz >= vxy {
        (vz / LADDER_CLIMB_REF) * LADDER_CLIMB_BOB
    } else {
        (vxy / LADDER_STRAFE_REF) * LADDER_STRAFE_BOB
    };

    let old = ps.bob_cycle as u8;
    let new = libm::roundf(old as f32 + msec as f32 * bobmove) as i32 as u8;
    ps.bob_cycle = i32::from(new);

    if !bob_cycle_wrapped(old, new) {
        return false;
    }
    if ps.ground_entity_num != ENTITYNUM_NONE || (ps.pm_flags & PMF_LADDER) == 0 {
        return false;
    }
    add_predictable_event(ps, EV_FOOTSTEP_RUN, LADDER_SURFACE_TYPE as i32);
    true
}
