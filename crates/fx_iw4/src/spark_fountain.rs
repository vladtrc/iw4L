use crate::particle_cloud::{GfxPosTexVertex, fx_particle_spark_cell_indices};
use crate::vec::fx_vec3_normalize;

pub const FX_SPARK_FOUNTAIN_CLUSTER_STRIDE: usize = 0x40;

pub const FX_SPARK_FOUNTAIN_CLUSTER_CAPACITY: u32 = 0x40;

pub const FX_SPARK_FOUNTAIN_MESH_STRIDE: usize = 0x1c00;

pub const FX_SPARK_FOUNTAIN_MESH_CAPACITY: u32 = 0x60;

pub const FX_SPARK_FOUNTAIN_CELLS: u32 = 0x40;

pub const FX_SPARK_FOUNTAIN_KEYFRAME_STRIDE: usize = 0x70;

pub const FX_SPARK_FOUNTAIN_SAMPLES: usize = 4;

pub const FX_SPARK_FOUNTAIN_CELL_OFF_TIMES: usize = 0;

pub const FX_SPARK_FOUNTAIN_CELL_OFF_ORIGIN: usize = 0x10;

pub const FX_SPARK_FOUNTAIN_CELL_OFF_VEL: usize = 0x40;

pub const FX_SPARK_FOUNTAIN_INTEGRATE_CELLS: u32 = 0x10;

pub const FX_SPARK_FOUNTAIN_INTEGRATE_BUDGET: f32 = 5000.0;

pub const FX_SPARK_FOUNTAIN_TIME_SENTINEL: f32 = f32::MAX;

pub const FX_SPARK_FOUNTAIN_TRACE_MASK: u32 = 0x2001;

pub const FX_SPARK_FOUNTAIN_TRACE_BOUNDS: [f32; 3] = [0.0; 3];

pub const FX_SPARK_FOUNTAIN_TRACE_SUBSTEP: f32 = 20.0;

pub const FX_SPARK_FOUNTAIN_TRACE_GROW: f32 = 3.0;

pub const FX_SPARK_FOUNTAIN_TRACE_LOOKAHEAD_VEL: f32 = 10.0;

pub const FX_SPARK_FOUNTAIN_TRACE_LOOKAHEAD_ACCEL: f32 = 50.0;

pub const FX_SPARK_FOUNTAIN_BALLISTIC_HALF: f32 = 0.5;

pub const FX_SPARK_FOUNTAIN_BOOST_HALF_PI: f32 = f64::from_bits(0x3FF9_21FB_6000_0000) as f32;

pub const FX_SPARK_FOUNTAIN_HIT_TIME_EPS: f32 = f32::from_bits(0x2d2f_ebff);

pub const FX_SPARK_FOUNTAIN_HIT_TIME_FOUR: f32 = 4.0;

pub const FX_SPARK_FOUNTAIN_VERTS_PER_SPARK: u32 = 0x200;

pub const FX_SPARK_FOUNTAIN_INDICES_PER_CELL: u32 = 18;

pub const FX_SPARK_FOUNTAIN_HANDLE_NONE: u16 = 0xffff;

pub const FX_SPARK_FOUNTAIN_CLUSTER_OFF_READY: usize = 4;

pub const FX_SPARK_FOUNTAIN_CLUSTER_OFF_SPARK_N: usize = 5;

pub const FX_SPARK_FOUNTAIN_CLUSTER_OFF_WRITE: usize = 6;

pub const FX_SPARK_FOUNTAIN_CLUSTER_OFF_KEYFRAME: usize = 7;

pub const FX_SPARK_FOUNTAIN_CLUSTER_OFF_MESH_IDX: usize = 8;

pub const FX_SPARK_FOUNTAIN_DEF_SIZE: usize = 52;

pub const FX_SPARK_FOUNTAIN_DEF_OFF_SPARK_COUNT: usize = 0x14;

#[inline]
pub fn fx_spark_fountain_handle_for_slot(slot: u32) -> u16 {
    (slot as usize * FX_SPARK_FOUNTAIN_CLUSTER_STRIDE) as u16
}

