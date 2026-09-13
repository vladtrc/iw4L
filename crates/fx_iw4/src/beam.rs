use crate::vec::{fx_vec3_length_sq, fx_vec3_normalize};

pub const FX_INFINITE_PERSPECTIVE_K: f32 = 2047.0 / 2048.0;

pub const FX_CREATE_CLIP_ZNEAR: f32 = 1.0;

pub const FX_BEAM_CLIP_W_BIAS: f32 = 1.0;

pub const FX_BEAM_CLIP_PLANE_Z1: f32 = -1.0;

pub const FX_BEAM_CLIP_T_MAX: f32 = f32::MAX;

pub const FX_BEAM_CLIP_DZ_AND: u32 = 0;

pub const FX_BEAM_FLAT_DELTA_MIN_LEN_SQ: f32 = 2e-6;

pub const FX_BEAM_VIEWER_SHUFFLE0: u32 = 0x0405_0607;

pub const FX_BEAM_VIEWER_SHUFFLE1: u32 = 0x0809_0a0b;

pub const FX_BEAM_VIEWER_SHUFFLE2: u32 = 0x0001_0203;

pub const FX_BEAM_VIEWER_SHUFFLE3: u32 = 0x0c0d_0e0f;

pub const FX_BEAM_ADD_CAP: usize = 0x30;

pub const FX_BEAM_MAX_SEGMENTS: u8 = 0xff;

pub const FX_TRACER_MIN_DIST: f32 = 5.0;

pub const FX_TRACER_FIRST_PERSON_MAX_WIDTH: f32 = 4.0;

pub const FX_BEAM_WIGGLE: [[f32; 2]; 8] = [
    [0.0, 1.0],
    [0.71, 0.71],
    [1.0, 0.0],
    [0.71, -0.71],
    [0.0, -1.0],
    [-0.71, -0.71],
    [-1.0, 0.0],
    [-0.71, 0.71],
];

#[derive(Clone, Copy, Debug)]
pub struct FxBeamTess {
    pub begin: [f32; 3],
    pub end: [f32; 3],
    pub begin_radius: f32,
    pub end_radius: f32,
    pub colors: [[f32; 4]; 5],
    pub segment_count: u8,
    pub wiggle_dist: f32,
}

#[derive(Clone, Copy, Debug)]
pub struct FxBeamVert {
    pub xyz: [f32; 3],
    pub uv: [f32; 2],
    pub color: [f32; 4],
}

#[inline]
pub const fn fx_beam_vert_count(seg_count: usize) -> usize {
    2 * seg_count + 4
}

#[inline]
pub const fn fx_beam_index_count(seg_count: usize) -> usize {
    3 * (2 * seg_count + 2)
}

#[inline]
pub fn fx_beam_segment_count(length: f32, screw_dist: f32) -> u8 {
    if screw_dist < 1e-5 {
        return 1;
    }
    let n = libm::roundf(length / screw_dist) as i32;
    n.clamp(1, i32::from(FX_BEAM_MAX_SEGMENTS)) as u8
}

#[inline]
pub fn fx_beam_sample_color(colors: &[[f32; 4]; 5], t: f32) -> [f32; 4] {
    let t = t.clamp(0.0, 1.0) * 4.0;
    let i = libm::floorf(t) as usize;
    let j = (i + 1).min(4);
    let f = t - i as f32;
    let a = colors[i.min(4)];
    let b = colors[j];
    [
        a[0] + (b[0] - a[0]) * f,
        a[1] + (b[1] - a[1]) * f,
        a[2] + (b[2] - a[2]) * f,
        a[3] + (b[3] - a[3]) * f,
    ]
}

#[inline]
pub fn fx_beam_color_rgba(color: [f32; 4]) -> [u8; 4] {
    [
        pack_u8(color[0]),
        pack_u8(color[1]),
        pack_u8(color[2]),
        pack_u8(color[3]),
    ]
}

#[inline]
fn pack_u8(c: f32) -> u8 {
    (c.clamp(0.0, 1.0) * 255.0 + 0.5) as u8
}

