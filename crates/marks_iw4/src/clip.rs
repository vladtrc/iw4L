use crate::{
    FX_MARK_STAGING_POINT_STRIDE, FX_MARK_STAGING_TRI_STRIDE, GFX_WORLD_VERTEX_STRIDE,
    R_MARK_CHOP_BEHIND, R_MARK_CHOP_MAX_POINTS, R_MARK_CHOP_ON_PLANE, R_MARK_CLIP_PLANE_COUNT,
    R_MARK_FRAGMENTS_WORLD_SURF_STACK, R_MARK_TRI_REJECT_LEN_SQ_SCALE,
};

const _: () = assert!(GFX_WORLD_VERTEX_STRIDE == 0x2c);
const _: () = assert!(GFX_WORLD_VERTEX_STRIDE != 0x20);
const _: () = assert!(core::mem::size_of::<FxMarkStagingTri>() == FX_MARK_STAGING_TRI_STRIDE);
const _: () = assert!(core::mem::size_of::<FxMarkStagingPoint>() == FX_MARK_STAGING_POINT_STRIDE);
const _: () = assert!(FX_MARK_STAGING_TRI_STRIDE != 0x0c);

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FxWorldMarkPoint {
    pub xyz: [f32; 3],
    pub weights: [f32; 3],
}