#[inline]
pub fn fx_spark_fountain_slot_for_handle(handle: u16) -> Option<usize> {
    if handle == FX_SPARK_FOUNTAIN_HANDLE_NONE {
        return None;
    }
    let off = handle as usize;
    if off % FX_SPARK_FOUNTAIN_CLUSTER_STRIDE != 0 {
        return None;
    }
    let slot = off / FX_SPARK_FOUNTAIN_CLUSTER_STRIDE;
    (slot < FX_SPARK_FOUNTAIN_CLUSTER_CAPACITY as usize).then_some(slot)
}

#[inline]
pub fn fx_spark_fountain_reserve_verts(spark_count: u32) -> u32 {
    spark_count << 9
}

#[inline]
pub fn fx_spark_fountain_index_count(cells_emitted: u32) -> u32 {
    cells_emitted * FX_SPARK_FOUNTAIN_INDICES_PER_CELL
}

#[inline]
pub fn fx_spark_fountain_prim_count(cells_emitted: u32) -> u32 {
    fx_spark_fountain_index_count(cells_emitted) / 3
}

#[inline]
pub fn fx_spark_fountain_def_allows_draw(spark_count: i32) -> bool {
    spark_count != 0
}

pub const FX_SPARK_FOUNTAIN_CLUSTER_MESH_MAX: u32 = 0x1c;

pub const FX_ELEM_FLAG_FOUNTAIN_WRITE_SPARK_N: u32 = 0x8000_0000;

pub const FX_SPARK_FOUNTAIN_KEYFRAME_STEP: u8 = 0x10;

pub const FX_SPARK_FOUNTAIN_RAND_CUBE: f32 = 2.0 / 32767.0;

pub const FX_SPARK_FOUNTAIN_CONE_DOT_GATE: f32 = 0.0;

pub const FX_SPARK_FOUNTAIN_CONE_FLIP: f32 = -1.0;

#[inline]
pub fn fx_spark_fountain_draw_clouds_allows(draw_clouds: bool, scale: f32) -> bool {
    draw_clouds && scale != 0.0
}

#[inline]
pub fn fx_spark_fountain_cluster_draw_allows(ready: u8, spark_n: u8, spark_count: i32) -> bool {
    ready != 0 && spark_n != 0 && fx_spark_fountain_def_allows_draw(spark_count)
}

#[inline]
pub fn fx_spark_fountain_mark_ready(spark_n: u8, flags: i32) -> (u8, u8, u8) {
    let write = if (flags as u32 & FX_ELEM_FLAG_FOUNTAIN_WRITE_SPARK_N) != 0 {
        spark_n
    } else {
        0
    };
    (1, write, 0)
}

#[inline]
pub fn fx_spark_fountain_accel_from_gravity(gravity: f32) -> [f32; 3] {
    [0.0, 0.0, gravity]
}

#[inline]
pub fn fx_spark_fountain_integrate_cell_begin(keyframe: u8) -> u32 {
    u32::from(keyframe)
}

pub fn fx_spark_fountain_integrate_miss_cell(
    origin: [f32; 3],
    vel: [f32; 3],
) -> ([f32; 4], [[f32; 3]; 4], [[f32; 3]; 4]) {
    fx_spark_fountain_integrate_cell(
        origin,
        vel,
        [0.0; 3],
        FX_SPARK_FOUNTAIN_INTEGRATE_BUDGET,
        0.0,
        |_s, _e| (1.0, [0.0, 0.0, 1.0]),
    )
}

