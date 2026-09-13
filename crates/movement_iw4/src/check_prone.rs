use math_iw4::{angle_normalize_360, angle_subtract, yaw_vectors_2d};
use playerstate_iw4::{ENTITYNUM_NONE, PlayerState};
use trace_iw4::Trace;

use crate::{CollisionBackend, GroundTraceInput, StanceSurface, stance_surface_type};

pub const PRONE_CHECK_HEIGHT: f32 = 30.0;

pub const PRONE_FEET_DIST: f32 = 50.0;

const PRONE_TRACE_MASK: u32 = 0x0081_0011;

const PRONE_CAPSULE: f32 = 6.0;

const PRONE_FIRST_LIFT: f32 = 10.0;

const PRONE_YAW_FLIP: f32 = 180.0;

const PRONE_WAIST_ALONG: f32 = 18.0;

const PRONE_SHORT_FEET_FRAC: f32 = 0.699_999_988_079_071;

const PRONE_SHORT_FEET_BUMP: f32 = 22.0;

const PRONE_MIN_FIRST_PAD: f32 = 2.0;

const PRONE_GROUNDED_WAIST_SCALE: f32 = 2.5;

const PRONE_WAIST_FEET_REJECT: f32 = -0.75;

const PRONE_POINT_LIFT: f32 = 5.0;

const PRONE_PITCH_DELTA_MIN: f32 = -50.0;
const PRONE_PITCH_DELTA_MAX: f32 = 70.0;

fn prone_special_air_ok(ps: &PlayerState) -> bool {
    let flags = ps.pm_flags;
    if (flags & 0x800) != 0 {
        return (flags & 0xffff_ff00) != 0;
    }
    (flags & 0x40_0000) != 0 && ps.pm_time == 0
}

#[must_use]
pub fn player_prone_allowed<C: CollisionBackend>(
    ps: &PlayerState,
    collision: &C,
    f_size: f32,
    weapon_blocks_prone: bool,
) -> bool {
    if weapon_blocks_prone {
        return false;
    }
    if (ps.pm_flags & crate::PMF_PRONE) != 0 {
        return true;
    }
    if ps.ground_entity_num == ENTITYNUM_NONE && !prone_special_air_ok(ps) {
        return false;
    }
    bg_check_prone(
        collision,
        ps.origin,
        f_size,
        PRONE_CHECK_HEIGHT,
        ps.viewangles[1],
        false,
        ps.ground_entity_num != ENTITYNUM_NONE,
        true,
        stance_surface_type(ps) != StanceSurface::LastStand,
        PRONE_FEET_DIST,
        None,
        None,
    )
}