#[inline]
pub fn fx_mat4_mul_vec4(m: &[f32; 16], v: [f32; 4]) -> [f32; 4] {
    [
        m[0] * v[0] + m[4] * v[1] + m[8] * v[2] + m[12] * v[3],
        m[1] * v[0] + m[5] * v[1] + m[9] * v[2] + m[13] * v[3],
        m[2] * v[0] + m[6] * v[1] + m[10] * v[2] + m[14] * v[3],
        m[3] * v[0] + m[7] * v[1] + m[11] * v[2] + m[15] * v[3],
    ]
}

#[inline]
pub fn fx_mat4_mul(a: &[f32; 16], b: &[f32; 16]) -> [f32; 16] {
    let c0 = fx_mat4_mul_vec4(a, [b[0], b[1], b[2], b[3]]);
    let c1 = fx_mat4_mul_vec4(a, [b[4], b[5], b[6], b[7]]);
    let c2 = fx_mat4_mul_vec4(a, [b[8], b[9], b[10], b[11]]);
    let c3 = fx_mat4_mul_vec4(a, [b[12], b[13], b[14], b[15]]);
    [
        c0[0], c0[1], c0[2], c0[3], c1[0], c1[1], c1[2], c1[3], c2[0], c2[1], c2[2], c2[3], c3[0],
        c3[1], c3[2], c3[3],
    ]
}

#[inline]
pub fn fx_beam_viewer_pshufb(src: [u8; 16]) -> [u8; 16] {
    let ctrls = [
        FX_BEAM_VIEWER_SHUFFLE0,
        FX_BEAM_VIEWER_SHUFFLE1,
        FX_BEAM_VIEWER_SHUFFLE2,
        FX_BEAM_VIEWER_SHUFFLE3,
    ];
    let mut dst = [0u8; 16];
    for (k, ctrl) in ctrls.into_iter().enumerate() {
        dst[4 * k] = src[(ctrl >> 24) as usize];
        dst[4 * k + 1] = src[((ctrl >> 16) & 0xff) as usize];
        dst[4 * k + 2] = src[((ctrl >> 8) & 0xff) as usize];
        dst[4 * k + 3] = src[(ctrl & 0xff) as usize];
    }
    dst
}

pub fn fx_infinite_perspective_matrix(tan_half_x: f32, tan_half_y: f32, z_near: f32) -> [f32; 16] {
    let k = FX_INFINITE_PERSPECTIVE_K;
    [
        k / tan_half_x,
        0.0,
        0.0,
        0.0,
        0.0,
        k / tan_half_y,
        0.0,
        0.0,
        0.0,
        0.0,
        k,
        -z_near * k,
        0.0,
        0.0,
        1.0,
        0.0,
    ]
}

pub fn fx_matrix_for_viewer(origin: [f32; 3], axis: [[f32; 3]; 3]) -> [f32; 16] {
    let right_n = [-axis[1][0], -axis[1][1], -axis[1][2]];
    let up = axis[2];
    let fwd = axis[0];
    let tx = -(origin[0] * right_n[0] + origin[1] * right_n[1] + origin[2] * right_n[2]);
    let ty = -(origin[0] * up[0] + origin[1] * up[1] + origin[2] * up[2]);
    let tz = -(origin[0] * fwd[0] + origin[1] * fwd[1] + origin[2] * fwd[2]);
    [
        right_n[0], up[0], fwd[0], 0.0, right_n[1], up[1], fwd[1], 0.0, right_n[2], up[2], fwd[2],
        0.0, tx, ty, tz, 1.0,
    ]
}

pub fn fx_create_clip_matrix(
    origin: [f32; 3],
    axis: [[f32; 3]; 3],
    tan_half_x: f32,
    tan_half_y: f32,
) -> [f32; 16] {
    let proj = fx_infinite_perspective_matrix(tan_half_x, tan_half_y, FX_CREATE_CLIP_ZNEAR);
    let view = fx_matrix_for_viewer(origin, axis);
    fx_mat4_mul(&proj, &view)
}

