use playerstate_iw4::{PlayerState, UserCmd};

use crate::stance_speed_scale;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CmdScaleWalkContext {
    pub player_back_speed_scale: f32,

    pub player_strafe_speed_scale: f32,

    pub player_sprint_speed_scale: f32,

    pub player_last_stand_crawl_speed_scale: f32,

    pub weapon_move_speed_scale: f32,

    pub weapon_ads_move_speed_scale: f32,

    pub shellshock_affects_movement: bool,
}

#[must_use]
pub fn pm_cmd_scale_walk(ps: &PlayerState, cmd: &UserCmd, context: CmdScaleWalkContext) -> f32 {
    let flags = ps.pm_flags;
    let sprinting_ads = (flags & 1) != 0 && ps.f_weapon_pos_frac > 0.0;

    let forward = if cmd.forwardmove < 0 {
        (cmd.forwardmove as f32) * context.player_back_speed_scale
    } else {
        cmd.forwardmove as f32
    };
    let forward_abs = forward.abs();
    let right_abs = ((cmd.rightmove as f32) * context.player_strafe_speed_scale).abs();
    let max_component = if right_abs > forward_abs {
        right_abs
    } else {
        forward_abs
    };

    let mut scale = scale_command(
        cmd.forwardmove,
        cmd.rightmove,
        context.player_back_speed_scale,
        context.player_strafe_speed_scale,
        ps.speed as f32,
    );
    if max_component == 0.0 {
        return 0.0;
    }

    if (flags & 0x40) != 0 || ps.leanf != 0.0 || sprinting_ads {
        scale *= 0.4_f32;
    }

    if (flags & 0x4000) != 0 {
        scale *= context.player_sprint_speed_scale;
    }

    scale *= match ps.pm_type {
        2 => 3.0_f32,
        3 => 6.0_f32,
        _ => stance_speed_scale(
            ps,
            cmd.server_time,
            context.player_last_stand_crawl_speed_scale,
        ),
    };

    if ps.weapon != 0 {
        let leaning = (flags & 0x40) != 0;
        let use_move = context.weapon_move_speed_scale > 0.0 && !leaning && !sprinting_ads;
        if use_move {
            scale *= context.weapon_move_speed_scale;
        } else if context.weapon_ads_move_speed_scale > 0.0 {
            scale *= context.weapon_ads_move_speed_scale;
        }
    }

    if (flags & 0x8000) != 0 && context.shellshock_affects_movement {
        scale *= 0.4_f32;
    }

    scale * ps.move_speed_scale_multiplier
}

fn scale_command(
    forward: i8,
    right: i8,
    back_speed_scale: f32,
    strafe_speed_scale: f32,
    speed: f32,
) -> f32 {
    let raw_forward = forward as f32;
    let raw_right = right as f32;
    let scaled_forward = if forward < 0 {
        raw_forward * back_speed_scale
    } else {
        raw_forward
    };
    let forward_abs = scaled_forward.abs();
    let right_abs = (raw_right * strafe_speed_scale).abs();
    let max_component = if right_abs > forward_abs {
        right_abs
    } else {
        forward_abs
    };
    if max_component == 0.0 {
        return 0.0;
    }
    let magnitude = libm::sqrtf((raw_forward * raw_forward) + (raw_right * raw_right));
    (max_component * speed) / (magnitude * 127.0_f32)
}