#[expect(
    clippy::too_many_arguments,
    reason = "preserves the proven BG_CheckProne call boundary"
)]
#[must_use]
pub fn bg_check_prone<C: CollisionBackend>(
    collision: &C,
    origin: [f32; 3],
    f_size: f32,
    f_height: f32,
    f_yaw: f32,
    already_prone: bool,
    on_ground: bool,
    ground_walkable: bool,
    not_last_stand: bool,
    prone_feet_dist: f32,
    mut torso_pitch: Option<&mut f32>,
    mut waist_pitch: Option<&mut f32>,
) -> bool {
    if !already_prone {
        let lifted = [origin[0], origin[1], origin[2] + PRONE_FIRST_LIFT];
        let hull = collision.trace(GroundTraceInput {
            start: origin,
            end: lifted,
            mins: [-f_size, -f_size, 0.0],
            maxs: [f_size, f_size, f_height],
            tracemask: PRONE_TRACE_MASK,
        });
        if hull.allsolid != 0 {
            return false;
        }
    }
    if on_ground && !ground_walkable {
        return false;
    }

    let yaw = if not_last_stand {
        f_yaw - PRONE_YAW_FLIP
    } else {
        f_yaw
    };
    let (fwd_xy, _) = yaw_vectors_2d(yaw);
    let mut forward = [fwd_xy[0], fwd_xy[1], 0.0];
    let f_trace_height = f_height - PRONE_CAPSULE;
    let mut start = [origin[0], origin[1], origin[2] + f_trace_height];
    let along = prone_feet_dist - PRONE_CAPSULE;
    let mut end = [
        start[0] + along * forward[0],
        start[1] + along * forward[1],
        start[2] + along * forward[2],
    ];
    let cap_mins = [-PRONE_CAPSULE, -PRONE_CAPSULE, -PRONE_CAPSULE];
    let cap_maxs = [PRONE_CAPSULE, PRONE_CAPSULE, PRONE_CAPSULE];
    let mut trace = prone_trace(collision, start, end, cap_mins, cap_maxs);
    let mut first_hit = false;
    let first_dist = if trace.fraction >= 1.0 {
        prone_feet_dist
    } else {
        if !on_ground {
            return false;
        }
        first_hit = true;
        let dist = along * trace.fraction + PRONE_CAPSULE;
        if dist < f_size + PRONE_MIN_FIRST_PAD {
            return false;
        }
        let short = f_trace_height * PRONE_SHORT_FEET_FRAC + PRONE_WAIST_ALONG;
        if dist < short {
            first_hit = false;
            end[2] += PRONE_SHORT_FEET_BUMP;
            let delta = [end[0] - start[0], end[1] - start[1], end[2] - start[2]];
            let pitch_len = vec3_normalize_to(delta, &mut forward);
            trace = prone_trace(collision, start, end, cap_mins, cap_maxs);
            if trace.fraction >= 1.0 {
                prone_feet_dist
            } else {
                first_hit = true;
                let retry = trace.fraction * pitch_len + PRONE_CAPSULE;
                if retry < short {
                    return false;
                }
                retry
            }
        } else {
            dist
        }
    };

    let mut feet = vec3_lerp(start, end, trace.fraction);
    let waist_scale = if on_ground {
        PRONE_GROUNDED_WAIST_SCALE
    } else {
        1.0
    };
    start = [
        origin[0] + PRONE_WAIST_ALONG * forward[0],
        origin[1] + PRONE_WAIST_ALONG * forward[1],
        origin[2] + f_trace_height + PRONE_WAIST_ALONG * forward[2],
    ];
    let down = waist_scale * f_size + f_trace_height - PRONE_CAPSULE;
    end = [start[0], start[1], start[2] - down];
    trace = prone_trace(collision, start, end, cap_mins, cap_maxs);
    if trace.fraction == 1.0 {
        return prone_fail(
            on_ground,
            torso_pitch.as_deref_mut(),
            waist_pitch.as_deref_mut(),
        );
    }
    if trace.walkable == 0 {
        return false;
    }
    let waist_dist = down * trace.fraction + PRONE_CAPSULE;
    let mut waist = vec3_lerp(start, end, trace.fraction);
    waist[2] -= PRONE_CAPSULE;
    if first_hit {
        if first_dist - waist_dist < waist_dist * PRONE_WAIST_FEET_REJECT {
            return prone_fail(
                on_ground,
                torso_pitch.as_deref_mut(),
                waist_pitch.as_deref_mut(),
            );
        }
        let mut delta = [feet[0] - waist[0], feet[1] - waist[1], feet[2] - waist[2]];
        delta[0] += PRONE_CAPSULE * forward[0];
        delta[1] += PRONE_CAPSULE * forward[1];
        delta[2] += PRONE_CAPSULE * forward[2] + PRONE_CAPSULE;
        vec3_normalize(&mut delta);
        let mad_scale = along - PRONE_WAIST_ALONG;
        end = [
            start[0] + mad_scale * delta[0],
            start[1] + mad_scale * delta[1],
            start[2] + mad_scale * delta[2],
        ];
        end[0] = (along * forward[0] + origin[0] + end[0]) * 0.5;
        end[1] = (along * forward[1] + origin[1] + end[1]) * 0.5;
        trace = prone_trace(collision, start, end, cap_mins, cap_maxs);
        if trace.fraction < 1.0 {
            start = vec3_lerp(start, end, trace.fraction);
            start[2] += PRONE_WAIST_ALONG;
            end[2] += PRONE_WAIST_ALONG;
            trace = prone_trace(collision, start, end, cap_mins, cap_maxs);
            if trace.fraction < 1.0 {
                return prone_fail(
                    on_ground,
                    torso_pitch.as_deref_mut(),
                    waist_pitch.as_deref_mut(),
                );
            }
        }
        feet = vec3_lerp(start, end, trace.fraction);
    }

    start = feet;
    end = [
        feet[0],
        feet[1],
        feet[2] - ((feet[2] - waist[2]) + (feet[2] - waist[2]) + f_size),
    ];
    trace = prone_trace(collision, start, end, cap_mins, cap_maxs);
    if trace.fraction == 1.0 {
        return prone_fail(
            on_ground,
            torso_pitch.as_deref_mut(),
            waist_pitch.as_deref_mut(),
        );
    }
    if trace.walkable == 0 {
        return false;
    }
    feet = vec3_lerp(start, end, trace.fraction);
    feet[2] -= PRONE_CAPSULE;
    let torso = origin;
    let torso_delta = if not_last_stand {
        [
            torso[0] - waist[0],
            torso[1] - waist[1],
            torso[2] - waist[2],
        ]
    } else {
        [
            waist[0] - torso[0],
            waist[1] - torso[1],
            waist[2] - torso[2],
        ]
    };
    let f_torso = angle_normalize_360(vec_to_pitch(torso_delta));
    let waist_delta = if not_last_stand {
        [waist[0] - feet[0], waist[1] - feet[1], waist[2] - feet[2]]
    } else {
        [feet[0] - waist[0], feet[1] - waist[1], feet[2] - waist[2]]
    };
    let f_waist = angle_normalize_360(vec_to_pitch(waist_delta));
    let pitch_diff = angle_subtract(f_torso, f_waist);
    let mut success = true;
    if !(PRONE_PITCH_DELTA_MIN..=PRONE_PITCH_DELTA_MAX).contains(&pitch_diff) {
        success = false;
    }
    let zero = [0.0, 0.0, 0.0];
    trace = prone_trace(
        collision,
        [torso[0], torso[1], torso[2] + PRONE_POINT_LIFT],
        [waist[0], waist[1], waist[2] + PRONE_POINT_LIFT],
        zero,
        zero,
    );
    if trace.fraction < 1.0 {
        success = false;
    }
    trace = prone_trace(
        collision,
        [waist[0], waist[1], waist[2] + PRONE_POINT_LIFT],
        [feet[0], feet[1], feet[2] + PRONE_POINT_LIFT],
        zero,
        zero,
    );
    if trace.fraction < 1.0 {
        success = false;
    }
    if let Some(slot) = torso_pitch.as_deref_mut() {
        *slot = f_torso;
    }
    if let Some(slot) = waist_pitch.as_deref_mut() {
        *slot = f_waist;
    }
    if success {
        return true;
    }
    prone_fail(on_ground, torso_pitch, waist_pitch)
}