pub fn fx_beam_clip_against_plane(
    begin: &mut [f32; 4],
    end: &mut [f32; 4],
    plane: [f32; 4],
) -> bool {
    let d_end = plane[0] * end[0] + plane[1] * end[1] + plane[2] * end[2] + plane[3] * end[3];
    let d_begin =
        plane[0] * begin[0] + plane[1] * begin[1] + plane[2] * begin[2] + plane[3] * begin[3];
    let end_in = d_end >= 0.0;
    let begin_in = d_begin >= 0.0;
    if !end_in && !begin_in {
        return false;
    }
    let denom = d_end - d_begin;
    let t = if denom != 0.0 {
        1.0 / denom
    } else {
        FX_BEAM_CLIP_T_MAX
    };
    let hit = [
        (begin[0] - end[0]) * t * d_end + end[0],
        (begin[1] - end[1]) * t * d_end + end[1],
        (begin[2] - end[2]) * t * d_end + end[2],
        (begin[3] - end[3]) * t * d_end + end[3],
    ];
    if !end_in {
        *end = hit;
    }
    if !begin_in {
        *begin = hit;
    }
    true
}

pub fn fx_beam_clip_pair_z_planes(begin: &mut [f32; 4], end: &mut [f32; 4]) -> bool {
    fx_beam_clip_against_plane(begin, end, [0.0, 0.0, 1.0, 0.0])
        && fx_beam_clip_against_plane(begin, end, [0.0, 0.0, FX_BEAM_CLIP_PLANE_Z1, 1.0])
}

pub fn fx_generate_beam_get_flat_delta(
    clip: &[f32; 16],
    inv_clip: &[f32; 16],
    begin: [f32; 3],
    end: [f32; 3],
) -> Option<[f32; 3]> {
    let mut b = fx_mat4_mul_vec4(clip, [begin[0], begin[1], begin[2], FX_BEAM_CLIP_W_BIAS]);
    let mut e = fx_mat4_mul_vec4(clip, [end[0], end[1], end[2], FX_BEAM_CLIP_W_BIAS]);
    if !fx_beam_clip_pair_z_planes(&mut b, &mut e) {
        return None;
    }
    if b[3] == 0.0 || e[3] == 0.0 {
        return None;
    }
    let bw = 1.0 / b[3];
    let ew = 1.0 / e[3];
    let dx = e[0] * ew - b[0] * bw;
    let dy = e[1] * ew - b[1] * bw;
    let dz = 0.0;
    let world = fx_mat4_mul_vec4(inv_clip, [dx, dy, dz, 0.0]);
    let len_sq =
        world[0] * world[0] + world[1] * world[1] + world[2] * world[2] + world[3] * world[3];
    if len_sq < FX_BEAM_FLAT_DELTA_MIN_LEN_SQ {
        return None;
    }
    let inv = 1.0 / libm::sqrtf(len_sq);
    Some([world[0] * inv, world[1] * inv, world[2] * inv])
}

pub fn fx_beam_perp_from_flat(view_axis: [f32; 3], flat: [f32; 3]) -> Option<[f32; 3]> {
    let p = cross(view_axis, flat);
    if fx_vec3_length_sq(p) == 0.0 {
        return None;
    }
    Some(fx_vec3_normalize(p))
}

pub fn fx_beam_flat_basis(
    begin: [f32; 3],
    end: [f32; 3],
    view_pos: [f32; 3],
    view_fwd: [f32; 3],
) -> Option<([f32; 3], [f32; 3])> {
    let delta = sub(end, begin);
    let len_sq = fx_vec3_length_sq(delta);
    if len_sq < 1e-10 {
        return None;
    }
    let beam_dir = scale(delta, 1.0 / libm::sqrtf(len_sq));
    let mut fwd = fx_vec3_normalize(view_fwd);
    if fx_vec3_length_sq(fwd) < 1e-6 {
        let mid = scale(add(begin, end), 0.5);
        fwd = fx_vec3_normalize(sub(mid, view_pos));
    }
    if fx_vec3_length_sq(fwd) < 1e-6 {
        return None;
    }
    let mut flat = sub(beam_dir, scale(fwd, dot(beam_dir, fwd)));
    if fx_vec3_length_sq(flat) < 1e-8 {
        let mut alt = cross(fwd, [0.0, 1.0, 0.0]);
        if fx_vec3_length_sq(alt) < 1e-8 {
            alt = cross(fwd, [1.0, 0.0, 0.0]);
        }
        flat = fx_vec3_normalize(alt);
    } else {
        flat = fx_vec3_normalize(flat);
    }
    let mut perp = cross(flat, fwd);
    if fx_vec3_length_sq(perp) < 1e-8 {
        perp = cross(beam_dir, fwd);
    }
    if fx_vec3_length_sq(perp) < 1e-8 {
        return None;
    }
    Some((flat, fx_vec3_normalize(perp)))
}