pub fn fx_spark_fountain_integrate_cell(
    mut origin: [f32; 3],
    mut vel: [f32; 3],
    accel: [f32; 3],
    remaining: f32,
    bounce_frac: f32,
    mut on_trace: impl FnMut([f32; 3], [f32; 3]) -> (f32, [f32; 3]),
) -> ([f32; 4], [[f32; 3]; 4], [[f32; 3]; 4]) {
    let mut times = [FX_SPARK_FOUNTAIN_TIME_SENTINEL; FX_SPARK_FOUNTAIN_SAMPLES];
    let mut origins = [[0.0f32; 3]; FX_SPARK_FOUNTAIN_SAMPLES];
    let mut vels = [[0.0f32; 3]; FX_SPARK_FOUNTAIN_SAMPLES];
    let mut t = 0.0f32;
    let mut live = true;
    let mut i = 0usize;
    while i < FX_SPARK_FOUNTAIN_SAMPLES {
        if live {
            times[i] = t;
            origins[i] = origin;
            vels[i] = vel;
            if i + 1 == FX_SPARK_FOUNTAIN_SAMPLES {
                break;
            }
            match fx_spark_fountain_trace_until_miss_or_hit(
                remaining - t,
                origin,
                vel,
                accel,
                &mut on_trace,
            ) {
                FxSparkFountainTrace::Miss => live = false,
                FxSparkFountainTrace::Hit {
                    time,
                    origin: hit_o,
                    normal,
                    ..
                } => {
                    t += time;
                    origin = hit_o;
                    let v_hit = fx_spark_fountain_vel_at_time(vel, accel, time);
                    vel = fx_spark_fountain_bounce_vel(v_hit, normal, bounce_frac);
                }
            }
        }
        i += 1;
    }
    (times, origins, vels)
}

#[inline]
pub fn fx_spark_fountain_trace_start(origin: [f32; 3], vel: [f32; 3], accel: [f32; 3]) -> [f32; 3] {
    [
        origin[0]
            + vel[0] * FX_SPARK_FOUNTAIN_TRACE_LOOKAHEAD_VEL
            + accel[0] * FX_SPARK_FOUNTAIN_TRACE_LOOKAHEAD_ACCEL,
        origin[1]
            + vel[1] * FX_SPARK_FOUNTAIN_TRACE_LOOKAHEAD_VEL
            + accel[1] * FX_SPARK_FOUNTAIN_TRACE_LOOKAHEAD_ACCEL,
        origin[2]
            + vel[2] * FX_SPARK_FOUNTAIN_TRACE_LOOKAHEAD_VEL
            + accel[2] * FX_SPARK_FOUNTAIN_TRACE_LOOKAHEAD_ACCEL,
    ]
}

#[inline]
pub fn fx_spark_fountain_ballistic(
    origin: [f32; 3],
    vel: [f32; 3],
    accel: [f32; 3],
    dt: f32,
) -> [f32; 3] {
    let h = FX_SPARK_FOUNTAIN_BALLISTIC_HALF * dt * dt;
    [
        origin[0] + vel[0] * dt + accel[0] * h,
        origin[1] + vel[1] * dt + accel[1] * h,
        origin[2] + vel[2] * dt + accel[2] * h,
    ]
}

#[inline]
pub fn fx_spark_fountain_hit_time(a: f32, b: f32, c: f32) -> f32 {
    if FX_SPARK_FOUNTAIN_HIT_TIME_EPS <= libm::fabsf(a) {
        let disc = b * b - a * FX_SPARK_FOUNTAIN_HIT_TIME_FOUR * c;
        if disc < 0.0 {
            return 0.0;
        }
        let root = libm::sqrtf(disc);
        let inv_2a = 1.0 / (a + a);
        let sign = if a < 0.0 {
            FX_SPARK_FOUNTAIN_CONE_FLIP
        } else {
            1.0
        };
        let t0 = inv_2a * (-b - root * sign);
        if 0.0 <= t0 {
            return t0;
        }
        let t1 = (sign * root - b) * inv_2a;
        if 0.0 <= t1 { t1 } else { 0.0 }
    } else if libm::fabsf(b) < FX_SPARK_FOUNTAIN_HIT_TIME_EPS {
        0.0
    } else {
        let t = -c / b;
        if 0.0 <= t { t } else { 0.0 }
    }
}

#[inline]
pub fn fx_spark_fountain_hit_time_abs(
    vel: [f32; 3],
    accel: [f32; 3],
    start: [f32; 3],
    hit: [f32; 3],
    prev_dt: f32,
) -> f32 {
    let a = accel[2] * FX_SPARK_FOUNTAIN_BALLISTIC_HALF;
    let b = accel[2] * prev_dt + vel[2];
    let c = start[2] - hit[2];
    fx_spark_fountain_hit_time(a, b, c) + prev_dt
}

