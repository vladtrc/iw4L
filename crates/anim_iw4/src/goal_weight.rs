use crate::xanim_time::XANIM_WEIGHT_FLOOR_SCALE;

pub const XANIM_GOAL_WEIGHT_SNAP_EPS: f32 = 0.001;

pub const XANIM_CLIENT_ANIM_CLEAR_BLEND_MS: i32 = 200;

pub const XANIM_CLIENT_ANIM_BLEND_FLOOR_NEW_MS: i32 = 0x78;

pub const XANIM_CLIENT_ANIM_BLEND_FLOOR_NO_DURATION_MS: i32 = 0xaa;

pub const XANIM_CLIENT_ANIM_BLEND_FLOOR_OLD_DURATION_MS: i32 = 0xfa;

pub const XANIM_LEGS_PARENT_WEIGHT_WHEN_TORSO: f32 = 0.01;

pub fn xanim_sanitize_goal_weight(goal_weight: f32, notify_a: i32, notify_b: i32) -> f32 {
    if goal_weight < XANIM_GOAL_WEIGHT_SNAP_EPS {
        if notify_a == 0 && notify_b == 0 {
            0.0
        } else {
            XANIM_GOAL_WEIGHT_SNAP_EPS
        }
    } else {
        goal_weight
    }
}

pub fn xanim_goal_time_from_blend_ms(blend_ms: i32) -> f32 {
    blend_ms as f32 * XANIM_WEIGHT_FLOOR_SCALE
}

pub fn xanim_client_anim_blend_ms(
    new_index: u16,
    authored_blend_ms: i32,
    old_xanim_exists: bool,
    not_legs_slot: bool,
    old_move_speed_nonzero: bool,
    new_move_speed_nonzero: bool,
) -> i32 {
    let mut blend_ms = if new_index == 0 {
        XANIM_CLIENT_ANIM_CLEAR_BLEND_MS
    } else {
        authored_blend_ms
    };
    if old_xanim_exists || not_legs_slot {
        let mut floor = -1;
        if new_index == 0 {
            floor = move_speed_floor(old_xanim_exists, old_move_speed_nonzero);
        } else if blend_ms < 1 {
            if new_move_speed_nonzero {
                floor = XANIM_CLIENT_ANIM_BLEND_FLOOR_NEW_MS;
            } else {
                floor = move_speed_floor(old_xanim_exists, old_move_speed_nonzero);
            }
        }
        if blend_ms < floor {
            blend_ms = floor;
        }
    } else {
        blend_ms = 0;
    }
    blend_ms
}

fn move_speed_floor(old_xanim_exists: bool, old_move_speed_nonzero: bool) -> i32 {
    if !old_xanim_exists || !old_move_speed_nonzero {
        XANIM_CLIENT_ANIM_BLEND_FLOOR_NO_DURATION_MS
    } else {
        XANIM_CLIENT_ANIM_BLEND_FLOOR_OLD_DURATION_MS
    }
}

pub const XANIM_CLIENT_ANIM_RATE_MIN: f32 = f32::from_bits(0x3dcccccd);

pub const XANIM_CLIENT_ANIM_RATE_ZERO_EPS: f32 = 0.01;

pub const XANIM_CLIENT_ANIM_RATE_CAP: f32 = 2.0;

pub const XANIM_CLIENT_ANIM_RATE_LADDER_CAP: f32 = 4.0;

pub const XANIM_CLIENT_ANIM_RATE_SHORT_CAP: f32 = 3.0;

pub const XANIM_CLIENT_ANIM_RATE_LERP_MIN_SPEED: f32 = 20.0;

pub const XANIM_CLIENT_ANIM_RATE_LERP_MAX_SPEED: f32 = 150.0;

pub const XANIM_CLIENT_ANIM_RATE_LERP_SPAN: f32 = 130.0;

pub fn xanim_vec3_distance(a: [f32; 3], b: [f32; 3]) -> f32 {
    let dx = b[0] - a[0];
    let dy = b[1] - a[1];
    let dz = b[2] - a[2];
    libm::sqrtf(dx * dx + dy * dy + dz * dz)
}

pub fn xanim_client_anim_playback_rate(
    origin: [f32; 3],
    last_origin: [f32; 3],
    now_ms: i32,
    last_ms: i32,
    move_speed: f32,
    ladder: bool,
) -> Option<f32> {
    if move_speed == 0.0 || last_ms == 0 {
        return Some(1.0);
    }
    if now_ms == last_ms {
        return None;
    }
    let dt = (now_ms.wrapping_sub(last_ms) as f32) * XANIM_WEIGHT_FLOOR_SCALE;
    let dist = if ladder {
        libm::fabsf(origin[2] - last_origin[2])
    } else {
        xanim_vec3_distance(origin, last_origin)
    };
    let rate = dist / dt / move_speed;
    if rate < XANIM_CLIENT_ANIM_RATE_MIN {
        return Some(if rate >= XANIM_CLIENT_ANIM_RATE_ZERO_EPS || !ladder {
            XANIM_CLIENT_ANIM_RATE_MIN
        } else {
            0.0
        });
    }
    if rate <= XANIM_CLIENT_ANIM_RATE_CAP {
        return Some(rate);
    }
    if ladder {
        return Some(if rate <= XANIM_CLIENT_ANIM_RATE_LADDER_CAP {
            rate
        } else {
            XANIM_CLIENT_ANIM_RATE_LADDER_CAP
        });
    }
    if move_speed <= XANIM_CLIENT_ANIM_RATE_LERP_MAX_SPEED {
        if move_speed >= XANIM_CLIENT_ANIM_RATE_LERP_MIN_SPEED {
            let cap = XANIM_CLIENT_ANIM_RATE_SHORT_CAP
                - (move_speed - XANIM_CLIENT_ANIM_RATE_LERP_MIN_SPEED)
                    / XANIM_CLIENT_ANIM_RATE_LERP_SPAN;
            return Some(if cap < rate { cap } else { rate });
        }
        return Some(if rate <= XANIM_CLIENT_ANIM_RATE_SHORT_CAP {
            rate
        } else {
            XANIM_CLIENT_ANIM_RATE_SHORT_CAP
        });
    }
    Some(if rate <= XANIM_CLIENT_ANIM_RATE_CAP {
        rate
    } else {
        XANIM_CLIENT_ANIM_RATE_CAP
    })
}
