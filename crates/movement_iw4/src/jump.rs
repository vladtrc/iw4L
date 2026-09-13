use playerstate_iw4::{ENTITYNUM_NONE, PlayerState, UserCmd};

use crate::{Pml, StanceSurface, add_predictable_event, stance_surface_type};

const BUTTON_JUMP: u32 = 0x400;

const PMF_LADDER: u32 = 0x8;

const PMF_MOVEMENT_TIMER: u32 = 0x2000;

const JUMP_CLEAR_FLAGS: u32 = 0x0040_2000;

const EV_JUMP: i32 = 0x6f;

const LADDER_JUMP_VZ_SCALE: f32 = 0.75;

const LADDER_PUSHOFF_REFLECT: f32 = -2.0;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct JumpCheckContext {
    pub time_since_jump: i32,

    pub old_buttons: u32,

    pub stance_surface_type: u8,
}

pub fn jump_clear_state(ps: &mut PlayerState) {
    ps.pm_flags &= !JUMP_CLEAR_FLAGS;
    ps.jump_origin_z = 0.0;
}

const JUMP_GET_STEP_HEIGHT: f32 = 39.0;

const JUMP_GET_STEP_SIZE: f32 = 18.0;

pub fn jump_get_step_height(ps: &PlayerState, origin: [f32; 3]) -> Option<f32> {
    let ceiling = ps.jump_origin_z + JUMP_GET_STEP_HEIGHT;
    if ceiling <= origin[2] {
        return None;
    }
    let mut step = JUMP_GET_STEP_SIZE;
    if ceiling < origin[2] + JUMP_GET_STEP_SIZE {
        step = ceiling - origin[2];
    }
    Some(step)
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum JumpCheckResult {
    NotEligible,

    HeldJumpCleared,

    Ready,

    Launched { animation: JumpAnimation },
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct JumpLaunchContext {
    pub jump_height: f32,

    pub dive: bool,

    pub crouch_jump_scale: f32,

    pub jump_ladder_push_vel: f32,
}

pub fn pm_jump_start(
    ps: &mut PlayerState,
    pml: &mut Pml,
    cmd: &UserCmd,
    context: JumpLaunchContext,
) {
    let mut energy = (ps.gravity as f32) * (context.jump_height + context.jump_height);
    if (ps.pm_flags & PMF_MOVEMENT_TIMER) != 0 && ps.pm_time <= 0x708 && !context.dive {
        energy /= context.crouch_jump_scale;
    }

    pml.walking = 0;
    pml.ground_plane = 0;
    pml.almost_ground_plane = 0;

    ps.ground_entity_num = ENTITYNUM_NONE;
    ps.jump_origin_z = ps.origin[2];
    ps.jump_time = cmd.server_time;
    ps.velocity[2] = libm::sqrtf(energy);

    let flags = ps.pm_flags;
    ps.pm_time = 0;
    ps.sprint_button_up_required = 0;

    let spread = ps.aim_spread_scale + 64.0;
    ps.aim_spread_scale = if spread > 255.0 { 255.0 } else { spread };

    ps.pm_flags = if context.dive {
        (flags & 0xffff_fe7f) | 0x0040_2000
    } else {
        (flags & 0xffbf_fe7f) | PMF_MOVEMENT_TIMER
    };
}

pub fn pm_ground_surface_type(surface_flags: u32) -> i32 {
    if (surface_flags & 0x2000) != 0 {
        return 0;
    }
    crate::surface_type_index(surface_flags) as i32
}

pub fn pm_jump_event(ps: &mut PlayerState, surface_flags: u32) {
    if (ps.pm_flags & PMF_LADDER) != 0 {
        add_predictable_event(ps, EV_JUMP, 0x15);
        return;
    }
    let parm = pm_ground_surface_type(surface_flags);
    if parm != 0 {
        add_predictable_event(ps, EV_JUMP, parm);
    }
}

pub fn pm_jump_push_off_ladder(ps: &mut PlayerState, pml: &Pml, push_vel: f32) {
    debug_assert!((ps.pm_flags & PMF_LADDER) != 0);
    ps.velocity[2] *= LADDER_JUMP_VZ_SCALE;

    let mut flat_forward = [pml.forward[0], pml.forward[1], 0.0];
    normalize_inplace(&mut flat_forward);

    let ladder = ps.v_ladder_vec;
    let facing_dot =
        ladder[0] * pml.forward[0] + ladder[1] * pml.forward[1] + ladder[2] * pml.forward[2];

    let push = if facing_dot >= 0.0 {
        flat_forward
    } else {
        let along =
            flat_forward[0] * ladder[0] + flat_forward[1] * ladder[1] + flat_forward[2] * ladder[2];
        let scale = along * LADDER_PUSHOFF_REFLECT;
        let mut dir = [
            flat_forward[0] + scale * ladder[0],
            flat_forward[1] + scale * ladder[1],
            flat_forward[2] + scale * ladder[2],
        ];
        normalize_inplace(&mut dir);
        dir
    };

    ps.velocity[0] = push_vel * push[0];
    ps.velocity[1] = push_vel * push[1];
    ps.pm_flags &= !PMF_LADDER;
}

fn normalize_inplace(v: &mut [f32; 3]) {
    let len = libm::sqrtf(v[0] * v[0] + v[1] * v[1] + v[2] * v[2]);
    if len > 0.0 {
        v[0] /= len;
        v[1] /= len;
        v[2] /= len;
    }
}

pub fn jump_check(
    ps: &mut PlayerState,
    pml: &mut Pml,
    cmd: &mut UserCmd,
    gate: JumpCheckContext,
    launch: JumpLaunchContext,
) -> JumpCheckResult {
    let result = jump_check_gate(ps, cmd, gate);
    if result != JumpCheckResult::Ready {
        return result;
    }

    pm_jump_start(ps, pml, cmd, launch);
    pm_jump_event(ps, pml.ground_trace[4]);
    if (ps.pm_flags & PMF_LADDER) != 0 {
        pm_jump_push_off_ladder(ps, pml, launch.jump_ladder_push_vel);
    }

    JumpCheckResult::Launched {
        animation: if cmd.forwardmove >= 0 {
            JumpAnimation::Forward
        } else {
            JumpAnimation::Backward
        },
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum JumpAnimation {
    Forward,

    Backward,
}

pub fn jump_check_gate(
    ps: &PlayerState,
    cmd: &mut UserCmd,
    context: JumpCheckContext,
) -> JumpCheckResult {
    evaluate_gate(ps.pm_flags, ps.pm_type, cmd, context)
}

#[must_use]
pub fn jump_stance_allows(ps: &PlayerState) -> bool {
    stance_surface_type(ps) == StanceSurface::Stand
}

fn evaluate_gate(
    pm_flags: u32,
    pm_type: i32,
    cmd: &mut UserCmd,
    context: JumpCheckContext,
) -> JumpCheckResult {
    if (pm_flags & 0x40000) != 0
        || context.time_since_jump <= 499
        || (pm_flags & 0x400) != 0
        || (pm_flags & 4) != 0
        || pm_type >= 8
    {
        return JumpCheckResult::NotEligible;
    }

    if context.stance_surface_type != 0 || (cmd.buttons & BUTTON_JUMP) == 0 {
        return JumpCheckResult::NotEligible;
    }

    if (context.old_buttons & BUTTON_JUMP) != 0 {
        cmd.buttons &= !BUTTON_JUMP;
        JumpCheckResult::HeldJumpCleared
    } else {
        JumpCheckResult::Ready
    }
}