#[inline]
pub fn fx_spark_fountain_vel_at_time(vel: [f32; 3], accel: [f32; 3], t: f32) -> [f32; 3] {
    [
        vel[0] + t * accel[0],
        vel[1] + t * accel[1],
        vel[2] + t * accel[2],
    ]
}

#[inline]
pub fn fx_spark_fountain_bounce_vel(vel: [f32; 3], normal: [f32; 3], bounce_frac: f32) -> [f32; 3] {
    let dot = vel[0] * normal[0] + vel[1] * normal[1] + vel[2] * normal[2];
    let s = (-bounce_frac + -bounce_frac) * dot;
    [
        s * normal[0] + bounce_frac * vel[0],
        s * normal[1] + bounce_frac * vel[1],
        s * normal[2] + bounce_frac * vel[2],
    ]
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum FxSparkFountainTrace {
    Miss,
    Hit {
        fraction: f32,
        time: f32,
        origin: [f32; 3],
        normal: [f32; 3],
    },
}

pub fn fx_spark_fountain_trace_until_miss_or_hit(
    remaining: f32,
    origin: [f32; 3],
    vel: [f32; 3],
    accel: [f32; 3],
    mut on_trace: impl FnMut([f32; 3], [f32; 3]) -> (f32, [f32; 3]),
) -> FxSparkFountainTrace {
    let mut dt = FX_SPARK_FOUNTAIN_TRACE_SUBSTEP;
    let mut prev_dt = FX_SPARK_FOUNTAIN_TRACE_LOOKAHEAD_VEL;
    let mut start = fx_spark_fountain_trace_start(origin, vel, accel);
    loop {
        if remaining <= dt {
            dt = remaining;
        }
        let end = fx_spark_fountain_ballistic(origin, vel, accel, dt);
        let (fraction, normal) = on_trace(start, end);
        if fraction != 1.0 {
            let hit = [
                start[0] + fraction * (end[0] - start[0]),
                start[1] + fraction * (end[1] - start[1]),
                start[2] + fraction * (end[2] - start[2]),
            ];
            let time = fx_spark_fountain_hit_time_abs(vel, accel, start, hit, prev_dt);
            return FxSparkFountainTrace::Hit {
                fraction,
                time,
                origin: hit,
                normal,
            };
        }
        if remaining == dt {
            return FxSparkFountainTrace::Miss;
        }
        prev_dt = dt;
        dt *= FX_SPARK_FOUNTAIN_TRACE_GROW;
        start = end;
    }
}

pub fn fx_spark_fountain_boost(boost_time: f32, boost_factor: f32, age: f32) -> (f32, f32) {
    if !(boost_time > 0.0) {
        return (age, 1.0);
    }
    let k = FX_SPARK_FOUNTAIN_BOOST_HALF_PI / boost_time;
    let (trig_t, leftover, length_scale) = if boost_time <= age {
        (boost_time, age - boost_time, 1.0)
    } else {
        let s = libm::sinf(k * age);
        (age, 0.0, boost_factor * (1.0 - s) + 1.0)
    };
    let c = libm::cosf(k * trig_t);
    let warped = trig_t * (boost_factor + 1.0)
        + (boost_factor * boost_time / FX_SPARK_FOUNTAIN_BOOST_HALF_PI) * (c - 1.0)
        + leftover;
    (warped, length_scale)
}

pub fn fx_spark_fountain_wrap_loop_time(t: f32, loop_time: f32) -> f32 {
    if !(loop_time > 0.0) {
        return t;
    }
    let mut out = t;
    while loop_time < out {
        out -= loop_time;
    }
    out
}

pub fn fx_spark_fountain_same_sample_ribbon(
    origin: [f32; 3],
    vel: [f32; 3],
    gravity: f32,
    t0: f32,
    t_step: f32,
) -> [[f32; 3]; 4] {
    let accel = fx_spark_fountain_accel_from_gravity(gravity);
    let mut out = [[0.0f32; 3]; 4];
    let mut t = t0;
    let mut i = 0usize;
    while i < 4 {
        out[i] = fx_spark_fountain_ballistic(origin, vel, accel, t);
        t += t_step;
        i += 1;
    }
    out
}

pub fn fx_spark_fountain_sample_window(
    times: [f32; 4],
    t0: f32,
    wrapped: f32,
) -> Option<(usize, usize, f32)> {
    let mut i12 = 0usize;
    let mut i13 = 0usize;
    while i12 != 4 {
        i13 = i12;
        if t0 < times[i12] {
            break;
        }
        i12 += 1;
        i13 = i12;
    }
    while i12 != 4 && times[i12] <= wrapped {
        i12 += 1;
    }
    if i13 == 0 {
        if i12 == 0 {
            return None;
        }
        return Some((1, i12, 0.0));
    }
    Some((i13, i12, t0))
}

pub fn fx_spark_fountain_generate_ribbon(
    times: [f32; 4],
    origins: [[f32; 3]; 4],
    vels: [[f32; 3]; 4],
    gravity: f32,
    t0: f32,
    wrapped: f32,
) -> Option<([[f32; 3]; 4], [f32; 4])> {
    let (i13, i12, t0) = fx_spark_fountain_sample_window(times, t0, wrapped)?;
    if i13 == i12 {
        if i12 == 4 {
            let p = origins[3];
            return Some(([p, p, p, p], [0.0; 4]));
        }
        let si = i13 - 1;
        let g = if i13 == 4 { 0.0 } else { gravity };
        let dt0 = t0 - times[si];
        let t_step = (wrapped - t0) / FX_SPARK_FOUNTAIN_TRACE_GROW;
        let ribbon = fx_spark_fountain_same_sample_ribbon(origins[si], vels[si], g, dt0, t_step);
        return Some((ribbon, [0.0; 4]));
    }
    let a = i13 - 1;
    let t_mid = times[i12 - 1];
    let inv_len = 1.0 / (wrapped - t0);
    let dt_step_a = (t_mid - t0) * FX_SPARK_FOUNTAIN_BALLISTIC_HALF;
    let g_a = if i13 != 4 { gravity } else { 0.0 };
    let accel_a = fx_spark_fountain_accel_from_gravity(g_a);
    let mut dt = t0 - times[a];
    let mut uv = 0.0f32;
    let mut ribbon = [[0.0f32; 3]; 4];
    let mut uv_v_lerp = [0.0f32; 4];
    let mut i = 0usize;
    while i < 2 {
        uv_v_lerp[i] = uv;
        ribbon[i] = fx_spark_fountain_ballistic(origins[a], vels[a], accel_a, dt);
        dt += dt_step_a;
        uv += dt_step_a * inv_len;
        i += 1;
    }
    if i12 == 4 {
        let du = (wrapped - t_mid) * inv_len;
        while i < 4 {
            uv_v_lerp[i] = uv;
            ribbon[i] = origins[3];
            uv += du;
            i += 1;
        }
    } else {
        let b = i12 - 1;
        let dt_step_b = wrapped - t_mid;
        let mut dt_b = 0.0f32;
        while i < 4 {
            uv_v_lerp[i] = uv;
            ribbon[i] = fx_spark_fountain_ballistic(origins[b], vels[b], [0.0; 3], dt_b);
            dt_b += dt_step_b;
            uv += dt_step_b * inv_len;
            i += 1;
        }
    }
    Some((ribbon, uv_v_lerp))
}

#[inline]
pub fn fx_spark_fountain_update_keyframe_cursor(
    write_spark: u8,
    keyframe: u8,
    spark_n: u8,
) -> (u8, u8) {
    if write_spark == spark_n {
        return (write_spark, keyframe);
    }
    let next = keyframe.wrapping_add(FX_SPARK_FOUNTAIN_KEYFRAME_STEP);
    if next == 0x40 {
        (write_spark.wrapping_add(1), 0)
    } else {
        (write_spark, next)
    }
}

#[inline]
pub fn fx_spark_fountain_cone_is_isotropic(vel_cone_frac: f32) -> bool {
    vel_cone_frac == 1.0
}

#[inline]
pub fn fx_spark_fountain_isotropic_dir(rand_xyz: [i32; 3]) -> [f32; 3] {
    crate::vec::fx_vec3_normalize([
        rand_xyz[0] as f32 * FX_SPARK_FOUNTAIN_RAND_CUBE - 1.0,
        rand_xyz[1] as f32 * FX_SPARK_FOUNTAIN_RAND_CUBE - 1.0,
        rand_xyz[2] as f32 * FX_SPARK_FOUNTAIN_RAND_CUBE - 1.0,
    ])
}

#[inline]
pub fn fx_spark_fountain_cone_dir(axis: [f32; 3], cube: [f32; 3], vel_cone_frac: f32) -> [f32; 3] {
    let axis = fx_vec3_normalize(axis);
    let cube = fx_vec3_normalize(cube);
    let dot = cube[0] * axis[0] + cube[1] * axis[1] + cube[2] * axis[2];
    let sign = if FX_SPARK_FOUNTAIN_CONE_DOT_GATE <= dot {
        1.0
    } else {
        FX_SPARK_FOUNTAIN_CONE_FLIP
    };
    let keep = 1.0 - vel_cone_frac;
    let mix = vel_cone_frac * sign;
    fx_vec3_normalize([
        keep * axis[0] + mix * cube[0],
        keep * axis[1] + mix * cube[1],
        keep * axis[2] + mix * cube[2],
    ])
}

#[inline]
pub fn fx_spark_fountain_spray_dir(
    axis: [f32; 3],
    rand_xyz: [i32; 3],
    vel_cone_frac: f32,
) -> [f32; 3] {
    let cube = fx_spark_fountain_isotropic_dir(rand_xyz);
    if fx_spark_fountain_cone_is_isotropic(vel_cone_frac) {
        cube
    } else {
        fx_spark_fountain_cone_dir(axis, cube, vel_cone_frac)
    }
}

#[inline]
pub fn fx_spark_fountain_speed(rand: i32, vel_min: f32, vel_max: f32) -> f32 {
    (rand as f32 / crate::particle_cloud::FX_PARTICLE_CLOUD_CRT_RAND_MAX) * (vel_max - vel_min)
        + vel_min
}

#[inline]
pub fn fx_spark_fountain_spark_n_clamped(spark_count: i32) -> u8 {
    if spark_count <= 0 {
        0
    } else {
        spark_count.min(FX_SPARK_FOUNTAIN_CLUSTER_MESH_MAX as i32) as u8
    }
}

#[inline]
pub fn fx_spark_fountain_atlas_uv(col_bits: u8, row_bits: u8, cell: u32) -> [f32; 4] {
    let cols = 1u32 << (col_bits & 0x1f);
    let rows = 1u32 << (row_bits & 0x1f);
    let u_span = 1.0 / cols as f32;
    let v_span = 1.0 / rows as f32;
    let u0 = (cell % cols) as f32 * u_span;
    let v0 = ((cell / cols) % rows) as f32 * v_span;
    [u0, u_span, v0, v_span]
}

pub const R_PARTICLE_CLOUD_CUSTOM_CAP: u32 = 0x40;

#[inline]
pub fn r_add_particle_cloud_custom_allows(live: u32) -> bool {
    live < R_PARTICLE_CLOUD_CUSTOM_CAP
}

#[inline]
pub fn r_reserve_particle_cloud_verts_allows(
    used: u32,
    vert_stride: u32,
    vert_count: u32,
    cap: u32,
) -> bool {
    let Some(delta) = vert_stride.checked_mul(vert_count) else {
        return false;
    };
    let Some(next) = used.checked_add(delta) else {
        return false;
    };
    next <= cap
}

const FX_SPARK_FOUNTAIN_HALF: f32 = 0.5;

const FX_SPARK_FOUNTAIN_SIDE_NUDGE: f32 = 0.01;

#[inline]
fn fx_cross(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}

pub fn fx_spark_fountain_cell_verts(
    ribbon: [[f32; 3]; 4],
    camera: [f32; 3],
    size0: f32,
    uv_u0: f32,
    uv_u_span: f32,
    uv_v0: f32,
    uv_v_span: f32,
    uv_v_lerp: [f32; 4],
) -> [GfxPosTexVertex; 8] {
    let half = size0 * FX_SPARK_FOUNTAIN_HALF;
    let mut side = [[0.0f32; 3]; 3];
    let mut nrm = [[0.0f32; 3]; 3];
    let mut i = 0usize;
    while i < 3 {
        let d = [
            ribbon[i + 1][0] - ribbon[i][0],
            ribbon[i + 1][1] - ribbon[i][1],
            ribbon[i + 1][2] - ribbon[i][2],
        ];
        let d = fx_vec3_normalize(d);
        let to_cam = [
            camera[0] - ribbon[i][0],
            camera[1] - ribbon[i][1],
            camera[2] - ribbon[i][2],
        ];
        let mut s = fx_cross(d, to_cam);
        s[0] += FX_SPARK_FOUNTAIN_SIDE_NUDGE;
        s = fx_vec3_normalize(s);
        let mut n = fx_cross(to_cam, s);
        n = fx_vec3_normalize(n);
        side[i] = [s[0] * half, s[1] * half, s[2] * half];
        nrm[i] = [n[0] * half, n[1] * half, n[2] * half];
        i += 1;
    }
    let u1 = uv_u0 + uv_u_span;
    let v = [
        uv_v0 + uv_v_span * uv_v_lerp[0],
        uv_v0 + uv_v_span * uv_v_lerp[1],
        uv_v0 + uv_v_span * uv_v_lerp[2],
        uv_v0 + uv_v_span * uv_v_lerp[3],
    ];
    let p = ribbon;
    let s0 = side[0];
    let s1 = side[1];
    let s2 = side[2];
    let n0 = nrm[0];
    let n2 = nrm[2];
    let xyz = [
        [
            (p[0][0] - s0[0]) - n0[0],
            (p[0][1] - s0[1]) - n0[1],
            (p[0][2] - s0[2]) - n0[2],
        ],
        [p[1][0] - s1[0], p[1][1] - s1[1], p[1][2] - s1[2]],
        [
            (p[0][0] + s1[0]) - n0[0],
            (p[0][1] + s1[1]) - n0[1],
            (p[0][2] + s1[2]) - n0[2],
        ],
        [p[1][0] + s1[0], p[1][1] + s1[1], p[1][2] + s1[2]],
        [p[2][0] - s1[0], p[2][1] - s1[1], p[2][2] - s1[2]],
        [
            n2[0] + (p[3][0] - s2[0]),
            n2[1] + (p[3][1] - s2[1]),
            n2[2] + (p[3][2] - s2[2]),
        ],
        [p[2][0] + s1[0], p[2][1] + s1[1], p[2][2] + s1[2]],
        [
            n2[0] + p[3][0] + s2[0],
            n2[1] + p[3][1] + s2[1],
            n2[2] + p[3][2] + s2[2],
        ],
    ];
    let uv = [
        [uv_u0, v[0]],
        [uv_u0, v[1]],
        [u1, v[0]],
        [u1, v[1]],
        [uv_u0, v[2]],
        [uv_u0, v[3]],
        [u1, v[2]],
        [u1, v[3]],
    ];
    let mut out = [GfxPosTexVertex {
        xyz: [0.0; 3],
        tex_coord: [0.0; 2],
    }; 8];
    let mut k = 0usize;
    while k < 8 {
        out[k] = GfxPosTexVertex {
            xyz: xyz[k],
            tex_coord: uv[k],
        };
        k += 1;
    }
    out
}

#[inline]
pub fn fx_spark_fountain_cell_indices(cell: u32) -> [u16; 18] {
    fx_particle_spark_cell_indices(cell)
}
