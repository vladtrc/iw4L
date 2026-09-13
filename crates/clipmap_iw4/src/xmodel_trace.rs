use alloc::vec::Vec;
use libm::sqrtf;
use trace_iw4::{ENTITYNUM_WORLD, HITTYPE_ENTITY, Trace};

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct XModelCollTri {
    pub plane: [f32; 4],
    pub svec: [f32; 4],
    pub tvec: [f32; 4],
}

#[derive(Clone, Debug, PartialEq)]
pub struct XModelCollSurf {
    pub tris: Vec<XModelCollTri>,
    pub midpoint: [f32; 3],
    pub half_size: [f32; 3],
    pub bone_idx: i32,
    pub contents: u32,
    pub surf_flags: u32,
}

#[derive(Clone, Debug, PartialEq)]
pub struct XModelColl {
    pub coll_lod: i16,

    pub contents: u32,
    pub surfs: Vec<XModelCollSurf>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ClipStaticModel {
    pub origin: [f32; 3],
    pub inv_scaled_axis: [[f32; 3]; 3],
    pub bounds_mid: [f32; 3],
    pub bounds_half: [f32; 3],
    pub coll: XModelColl,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct StaticModelHit {
    pub index: usize,
    pub bone_idx: i32,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct StaticModelWalkStats {
    pub considered: u32,

    pub aabb_miss: u32,

    pub traced: u32,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RigidXform {
    pub axis: [[f32; 3]; 3],
    pub trans: [f32; 3],
}

impl RigidXform {
    pub const IDENTITY: Self = Self {
        axis: [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]],
        trans: [0.0, 0.0, 0.0],
    };
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct XModelAnimBone {
    pub posed: RigidXform,
    pub bind: RigidXform,
}

const TRACE_OFFSET: f32 = 0.125;

pub fn xmodel_trace_line(
    model: &XModelColl,
    results: &mut Trace,
    local_start: [f32; 3],
    local_end: [f32; 3],
    contentmask: u32,
) -> i32 {
    if model.coll_lod < 0 {
        return -1;
    }
    let mut part_index = -1;
    let delta = [
        local_end[0] - local_start[0],
        local_end[1] - local_start[1],
        local_end[2] - local_start[2],
    ];
    for surf in &model.surfs {
        if surf.contents & contentmask == 0 {
            continue;
        }
        if cm_trace_box_misses(
            local_start,
            local_end,
            surf.midpoint,
            surf.half_size,
            results.fraction,
        ) {
            continue;
        }
        if trace_surf_tris(surf, local_start, local_end, delta, results) {
            part_index = surf.bone_idx;
        }
    }
    part_index
}

pub fn xmodel_trace_line_animated(
    model: &XModelColl,
    results: &mut Trace,
    local_start: [f32; 3],
    local_end: [f32; 3],
    contentmask: u32,
    bones: &[XModelAnimBone],
    hide: &[u32],
) -> i32 {
    if model.coll_lod < 0 {
        return -1;
    }
    let mut part_index = -1;
    for surf in &model.surfs {
        if surf.contents & contentmask == 0 {
            continue;
        }
        let bone_idx = if surf.bone_idx < 0 {
            0usize
        } else {
            surf.bone_idx as usize
        };
        if hide_bit(hide, bone_idx) {
            continue;
        }
        let (start, end) = match bones.get(bone_idx) {
            Some(bone) if bone.posed != bone.bind => {
                remap_ray_to_bind(bone, local_start, local_end)
            }
            _ => (local_start, local_end),
        };
        if cm_trace_box_misses(start, end, surf.midpoint, surf.half_size, results.fraction) {
            continue;
        }
        let delta = [end[0] - start[0], end[1] - start[1], end[2] - start[2]];
        if trace_surf_tris(surf, start, end, delta, results) {
            part_index = surf.bone_idx;
        }
    }
    part_index
}

pub fn cm_trace_static_model(
    sm: &ClipStaticModel,
    results: &mut Trace,
    start: [f32; 3],
    end: [f32; 3],
    contentmask: u32,
) -> i32 {
    let local_start = world_to_local(sm, start);
    let local_end = world_to_local(sm, end);
    let bone = xmodel_trace_line(&sm.coll, results, local_start, local_end, contentmask);
    if bone >= 0 {
        results.hit_type = HITTYPE_ENTITY;
        results.hit_id = ENTITYNUM_WORLD;
        results.normal = normalize(transpose_transform(results.normal, sm.inv_scaled_axis));
        results.endpos = [
            start[0] + (end[0] - start[0]) * results.fraction,
            start[1] + (end[1] - start[1]) * results.fraction,
            start[2] + (end[2] - start[2]) * results.fraction,
        ];
    }
    bone
}

pub fn point_trace_static_models<'a, I>(
    models: I,
    start: [f32; 3],
    end: [f32; 3],
    contentmask: u32,
    results: &mut Trace,
) -> Option<StaticModelHit>
where
    I: IntoIterator<Item = &'a ClipStaticModel>,
{
    let mut stats = StaticModelWalkStats::default();
    point_trace_static_models_stats(models, start, end, contentmask, results, &mut stats)
}

pub fn point_trace_static_models_stats<'a, I>(
    models: I,
    start: [f32; 3],
    end: [f32; 3],
    contentmask: u32,
    results: &mut Trace,
    stats: &mut StaticModelWalkStats,
) -> Option<StaticModelHit>
where
    I: IntoIterator<Item = &'a ClipStaticModel>,
{
    point_trace_static_models_stats_keyed(
        models.into_iter().enumerate(),
        start,
        end,
        contentmask,
        results,
        stats,
    )
}

pub fn point_trace_static_models_stats_keyed<'a, I>(
    models: I,
    start: [f32; 3],
    end: [f32; 3],
    contentmask: u32,
    results: &mut Trace,
    stats: &mut StaticModelWalkStats,
) -> Option<StaticModelHit>
where
    I: IntoIterator<Item = (usize, &'a ClipStaticModel)>,
{
    let mut winner = None;
    for (index, sm) in models {
        if sm.coll.contents & contentmask == 0 {
            continue;
        }
        stats.considered = stats.considered.saturating_add(1);
        if cm_trace_box_misses(start, end, sm.bounds_mid, sm.bounds_half, results.fraction) {
            stats.aabb_miss = stats.aabb_miss.saturating_add(1);
            continue;
        }
        stats.traced = stats.traced.saturating_add(1);
        let bone = cm_trace_static_model(sm, results, start, end, contentmask);
        if bone >= 0 {
            winner = Some(StaticModelHit {
                index,
                bone_idx: bone,
            });
        }
    }
    winner
}

pub fn cm_trace_box_misses(
    start: [f32; 3],
    end: [f32; 3],
    mid: [f32; 3],
    half: [f32; 3],
    fraction: f32,
) -> bool {
    let mins = [mid[0] - half[0], mid[1] - half[1], mid[2] - half[2]];
    let maxs = [mid[0] + half[0], mid[1] + half[1], mid[2] + half[2]];
    cm_trace_box_mins_maxs(start, end, mins, maxs, fraction)
}

fn cm_trace_box_mins_maxs(
    start: [f32; 3],
    end: [f32; 3],
    mins: [f32; 3],
    maxs: [f32; 3],
    mut fraction: f32,
) -> bool {
    let inv_delta = [
        inv_delta(start[0], end[0]),
        inv_delta(start[1], end[1]),
        inv_delta(start[2], end[2]),
    ];
    let mut enter_frac = 0.0_f32;
    let bounds = [mins, maxs];
    let mut sign = -1.0_f32;
    for bound in &bounds {
        for t in 0..3 {
            let dist1 = (start[t] - bound[t]) * sign;
            let dist2 = (end[t] - bound[t]) * sign;
            if dist1 <= 0.0 {
                if dist2 > 0.0 {
                    let fraca = dist1 * inv_delta[t] * sign;
                    if fraca <= enter_frac {
                        return true;
                    }
                    fraction = if fraca - fraction < 0.0 {
                        fraca
                    } else {
                        fraction
                    };
                }
            } else {
                if dist2 > 0.0 {
                    return true;
                }
                let frac = dist1 * inv_delta[t] * sign;
                if fraction <= frac {
                    return true;
                }
                enter_frac = if enter_frac - frac < 0.0 {
                    frac
                } else {
                    enter_frac
                };
            }
        }
        sign = 1.0;
    }
    false
}

fn inv_delta(start: f32, end: f32) -> f32 {
    let diff = start - end;
    if diff == 0.0 { 0.0 } else { 1.0 / diff }
}

fn trace_surf_tris(
    surf: &XModelCollSurf,
    start: [f32; 3],
    end: [f32; 3],
    delta: [f32; 3],
    results: &mut Trace,
) -> bool {
    let mut hit = false;
    for tri in &surf.tris {
        let end_dist = plane_dist(tri.plane, end);
        if end_dist >= 0.0 {
            continue;
        }
        let start_dist = plane_dist(tri.plane, start);
        if start_dist <= 0.0 {
            continue;
        }
        let mut frac = (start_dist - TRACE_OFFSET) / (start_dist - end_dist);
        if frac < 0.0 {
            frac = 0.0;
        }
        if frac >= results.fraction {
            continue;
        }
        let hit_frac = start_dist / (start_dist - end_dist);
        let point = [
            start[0] + hit_frac * delta[0],
            start[1] + hit_frac * delta[1],
            start[2] + hit_frac * delta[2],
        ];
        let s = plane_dist(tri.svec, point);
        if s < 0.0 {
            continue;
        }
        let t = plane_dist(tri.tvec, point);
        if t < 0.0 || s + t > 1.0 {
            continue;
        }
        results.startsolid = 0;
        results.allsolid = 0;
        results.fraction = frac;
        results.surface_flags = surf.surf_flags;
        results.contents = surf.contents;
        results.normal = [tri.plane[0], tri.plane[1], tri.plane[2]];
        hit = true;
    }
    hit
}

fn hide_bit(hide: &[u32], bone: usize) -> bool {
    let word = bone / 32;
    let bit = bone % 32;
    hide.get(word).copied().unwrap_or(0) & (0x8000_0000 >> bit) != 0
}

fn remap_ray_to_bind(
    bone: &XModelAnimBone,
    local_start: [f32; 3],
    local_end: [f32; 3],
) -> ([f32; 3], [f32; 3]) {
    let inv_bind = inverse_rigid(&bone.bind);
    let axis = multiply_43(&inv_bind, &bone.posed);
    (
        transpose_transform_43(local_start, &axis),
        transpose_transform_43(local_end, &axis),
    )
}

type Mat43 = [[f32; 3]; 4];

fn to_43(m: &RigidXform) -> Mat43 {
    [m.axis[0], m.axis[1], m.axis[2], m.trans]
}

fn from_43(m: Mat43) -> RigidXform {
    RigidXform {
        axis: [m[0], m[1], m[2]],
        trans: m[3],
    }
}

fn inverse_rigid(m: &RigidXform) -> RigidXform {
    let axis = [
        [m.axis[0][0], m.axis[1][0], m.axis[2][0]],
        [m.axis[0][1], m.axis[1][1], m.axis[2][1]],
        [m.axis[0][2], m.axis[1][2], m.axis[2][2]],
    ];
    let trans = [
        -(axis[0][0] * m.trans[0] + axis[0][1] * m.trans[1] + axis[0][2] * m.trans[2]),
        -(axis[1][0] * m.trans[0] + axis[1][1] * m.trans[1] + axis[1][2] * m.trans[2]),
        -(axis[2][0] * m.trans[0] + axis[2][1] * m.trans[1] + axis[2][2] * m.trans[2]),
    ];
    RigidXform { axis, trans }
}

fn multiply_43(in1: &RigidXform, in2: &RigidXform) -> RigidXform {
    let a = to_43(in1);
    let b = to_43(in2);
    let mut out = [[0.0_f32; 3]; 4];
    for i in 0..3 {
        for j in 0..3 {
            out[i][j] = a[i][0] * b[0][j] + a[i][1] * b[1][j] + a[i][2] * b[2][j];
        }
    }
    for j in 0..3 {
        out[3][j] = a[3][0] * b[0][j] + a[3][1] * b[1][j] + a[3][2] * b[2][j] + b[3][j];
    }
    from_43(out)
}

fn transpose_transform_43(p: [f32; 3], axis: &RigidXform) -> [f32; 3] {
    let d = [
        p[0] - axis.trans[0],
        p[1] - axis.trans[1],
        p[2] - axis.trans[2],
    ];
    transpose_transform(d, axis.axis)
}

fn plane_dist(plane: [f32; 4], p: [f32; 3]) -> f32 {
    plane[0] * p[0] + plane[1] * p[1] + plane[2] * p[2] - plane[3]
}

fn world_to_local(sm: &ClipStaticModel, world: [f32; 3]) -> [f32; 3] {
    let delta = [
        world[0] - sm.origin[0],
        world[1] - sm.origin[1],
        world[2] - sm.origin[2],
    ];
    matrix_transform(delta, sm.inv_scaled_axis)
}

fn matrix_transform(v: [f32; 3], m: [[f32; 3]; 3]) -> [f32; 3] {
    [
        v[0] * m[0][0] + v[1] * m[1][0] + v[2] * m[2][0],
        v[0] * m[0][1] + v[1] * m[1][1] + v[2] * m[2][1],
        v[0] * m[0][2] + v[1] * m[1][2] + v[2] * m[2][2],
    ]
}

fn transpose_transform(v: [f32; 3], m: [[f32; 3]; 3]) -> [f32; 3] {
    [
        v[0] * m[0][0] + v[1] * m[0][1] + v[2] * m[0][2],
        v[0] * m[1][0] + v[1] * m[1][1] + v[2] * m[1][2],
        v[0] * m[2][0] + v[1] * m[2][1] + v[2] * m[2][2],
    ]
}

fn normalize(v: [f32; 3]) -> [f32; 3] {
    let len = sqrtf(v[0] * v[0] + v[1] * v[1] + v[2] * v[2]);
    if len <= 0.0 {
        return v;
    }
    [v[0] / len, v[1] / len, v[2] / len]
}