fn prone_fail(
    on_ground: bool,
    torso_pitch: Option<&mut f32>,
    waist_pitch: Option<&mut f32>,
) -> bool {
    if on_ground {
        return false;
    }
    if let Some(slot) = torso_pitch {
        *slot = 0.0;
    }
    if let Some(slot) = waist_pitch {
        *slot = 0.0;
    }
    true
}

fn prone_trace<C: CollisionBackend>(
    collision: &C,
    start: [f32; 3],
    end: [f32; 3],
    mins: [f32; 3],
    maxs: [f32; 3],
) -> Trace {
    collision.trace(GroundTraceInput {
        start,
        end,
        mins,
        maxs,
        tracemask: PRONE_TRACE_MASK,
    })
}

fn vec3_lerp(start: [f32; 3], end: [f32; 3], frac: f32) -> [f32; 3] {
    [
        start[0] + frac * (end[0] - start[0]),
        start[1] + frac * (end[1] - start[1]),
        start[2] + frac * (end[2] - start[2]),
    ]
}

fn vec3_normalize(v: &mut [f32; 3]) -> f32 {
    let len = libm::sqrtf(v[0] * v[0] + v[1] * v[1] + v[2] * v[2]);
    let div = if len <= 0.0 { 1.0 } else { len };
    let s = 1.0 / div;
    v[0] *= s;
    v[1] *= s;
    v[2] *= s;
    len
}

fn vec3_normalize_to(src: [f32; 3], dst: &mut [f32; 3]) -> f32 {
    *dst = src;
    vec3_normalize(dst)
}

fn vec_to_pitch(vec: [f32; 3]) -> f32 {
    if vec[0] == 0.0 && vec[1] == 0.0 {
        return if -vec[2] < 0.0 { 270.0 } else { 90.0 };
    }
    let xy = libm::sqrtf(vec[0] * vec[0] + vec[1] * vec[1]);
    let pitch = libm::atan2f(vec[2], xy) * (-180.0 / core::f32::consts::PI);
    if pitch < 0.0 { pitch + 360.0 } else { pitch }
}