pub fn fx_beam_generate_verts(
    beam: &FxBeamTess,
    view_pos: [f32; 3],
    view_fwd: [f32; 3],
    clip: Option<(&[f32; 16], &[f32; 16])>,
    verts: &mut [FxBeamVert],
    indices: &mut [u16],
) -> Option<(usize, usize)> {
    let seg_count = beam.segment_count.max(1) as usize;
    let n_verts = fx_beam_vert_count(seg_count);
    let n_idx = fx_beam_index_count(seg_count);
    if verts.len() < n_verts || indices.len() < n_idx {
        return None;
    }
    let (flat, perp) = match clip {
        Some((clip_m, inv_m)) => {
            let flat = fx_generate_beam_get_flat_delta(clip_m, inv_m, beam.begin, beam.end)?;
            let perp = fx_beam_perp_from_flat(view_fwd, flat)?;
            (flat, perp)
        }
        None => fx_beam_flat_basis(beam.begin, beam.end, view_pos, view_fwd)?,
    };
    let begin_c = fx_beam_sample_color(&beam.colors, 0.0);
    verts[0] = FxBeamVert {
        xyz: sub(beam.begin, scale(flat, beam.begin_radius)),
        uv: [0.0, 0.5],
        color: begin_c,
    };
    let mut w = 1usize;
    for seg in 0..=seg_count {
        let alpha = seg as f32 / seg_count as f32;
        let wig = FX_BEAM_WIGGLE[seg % 8];
        let wx = beam.wiggle_dist * wig[0];
        let wy = beam.wiggle_dist * wig[1];
        let base = add(
            lerp(beam.begin, beam.end, alpha),
            add(scale(perp, wx), scale(flat, wy)),
        );
        let radius = beam.begin_radius + (beam.end_radius - beam.begin_radius) * alpha;
        let c = fx_beam_sample_color(&beam.colors, alpha);
        verts[w] = FxBeamVert {
            xyz: sub(base, scale(perp, radius)),
            uv: [alpha, 0.0],
            color: c,
        };
        verts[w + 1] = FxBeamVert {
            xyz: add(base, scale(perp, radius)),
            uv: [alpha, 1.0],
            color: c,
        };
        w += 2;
    }
    let end_c = fx_beam_sample_color(&beam.colors, 1.0);
    verts[w] = FxBeamVert {
        xyz: add(beam.end, scale(flat, beam.end_radius)),
        uv: [1.0, 0.5],
        color: end_c,
    };
    let wrote_idx = write_beam_indices(seg_count, indices);
    Some((n_verts, wrote_idx))
}

fn write_beam_indices(seg_count: usize, out: &mut [u16]) -> usize {
    let mut i = 0usize;
    out[i] = 0;
    out[i + 1] = 2;
    out[i + 2] = 1;
    i += 3;
    for seg in 0..seg_count {
        let o = (2 * seg) as u16;
        out[i] = o + 2;
        out[i + 1] = o + 4;
        out[i + 2] = o + 1;
        i += 3;
        out[i] = o + 1;
        out[i + 1] = o + 4;
        out[i + 2] = o + 3;
        i += 3;
    }
    let last_pair = (2 * seg_count) as u16;
    out[i] = last_pair + 1;
    out[i + 1] = last_pair + 2;
    out[i + 2] = last_pair + 3;
    i + 3
}

#[inline]
fn add(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [a[0] + b[0], a[1] + b[1], a[2] + b[2]]
}

#[inline]
fn sub(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}

#[inline]
fn scale(a: [f32; 3], s: f32) -> [f32; 3] {
    [a[0] * s, a[1] * s, a[2] * s]
}

#[inline]
fn dot(a: [f32; 3], b: [f32; 3]) -> f32 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

#[inline]
fn cross(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}

#[inline]
fn lerp(a: [f32; 3], b: [f32; 3], t: f32) -> [f32; 3] {
    add(a, scale(sub(b, a), t))
}
