use playerstate_iw4::{PlayerState, UserCmd};

use crate::{CollisionBackend, MoveBounds, Pml, pm_accelerate, pm_friction, pm_step_slide_move};

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct AirMoveContext {
    pub player_spectate_speed_scale: f32,

    pub shellshock_gravity_scale: f32,

    pub shellshock_gravity_bias: f32,
}

pub fn pm_air_move<C: CollisionBackend>(
    ps: &mut PlayerState,
    pml: &Pml,
    cmd: &UserCmd,
    context: AirMoveContext,
    bounds: MoveBounds,
    collision: &C,
) {
    pm_friction(ps, pml);

    let command_scale = pm_cmd_scale(ps, cmd, context.player_spectate_speed_scale);
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
    pm_accelerate(ps, pml, &wishdir, wishspeed * command_scale, 1.0);

    if pml.ground_plane != 0 {
        let velocity = ps.velocity;
        clip_velocity(
            &velocity,
            &[
                f32::from_bits(pml.ground_trace[1]),
                f32::from_bits(pml.ground_trace[2]),
                f32::from_bits(pml.ground_trace[3]),
            ],
            &mut ps.velocity,
        );
    }

    let effective_gravity = adjusted_gravity(ps, context);
    pm_step_slide_move(
        ps,
        pml,
        collision,
        bounds.mins,
        bounds.maxs,
        bounds.tracemask,
        Some(effective_gravity),
    );
}

fn pm_cmd_scale(ps: &PlayerState, cmd: &UserCmd, spectate_speed_scale: f32) -> f32 {
    let forward = cmd.forwardmove as f32;
    let right = cmd.rightmove as f32;
    let magnitude = libm::sqrtf(forward * forward + right * right);
    let forward_abs = forward.abs();
    let right_abs = right.abs();
    let largest = if forward_abs > right_abs {
        forward_abs
    } else {
        right_abs
    };
    if largest == 0.0 {
        return 0.0;
    }

    let mut scale = (ps.speed as f32 * largest) / (magnitude * 127.0_f32);
    if (ps.pm_flags & 0x40) != 0 || ps.leanf != 0.0 {
        scale *= 0.4_f32;
    }
    scale *= match ps.pm_type {
        2 => 3.0_f32,
        3 => 6.0_f32,
        5 => spectate_speed_scale,
        _ => 1.0_f32,
    };
    scale
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

fn clip_velocity(input: &[f32; 3], normal: &[f32; 3], output: &mut [f32; 3]) {
    let dot = input[0] * normal[0] + input[1] * normal[1] + input[2] * normal[2];
    let backoff = -(dot - dot.abs() * 0.001_f32);
    output[0] = input[0] + backoff * normal[0];
    output[1] = input[1] + backoff * normal[1];
    output[2] = input[2] + backoff * normal[2];
}

fn adjusted_gravity(ps: &PlayerState, context: AirMoveContext) -> f32 {
    if ((ps.pm_flags & 0x800) != 0 && (ps.pm_flags & 0xffffff00) != 0)
        || ((ps.pm_flags & 0x400000) != 0 && ps.pm_time == 0)
    {
        libm::floorf(
            (ps.gravity as f32) * context.shellshock_gravity_scale
                + context.shellshock_gravity_bias
                + 0.5,
        )
    } else {
        ps.gravity as f32
    }
}