impl FxWorldMarkPoint {
    pub const ZERO: Self = Self {
        xyz: [0.0; 3],
        weights: [0.0; 3],
    };
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct MarkWorldClipCensus {
    pub tri_seen: u32,

    pub tri_rejected: u32,

    pub clip_zero: u32,

    pub clip_kept: u32,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FxMarkStagingTri {
    pub indices: [u16; 3],

    pub context: [u8; 7],
    pub _pad: u8,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FxMarkStagingPoint {
    pub xyz: [f32; 3],
    pub lmap_coord: [f32; 2],
    pub normal: [f32; 3],
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FxMarkEmitRefuse {
    Overflow,

    FragmentTooSmall,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct MarkWorldStaging {
    pub census: MarkWorldClipCensus,
    pub used_tri: u32,
    pub used_point: u32,
    pub overflow: bool,
}

impl FxMarkStagingTri {
    pub const ZERO: Self = Self {
        indices: [0; 3],
        context: [0; 7],
        _pad: 0,
    };
}

impl FxMarkStagingPoint {
    pub const ZERO: Self = Self {
        xyz: [0.0; 3],
        lmap_coord: [0.0; 2],
        normal: [0.0; 3],
    };
}

pub fn fx_mark_context_from_world_surface(lmap: u8, primary_light: u8, probe: u8) -> [u8; 7] {
    let mut context = [0u8; 7];
    context[1] = lmap;
    context[5] = primary_light;
    context[6] = probe;
    context
}

pub fn fx_mark_context_from_smodel(
    surf_index: u8,
    smodel_index: u16,
    primary_light: u8,
    probe: u8,
) -> [u8; 7] {
    let mut context = [0u8; 7];
    context[0] = surf_index | 0x40;
    context[1] = 0x1f;
    context[2] = smodel_index as u8;
    context[3] = (smodel_index >> 8) as u8;
    context[5] = primary_light;
    context[6] = probe;
    context
}

pub fn fx_mark_context_lmap(context: &[u8; 7]) -> u8 {
    context[1]
}

pub fn fx_mark_context_primary_light(context: &[u8; 7]) -> u8 {
    context[5]
}

pub fn fx_mark_context_probe(context: &[u8; 7]) -> u8 {
    context[6]
}

pub fn fx_mark_fragment_clip_planes(
    origin: [f32; 3],
    axis: [[f32; 3]; 3],
    radius: f32,
) -> [[f32; 4]; R_MARK_CLIP_PLANE_COUNT] {
    let mut planes = [[0.0f32; 4]; R_MARK_CLIP_PLANE_COUNT];
    let mut axis_i = 0;
    while axis_i < 3 {
        let n = axis[axis_i];
        let p = axis_i * 2;
        planes[p][0] = n[0];
        planes[p][1] = n[1];
        planes[p][2] = n[2];
        planes[p][3] = n[0] * origin[0] + n[1] * origin[1] + n[2] * origin[2] - radius;
        planes[p + 1][0] = -n[0];
        planes[p + 1][1] = -n[1];
        planes[p + 1][2] = -n[2];
        planes[p + 1][3] = planes[p + 1][0] * origin[0]
            + planes[p + 1][1] * origin[1]
            + planes[p + 1][2] * origin[2]
            - radius;
        axis_i += 1;
    }
    planes
}

pub fn fx_mark_is_triangle_rejected(
    mark_dir: [f32; 3],
    v0: [f32; 3],
    v1: [f32; 3],
    v2: [f32; 3],
) -> bool {
    let nx = (v2[1] - v0[1]) * (v1[2] - v0[2]) - (v2[2] - v0[2]) * (v1[1] - v0[1]);
    let ny = (v1[0] - v0[0]) * (v2[2] - v0[2]) - (v2[0] - v0[0]) * (v1[2] - v0[2]);
    let nz = (v1[1] - v0[1]) * (v2[0] - v0[0]) - (v2[1] - v0[1]) * (v1[0] - v0[0]);
    let dot = nx * mark_dir[0] + ny * mark_dir[1] + nz * mark_dir[2];
    if dot < 0.0 {
        return true;
    }
    let len_sq = nx * nx + ny * ny + nz * nz;
    len_sq * R_MARK_TRI_REJECT_LEN_SQ_SCALE > dot * dot
}

pub fn fx_mark_setup_world_clip_points(
    v0: [f32; 3],
    v1: [f32; 3],
    v2: [f32; 3],
) -> [FxWorldMarkPoint; 3] {
    [
        FxWorldMarkPoint {
            xyz: v0,
            weights: [1.0, 0.0, 0.0],
        },
        FxWorldMarkPoint {
            xyz: v1,
            weights: [0.0, 1.0, 0.0],
        },
        FxWorldMarkPoint {
            xyz: v2,
            weights: [0.0, 0.0, 1.0],
        },
    ]
}

pub fn fx_mark_chop_world_poly_behind_plane(
    in_count: usize,
    inp: &[FxWorldMarkPoint],
    plane: [f32; 4],
    out: &mut [FxWorldMarkPoint],
) -> usize {
    if in_count == 0 || in_count > R_MARK_CHOP_MAX_POINTS || inp.len() < in_count {
        return 0;
    }
    let mut dists = [0.0f32; R_MARK_CHOP_MAX_POINTS + 1];
    let mut sides = [0i32; R_MARK_CHOP_MAX_POINTS + 1];
    let mut side_count = [0i32; 3];
    let mut i = 0;
    while i < in_count {
        let p = &inp[i];
        let dist = p.xyz[0] * plane[0] + p.xyz[1] * plane[1] + p.xyz[2] * plane[2] - plane[3];
        dists[i] = dist;
        let side = if dist <= R_MARK_CHOP_ON_PLANE {
            if R_MARK_CHOP_BEHIND <= dist { 2 } else { 1 }
        } else {
            0
        };
        sides[i] = side;
        side_count[side as usize] += 1;
        i += 1;
    }
    sides[in_count] = sides[0];
    dists[in_count] = dists[0];
    if side_count[0] == 0 {
        return 0;
    }
    if side_count[1] == 0 {
        let n = core::cmp::min(in_count, out.len());
        let mut j = 0;
        while j < n {
            out[j] = inp[j];
            j += 1;
        }
        return n;
    }
    let mut out_n = 0usize;
    i = 0;
    while i < in_count {
        let side = sides[i];
        if side == 2 {
            if out_n < out.len() {
                out[out_n] = inp[i];
                out_n += 1;
            }
        } else {
            if side == 0 && out_n < out.len() {
                out[out_n] = inp[i];
                out_n += 1;
            }
            let next_side = sides[i + 1];
            if next_side != 2 && next_side != side {
                let denom = dists[i] - dists[i + 1];
                if denom != 0.0 && out_n < out.len() {
                    let t = dists[i] / denom;
                    let a = inp[i];
                    let b = inp[(i + 1) % in_count];
                    out[out_n] = lerp_point(a, b, t);
                    out_n += 1;
                }
            }
        }
        i += 1;
    }
    out_n
}

fn lerp_point(a: FxWorldMarkPoint, b: FxWorldMarkPoint, t: f32) -> FxWorldMarkPoint {
    FxWorldMarkPoint {
        xyz: [
            a.xyz[0] + t * (b.xyz[0] - a.xyz[0]),
            a.xyz[1] + t * (b.xyz[1] - a.xyz[1]),
            a.xyz[2] + t * (b.xyz[2] - a.xyz[2]),
        ],
        weights: [
            a.weights[0] + t * (b.weights[0] - a.weights[0]),
            a.weights[1] + t * (b.weights[1] - a.weights[1]),
            a.weights[2] + t * (b.weights[2] - a.weights[2]),
        ],
    }
}

pub fn fx_mark_clip_world_triangle(
    mark_dir: [f32; 3],
    planes: &[[f32; 4]; R_MARK_CLIP_PLANE_COUNT],
    v0: [f32; 3],
    v1: [f32; 3],
    v2: [f32; 3],
) -> u32 {
    let mut pts = [FxWorldMarkPoint::ZERO; R_MARK_CHOP_MAX_POINTS];
    fx_mark_clip_world_triangle_points(mark_dir, planes, v0, v1, v2, &mut pts)
}

pub fn fx_mark_chop_world_triangle_points(
    planes: &[[f32; 4]; R_MARK_CLIP_PLANE_COUNT],
    v0: [f32; 3],
    v1: [f32; 3],
    v2: [f32; 3],
    out_pts: &mut [FxWorldMarkPoint],
) -> u32 {
    let setup = fx_mark_setup_world_clip_points(v0, v1, v2);
    let mut buf_a = [FxWorldMarkPoint::ZERO; R_MARK_CHOP_MAX_POINTS];
    let mut buf_b = [FxWorldMarkPoint::ZERO; R_MARK_CHOP_MAX_POINTS];
    buf_a[0] = setup[0];
    buf_a[1] = setup[1];
    buf_a[2] = setup[2];
    let mut count = 3usize;
    let mut ping = 0u32;
    let mut plane_i = 0;
    while plane_i < R_MARK_CLIP_PLANE_COUNT {
        let (inp, out) = if ping == 0 {
            (&buf_a[..], &mut buf_b[..])
        } else {
            (&buf_b[..], &mut buf_a[..])
        };
        count = fx_mark_chop_world_poly_behind_plane(count, inp, planes[plane_i], out);
        if count == 0 {
            return 0;
        }
        ping ^= 1;
        plane_i += 1;
    }
    let src = if ping == 0 { &buf_a[..] } else { &buf_b[..] };
    let n = core::cmp::min(count, out_pts.len());
    let mut i = 0;
    while i < n {
        out_pts[i] = src[i];
        i += 1;
    }
    n as u32
}

pub fn fx_mark_clip_world_triangle_points(
    mark_dir: [f32; 3],
    planes: &[[f32; 4]; R_MARK_CLIP_PLANE_COUNT],
    v0: [f32; 3],
    v1: [f32; 3],
    v2: [f32; 3],
    out_pts: &mut [FxWorldMarkPoint],
) -> u32 {
    if fx_mark_is_triangle_rejected(mark_dir, v0, v1, v2) {
        return 0;
    }
    fx_mark_chop_world_triangle_points(planes, v0, v1, v2, out_pts)
}

pub fn fx_mark_emit_brush_fragment(
    used_tri: u32,
    used_point: u32,
    max_tris: u32,
    max_points: u32,
    fragment: &[FxWorldMarkPoint],
    v0_lmap: [f32; 2],
    v1_lmap: [f32; 2],
    v2_lmap: [f32; 2],
    v0_n: [f32; 3],
    v1_n: [f32; 3],
    v2_n: [f32; 3],
    context: [u8; 7],
    tris: &mut [FxMarkStagingTri],
    points: &mut [FxMarkStagingPoint],
) -> Result<(u32, u32), FxMarkEmitRefuse> {
    let n = fragment.len() as i32;
    if n < 3 {
        return Err(FxMarkEmitRefuse::FragmentTooSmall);
    }
    let used_t = used_tri as i32;
    let used_p = used_point as i32;
    if (max_points as i32) - used_p < n || (max_tris as i32) - used_t < n * 3 - 6 {
        return Err(FxMarkEmitRefuse::Overflow);
    }
    let fan = (n as u32) - 2;
    let tri_end = used_tri.saturating_add(fan);
    let point_end = used_point.saturating_add(n as u32);
    if tri_end as usize > tris.len() || point_end as usize > points.len() {
        return Err(FxMarkEmitRefuse::Overflow);
    }
    let base = used_point as u16;
    let mut i = 2u16;
    while i < n as u16 {
        let slot = (used_tri + u32::from(i) - 2) as usize;
        tris[slot] = FxMarkStagingTri {
            indices: [base + i - 1, base + i, base],
            context,
            _pad: 0,
        };
        i += 1;
    }
    let mut p = 0usize;
    while p < n as usize {
        let w = fragment[p].weights;
        points[used_point as usize + p] = FxMarkStagingPoint {
            xyz: fragment[p].xyz,
            lmap_coord: [
                w[0] * v0_lmap[0] + w[1] * v1_lmap[0] + w[2] * v2_lmap[0],
                w[0] * v0_lmap[1] + w[1] * v1_lmap[1] + w[2] * v2_lmap[1],
            ],
            normal: [
                w[0] * v0_n[0] + w[1] * v1_n[0] + w[2] * v2_n[0],
                w[0] * v0_n[1] + w[1] * v1_n[1] + w[2] * v2_n[1],
                w[0] * v0_n[2] + w[1] * v1_n[2] + w[2] * v2_n[2],
            ],
        };
        p += 1;
    }
    Ok((tri_end, point_end))
}

pub fn fx_mark_clip_world_surfaces(
    origin: [f32; 3],
    radius: f32,
    axis: [[f32; 3]; 3],
    mids: &[[f32; 3]],
    halves: &[[f32; 3]],
    positions: &[[f32; 3]],
    packed_indices: &[u32],
    surface_index_ranges: &[(u32, u32)],
) -> MarkWorldClipCensus {
    let planes = fx_mark_fragment_clip_planes(origin, axis, radius);
    let mark_dir = axis[0];
    let radius_sq = radius * radius;
    let n = core::cmp::min(
        core::cmp::min(mids.len(), halves.len()),
        surface_index_ranges.len(),
    );
    let mut census = MarkWorldClipCensus::default();
    let mut stacked = 0u32;
    let cap = R_MARK_FRAGMENTS_WORLD_SURF_STACK;
    let mut si = 0;
    while si < n {
        if stacked >= cap {
            break;
        }
        if crate::fx_mark_sphere_hits_bounds(origin, radius_sq, mids[si], halves[si]) {
            stacked = stacked.saturating_add(1);
            let (start, count) = surface_index_ranges[si];
            let start = start as usize;
            let end = start.saturating_add(count as usize);
            if count >= 3 && start < packed_indices.len() {
                let end = core::cmp::min(end, packed_indices.len());
                let mut t = start;
                while t + 2 < end {
                    let i0 = packed_indices[t] as usize;
                    let i1 = packed_indices[t + 1] as usize;
                    let i2 = packed_indices[t + 2] as usize;
                    t += 3;
                    if i0 >= positions.len() || i1 >= positions.len() || i2 >= positions.len() {
                        continue;
                    }
                    census.tri_seen = census.tri_seen.saturating_add(1);
                    let v0 = positions[i0];
                    let v1 = positions[i1];
                    let v2 = positions[i2];
                    if fx_mark_is_triangle_rejected(mark_dir, v0, v1, v2) {
                        census.tri_rejected = census.tri_rejected.saturating_add(1);
                        continue;
                    }
                    let pts = fx_mark_clip_world_triangle(mark_dir, &planes, v0, v1, v2);
                    if pts < 3 {
                        census.clip_zero = census.clip_zero.saturating_add(1);
                    } else {
                        census.clip_kept = census.clip_kept.saturating_add(pts - 2);
                    }
                }
            }
        }
        si += 1;
    }
    census
}

pub fn fx_mark_stage_world_surfaces(
    origin: [f32; 3],
    radius: f32,
    axis: [[f32; 3]; 3],
    mids: &[[f32; 3]],
    halves: &[[f32; 3]],
    positions: &[[f32; 3]],
    lightmap_uvs: &[[f32; 2]],
    normals: &[[f32; 3]],
    packed_indices: &[u32],
    surface_index_ranges: &[(u32, u32)],
    contexts: &[[u8; 7]],
    max_tris: u32,
    max_points: u32,
    tris: &mut [FxMarkStagingTri],
    points: &mut [FxMarkStagingPoint],
    retest_sphere: bool,
) -> MarkWorldStaging {
    let planes = fx_mark_fragment_clip_planes(origin, axis, radius);
    let mark_dir = axis[0];
    let radius_sq = radius * radius;
    let n = core::cmp::min(
        core::cmp::min(mids.len(), halves.len()),
        surface_index_ranges.len(),
    );
    let mut out = MarkWorldStaging::default();
    let mut stacked = 0u32;
    let cap = R_MARK_FRAGMENTS_WORLD_SURF_STACK;
    let mut si = 0;
    let mut fragment = [FxWorldMarkPoint::ZERO; R_MARK_CHOP_MAX_POINTS];
    while si < n {
        if stacked >= cap || out.overflow {
            break;
        }
        if !retest_sphere
            || crate::fx_mark_sphere_hits_bounds(origin, radius_sq, mids[si], halves[si])
        {
            stacked = stacked.saturating_add(1);
            let (start, count) = surface_index_ranges[si];
            let start = start as usize;
            let end = start.saturating_add(count as usize);
            if count >= 3 && start < packed_indices.len() {
                let end = core::cmp::min(end, packed_indices.len());
                let mut t = start;
                while t + 2 < end {
                    if out.overflow {
                        break;
                    }
                    let i0 = packed_indices[t] as usize;
                    let i1 = packed_indices[t + 1] as usize;
                    let i2 = packed_indices[t + 2] as usize;
                    t += 3;
                    if i0 >= positions.len() || i1 >= positions.len() || i2 >= positions.len() {
                        continue;
                    }
                    out.census.tri_seen = out.census.tri_seen.saturating_add(1);
                    let v0 = positions[i0];
                    let v1 = positions[i1];
                    let v2 = positions[i2];
                    if fx_mark_is_triangle_rejected(mark_dir, v0, v1, v2) {
                        out.census.tri_rejected = out.census.tri_rejected.saturating_add(1);
                        continue;
                    }
                    let pts =
                        fx_mark_chop_world_triangle_points(&planes, v0, v1, v2, &mut fragment);
                    if pts < 3 {
                        out.census.clip_zero = out.census.clip_zero.saturating_add(1);
                        continue;
                    }
                    out.census.clip_kept = out.census.clip_kept.saturating_add(pts - 2);
                    if i0 >= lightmap_uvs.len()
                        || i1 >= lightmap_uvs.len()
                        || i2 >= lightmap_uvs.len()
                        || i0 >= normals.len()
                        || i1 >= normals.len()
                        || i2 >= normals.len()
                    {
                        continue;
                    }
                    match fx_mark_emit_brush_fragment(
                        out.used_tri,
                        out.used_point,
                        max_tris,
                        max_points,
                        &fragment[..pts as usize],
                        lightmap_uvs[i0],
                        lightmap_uvs[i1],
                        lightmap_uvs[i2],
                        normals[i0],
                        normals[i1],
                        normals[i2],
                        contexts.get(si).copied().unwrap_or([0; 7]),
                        tris,
                        points,
                    ) {
                        Ok((used_tri, used_point)) => {
                            out.used_tri = used_tri;
                            out.used_point = used_point;
                        }
                        Err(FxMarkEmitRefuse::Overflow) => {
                            out.overflow = true;
                            break;
                        }
                        Err(FxMarkEmitRefuse::FragmentTooSmall) => {}
                    }
                }
            }
        }
        si += 1;
    }
    out
}
