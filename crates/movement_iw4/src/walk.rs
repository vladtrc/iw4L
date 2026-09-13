use playerstate_iw4::{PlayerState, UserCmd};

use crate::{
    AirMoveContext, CmdScaleWalkContext, CollisionBackend, JumpCheckContext, JumpCheckResult,
    JumpLaunchContext, MoveBounds, Pml, StanceSurface, jump_check, pm_accelerate, pm_air_move,
    pm_cmd_scale_walk, pm_friction, pm_step_slide_move, stance_surface_type,
};

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct WalkMoveContext {
    pub cmd_scale: CmdScaleWalkContext,

    pub weapon_move_scale: f32,

    pub old_buttons: u32,

    pub jump: JumpLaunchContext,

    pub air: AirMoveContext,
}

#[allow(clippy::assign_op_pattern)]
pub fn pm_walk_move<C: CollisionBackend>(
    ps: &mut PlayerState,
    pml: &mut Pml,
    cmd: &mut UserCmd,
    context: WalkMoveContext,
    bounds: MoveBounds,
    collision: &C,
) {
    if (ps.pm_flags & 0x2000) != 0 {
        prone_velocity_scale(ps);
    }

    let gate = JumpCheckContext {
        time_since_jump: cmd.server_time.wrapping_sub(ps.jump_time),
        old_buttons: context.old_buttons,
        stance_surface_type: stance_surface_type(ps) as u8,
    };
    if let JumpCheckResult::Launched { .. } = jump_check(ps, pml, cmd, gate, context.jump) {
        pm_air_move(ps, pml, cmd, context.air, bounds, collision);
        return;
    }

    pm_friction(ps, pml);

    let command_scale = pm_cmd_scale_walk(ps, cmd, context.cmd_scale)
        * crate::pm_damage_scale_walk(ps.damage_timer);
    crate::pm_walk_move_drop_damage_timer(ps, pml.frametime);
    let mut forward = pml.forward;
    let mut right = pml.right;
    forward[2] = 0.0;
    right[2] = 0.0;
    normalize(&mut forward);
    normalize(&mut right);

    let mut wishdir = [
        (cmd.rightmove as f32) * right[0] + (cmd.forwardmove as f32) * forward[0],
        (cmd.rightmove as f32) * right[1] + (cmd.forwardmove as f32) * forward[1],
        (cmd.rightmove as f32) * right[2] + (cmd.forwardmove as f32) * forward[2],
    ];
    let wishspeed = normalize(&mut wishdir);
    clip_to_ground_plane(&mut wishdir, &pml.ground_trace[1..4]);

    pm_accelerate(
        ps,
        pml,
        &wishdir,
        wishspeed * context.weapon_move_scale * command_scale,
        walk_accel_scale(ps, pml),
    );

    if (pml.ground_trace[4] & 2) != 0 || (ps.pm_flags & 0x100) != 0 {
        ps.velocity[2] -= (ps.gravity as f32) * pml.frametime;
    }

    clip_to_ground_plane(&mut ps.velocity, &pml.ground_trace[1..4]);
    if ps.velocity[0] != 0.0 || ps.velocity[1] != 0.0 {
        pm_step_slide_move(
            ps,
            pml,
            collision,
            bounds.mins,
            bounds.maxs,
            bounds.tracemask,
            None,
        );
    }
}

fn prone_velocity_scale(ps: &mut PlayerState) {
    let mut scale = 1.0_f32;
    if ps.pm_time < 0x709 {
        if ps.pm_time == 0 {
            if ps.jump_origin_z <= ps.origin[2] {
                ps.pm_time = 0x4b0;
                scale = 0.5;
            } else {
                ps.pm_time = 0x708;
                scale = 0.65;
            }
        }
    } else {
        ps.pm_flags &= 0xffbfdfff;
        ps.jump_origin_z = 0.0;
        scale = 0.65;
    }

    if (ps.pm_flags & 0x400000) == 0 {
        ps.velocity[0] *= scale;
        ps.velocity[1] *= scale;
        ps.velocity[2] *= scale;
    }
}

fn normalize(vector: &mut [f32; 3]) -> f32 {
    let length = libm::sqrtf(vector[0] * vector[0] + vector[1] * vector[1] + vector[2] * vector[2]);
    let divisor = if length <= 0.0 { 1.0 } else { length };
    let scale = 1.0 / divisor;
    vector[0] *= scale;
    vector[1] *= scale;
    vector[2] *= scale;
    length
}

fn walk_accel_scale(ps: &PlayerState, pml: &Pml) -> f32 {
    const ACCEL_PRONE: f32 = 19.0;

    const ACCEL_CROUCH: f32 = 12.0;

    const ACCEL_STAND: f32 = 9.0;

    const ACCEL_SLICK: f32 = 1.0;

    const SLOW_WALK_SCALE: f32 = 0.25;

    let slick = (pml.ground_trace[4] & 2) != 0 || (ps.pm_flags & 0x100) != 0;
    let mut accel = if slick {
        ACCEL_SLICK
    } else {
        match stance_surface_type(ps) {
            StanceSurface::Prone | StanceSurface::LastStand => ACCEL_PRONE,
            StanceSurface::Crouch => ACCEL_CROUCH,
            StanceSurface::Stand => ACCEL_STAND,
        }
    };
    if (ps.pm_flags & 0x80) != 0 {
        accel *= SLOW_WALK_SCALE;
    }
    accel
}

fn clip_to_ground_plane(vector: &mut [f32; 3], normal: &[u32]) {
    let normal = [
        f32::from_bits(normal[0]),
        f32::from_bits(normal[1]),
        f32::from_bits(normal[2]),
    ];
    crate::pm_project_velocity(vector, &normal);
}
