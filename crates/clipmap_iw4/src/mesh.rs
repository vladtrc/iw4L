use alloc::vec::Vec;
use libm::sqrtf;
use trace_iw4::{ENTITYNUM_WORLD, HITTYPE_ENTITY, Trace};

use crate::TraceExtents;

const SURFACE_CLIP_EPSILON: f32 = 0.125;

const EQUAL_EPSILON: f32 = 0.001;

pub const VERTS_PER_SEGMENT: usize = 1024;
const WALKABLE_NORMAL_Z: f32 = 0.7;
const CONTENTS_SOLID: u32 = 1;

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct ClipAabbNode {
    pub origin: [f32; 3],
    pub half_size: [f32; 3],
    pub material_index: u16,
    pub child_count: u16,

    pub u: i32,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ClipPartition {
    pub tri_count: u8,
    pub first_tri: i32,

    pub first_vert_segment: u8,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct MeshWalkCensus {
    pub tris_tested: u32,
    pub aabb_nodes_visited: u32,

    pub forest_fallback: u8,

    pub aabb_roots: u32,
}

/// The collision mesh tables as read from a zone: immutable once the walk that
/// produced them is over, and shared from there by everyone who traces against
/// the map rather than copied per consumer.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ClipMeshTables {
    pub verts: Vec<[f32; 3]>,

    pub tri_indices: Vec<u16>,

    pub tri_edge_is_walkable: Vec<u8>,

    pub tri_surface_flags: Vec<u32>,

    pub tri_content_flags: Vec<u32>,

    pub aabb_trees: Vec<ClipAabbNode>,
    pub partitions: Vec<ClipPartition>,

    pub aabb_roots: Vec<u16>,
}

impl ClipMeshTables {
    pub fn as_ref(&self) -> ClipMeshRef<'_> {
        ClipMeshRef {
            verts: &self.verts,
            tri_indices: &self.tri_indices,
            tri_edge_is_walkable: &self.tri_edge_is_walkable,
            tri_surface_flags: &self.tri_surface_flags,
            tri_content_flags: &self.tri_content_flags,
            aabb_trees: &self.aabb_trees,
            partitions: &self.partitions,
            aabb_roots: &self.aabb_roots,
        }
    }

    pub fn tri_count(&self) -> usize {
        self.tri_indices.len() / 3
    }
}

#[derive(Clone, Copy, Debug)]
pub struct ClipMeshRef<'a> {
    pub verts: &'a [[f32; 3]],

    pub tri_indices: &'a [u16],

    /// One bit per triangle edge, `3 * tri + k`, where k = 0 is v1-v2, 1 is
    /// v0-v2 and 2 is v0-v1.
    pub tri_edge_is_walkable: &'a [u8],

    pub tri_surface_flags: &'a [u32],

    pub tri_content_flags: &'a [u32],

    pub aabb_trees: &'a [ClipAabbNode],
    pub partitions: &'a [ClipPartition],

    pub aabb_roots: &'a [u16],
}

impl<'a> ClipMeshRef<'a> {
    pub fn from_linear(
        verts: &'a [[f32; 3]],
        tri_indices: &'a [u16],
        tri_surface_flags: &'a [u32],
    ) -> Self {
        Self {
            verts,
            tri_indices,
            tri_edge_is_walkable: &[],
            tri_surface_flags,
            tri_content_flags: &[],
            aabb_trees: &[],
            partitions: &[],
            aabb_roots: &[],
        }
    }
}

pub fn aabb_forest_roots(trees: &[ClipAabbNode]) -> Vec<u16> {
    let n = trees.len();
    let mut is_child = alloc::vec![false; n];
    for node in trees {
        if node.child_count == 0 {
            continue;
        }
        if node.u < 0 {
            continue;
        }
        let first = node.u as usize;
        for i in 0..node.child_count as usize {
            if let Some(slot) = is_child.get_mut(first.saturating_add(i)) {
                *slot = true;
            }
        }
    }
    (0..n)
        .filter(|&i| !is_child[i])
        .filter_map(|i| u16::try_from(i).ok())
        .collect()
}

#[derive(Clone, Copy, Debug)]
struct Capsule {
    offset: [f32; 3],
    radius: f32,
    offset_z: f32,
}

impl Capsule {
    fn from_bounds(mins: [f32; 3], maxs: [f32; 3]) -> Self {
        let offset = [
            (mins[0] + maxs[0]) * 0.5,
            (mins[1] + maxs[1]) * 0.5,
            (mins[2] + maxs[2]) * 0.5,
        ];
        let size = [
            maxs[0] - offset[0],
            maxs[1] - offset[1],
            maxs[2] - offset[2],
        ];
        let radius = if size[0] <= size[2] { size[0] } else { size[2] };
        Self {
            offset,
            radius,
            offset_z: size[2] - radius,
        }
    }
}

pub fn trace_through_mesh(mesh: &ClipMeshRef<'_>, ext: &TraceExtents) -> Trace {
    trace_through_mesh_with_census(mesh, ext).0
}

pub fn trace_through_mesh_with_census(
    mesh: &ClipMeshRef<'_>,
    ext: &TraceExtents,
) -> (Trace, MeshWalkCensus) {
    let mut best = Trace {
        fraction: 1.0,
        endpos: ext.end,
        ..Trace::default()
    };
    let mut census = MeshWalkCensus::default();
    trace_through_mesh_into(mesh, ext, &mut best, &mut census);
    (best, census)
}

pub fn trace_through_mesh_into(
    mesh: &ClipMeshRef<'_>,
    ext: &TraceExtents,
    best: &mut Trace,
    census: &mut MeshWalkCensus,
) {
    census.aabb_roots = census
        .aabb_roots
        .saturating_add(mesh.aabb_roots.len() as u32);
    if mesh.tri_indices.len() < 3 || mesh.verts.is_empty() {
        return;
    }
    if mesh.tri_content_flags.is_empty() {
        if ext.mask & CONTENTS_SOLID == 0 {
            return;
        }
    } else if ext.mask == 0 {
        return;
    }

    let cap = Capsule::from_bounds(ext.mins, ext.maxs);
    let start = [
        ext.start[0] + cap.offset[0],
        ext.start[1] + cap.offset[1],
        ext.start[2] + cap.offset[2],
    ];
    let end = [
        ext.end[0] + cap.offset[0],
        ext.end[1] + cap.offset[1],
        ext.end[2] + cap.offset[2],
    ];
    let delta = [end[0] - start[0], end[1] - start[1], end[2] - start[2]];

    let r = cap.radius + cap.offset_z + SURFACE_CLIP_EPSILON;
    let cull_mins = [
        start[0].min(end[0]) - r,
        start[1].min(end[1]) - r,
        start[2].min(end[2]) - r,
    ];
    let cull_maxs = [
        start[0].max(end[0]) + r,
        start[1].max(end[1]) + r,
        start[2].max(end[2]) + r,
    ];

    if !mesh.aabb_trees.is_empty() {
        walk_aabb_forest(
            mesh, cap, start, delta, cull_mins, cull_maxs, ext.mask, best, census,
        );
    } else {
        let tri_count = mesh.tri_indices.len() / 3;
        for ti in 0..tri_count {
            test_tri(
                mesh, cap, start, delta, cull_mins, cull_maxs, ti, 0, ext.mask, best, census,
            );
            if best.allsolid != 0 {
                break;
            }
        }
    }

    if best.fraction < 1.0 {
        best.endpos = [
            start[0] + delta[0] * best.fraction - cap.offset[0],
            start[1] + delta[1] * best.fraction - cap.offset[1],
            start[2] + delta[2] * best.fraction - cap.offset[2],
        ];
        if best.walkable == 0 && best.startsolid == 0 {
            best.walkable = u8::from(best.normal[2] >= WALKABLE_NORMAL_Z);
        }
    } else {
        best.endpos = [
            end[0] - cap.offset[0],
            end[1] - cap.offset[1],
            end[2] - cap.offset[2],
        ];
    }
}

fn walk_aabb_forest(
    mesh: &ClipMeshRef<'_>,
    cap: Capsule,
    start: [f32; 3],
    delta: [f32; 3],
    cull_mins: [f32; 3],
    cull_maxs: [f32; 3],
    mask: u32,
    best: &mut Trace,
    census: &mut MeshWalkCensus,
) {
    let mut stack = Vec::new();
    for &root in mesh.aabb_roots {
        stack.push(root as usize);
    }
    while let Some(idx) = stack.pop() {
        census.aabb_nodes_visited = census.aabb_nodes_visited.saturating_add(1);
        let Some(node) = mesh.aabb_trees.get(idx) else {
            continue;
        };
        if !node_overlaps_sweep(node.origin, node.half_size, cull_mins, cull_maxs) {
            continue;
        }
        if node.child_count != 0 {
            if node.u < 0 {
                continue;
            }
            let first = node.u as usize;
            for i in (0..node.child_count as usize).rev() {
                stack.push(first.saturating_add(i));
            }
            continue;
        }
        if node.u < 0 {
            continue;
        }
        let Some(part) = mesh.partitions.get(node.u as usize) else {
            continue;
        };
        if part.first_tri < 0 {
            continue;
        }
        let start_ti = part.first_tri as usize;
        let end_ti = start_ti.saturating_add(part.tri_count as usize);
        let vert_base = part.first_vert_segment as usize * VERTS_PER_SEGMENT;
        for ti in start_ti..end_ti {
            test_tri(
                mesh, cap, start, delta, cull_mins, cull_maxs, ti, vert_base, mask, best, census,
            );
            if best.allsolid != 0 {
                return;
            }
        }
    }
}

fn node_overlaps_sweep(
    origin: [f32; 3],
    half: [f32; 3],
    cull_mins: [f32; 3],
    cull_maxs: [f32; 3],
) -> bool {
    let mins = [
        origin[0] - half[0],
        origin[1] - half[1],
        origin[2] - half[2],
    ];
    let maxs = [
        origin[0] + half[0],
        origin[1] + half[1],
        origin[2] + half[2],
    ];
    mins[0] <= cull_maxs[0]
        && cull_mins[0] <= maxs[0]
        && mins[1] <= cull_maxs[1]
        && cull_mins[1] <= maxs[1]
        && mins[2] <= cull_maxs[2]
        && cull_mins[2] <= maxs[2]
}

fn test_tri(
    mesh: &ClipMeshRef<'_>,
    cap: Capsule,
    start: [f32; 3],
    delta: [f32; 3],
    cull_mins: [f32; 3],
    cull_maxs: [f32; 3],
    ti: usize,

    vert_base: usize,
    mask: u32,
    best: &mut Trace,
    census: &mut MeshWalkCensus,
) {
    let Some(i0) = mesh.tri_indices.get(ti.saturating_mul(3)).copied() else {
        return;
    };
    let Some(i1) = mesh.tri_indices.get(ti * 3 + 1).copied() else {
        return;
    };
    let Some(i2) = mesh.tri_indices.get(ti * 3 + 2).copied() else {
        return;
    };
    let (Some(&v0), Some(&v1), Some(&v2)) = (
        mesh.verts.get(vert_base + i0 as usize),
        mesh.verts.get(vert_base + i1 as usize),
        mesh.verts.get(vert_base + i2 as usize),
    ) else {
        return;
    };
    if !tri_overlaps_aabb(v0, v1, v2, cull_mins, cull_maxs) {
        return;
    }
    let contents = if mesh.tri_content_flags.is_empty() {
        CONTENTS_SOLID
    } else {
        let cflags = mesh.tri_content_flags.get(ti).copied().unwrap_or(0);
        if cflags & mask == 0 {
            return;
        }
        cflags
    };
    census.tris_tested = census.tris_tested.saturating_add(1);
    let edge_walkable = [0, 1, 2].map(|k| {
        let bit = ti * 3 + k;
        mesh.tri_edge_is_walkable
            .get(bit >> 3)
            .is_some_and(|b| b & (1 << (bit & 7)) != 0)
    });
    capsule_through_triangle(
        cap,
        start,
        delta,
        v0,
        v1,
        v2,
        edge_walkable,
        mesh.tri_surface_flags.get(ti).copied().unwrap_or(0),
        contents,
        best,
    );
}

fn tri_overlaps_aabb(
    v0: [f32; 3],
    v1: [f32; 3],
    v2: [f32; 3],
    mins: [f32; 3],
    maxs: [f32; 3],
) -> bool {
    let tmin = [
        v0[0].min(v1[0]).min(v2[0]),
        v0[1].min(v1[1]).min(v2[1]),
        v0[2].min(v1[2]).min(v2[2]),
    ];
    let tmax = [
        v0[0].max(v1[0]).max(v2[0]),
        v0[1].max(v1[1]).max(v2[1]),
        v0[2].max(v1[2]).max(v2[2]),
    ];
    tmin[0] <= maxs[0]
        && mins[0] <= tmax[0]
        && tmin[1] <= maxs[1]
        && mins[1] <= tmax[1]
        && tmin[2] <= maxs[2]
        && mins[2] <= tmax[2]
}

fn capsule_through_triangle(
    cap: Capsule,
    start: [f32; 3],
    delta: [f32; 3],
    v0: [f32; 3],
    v1: [f32; 3],
    v2: [f32; 3],
    edge_walkable: [bool; 3],
    surface_flags: u32,
    contents: u32,
    trace: &mut Trace,
) {
    let v0_v1 = [v0[0] - v1[0], v0[1] - v1[1], v0[2] - v1[2]];
    let v0_v2 = [v0[0] - v2[0], v0[1] - v2[1], v0[2] - v2[2]];
    let mut area_n = cross(v0_v2, v0_v1);
    let proj = dot(delta, area_n);
    if proj >= 0.0 {
        return;
    }
    let area_x2 = normalize_to(&mut area_n);
    if area_x2 <= 0.0 {
        return;
    }
    let normal = area_n;

    let mut sphere_start = start;
    let tip = if normal[2] < 0.0 {
        -cap.offset_z
    } else {
        cap.offset_z
    };
    sphere_start[2] -= tip;

    let shifted = [
        sphere_start[0] - cap.radius * normal[0],
        sphere_start[1] - cap.radius * normal[1],
        sphere_start[2] - cap.radius * normal[2],
    ];
    let start_v0 = [shifted[0] - v0[0], shifted[1] - v0[1], shifted[2] - v0[2]];
    let hit_dist = dot(start_v0, normal);
    let (hit_frac, start_solid, plane_pt) = if hit_dist >= 0.0 {
        let frac = (-(hit_dist - SURFACE_CLIP_EPSILON) * area_x2) / proj;
        if frac >= trace.fraction {
            return;
        }
        (frac, false, start_v0)
    } else {
        let from_sphere = [
            sphere_start[0] - v0[0],
            sphere_start[1] - v0[1],
            sphere_start[2] - v0[2],
        ];
        let start_dist = dot(from_sphere, normal);
        if start_dist * start_dist >= cap.radius * cap.radius {
            return;
        }
        let plane_pt = [
            from_sphere[0] - start_dist * normal[0],
            from_sphere[1] - start_dist * normal[1],
            from_sphere[2] - start_dist * normal[2],
        ];
        (0.0, true, plane_pt)
    };

    let trace_plane = cross(delta, plane_pt);
    let mut missed_edge = false;
    let mut vert_to_check: Option<[f32; 3]> = None;
    let mut vert_walkable = true;

    let neg_v = dot(trace_plane, v0_v1);
    if neg_v < 0.0 {
        missed_edge = true;
        match sphere_through_edge(
            cap.radius,
            sphere_start,
            delta,
            v0,
            v0_v1,
            surface_flags,
            contents,
            trace,
        ) {
            EdgeHit::Hits => {
                trace.walkable = u8::from(edge_walkable[2]);
                return;
            }
            EdgeHit::MayV0 => {
                vert_to_check = Some(v0);
                vert_walkable &= edge_walkable[2];
            }
            EdgeHit::MayV1 => {
                vert_to_check = Some(v1);
                vert_walkable &= edge_walkable[2];
            }
            EdgeHit::Miss => {}
        }
    }
    let u = dot(trace_plane, v0_v2);
    if u > 0.0 {
        missed_edge = true;
        match sphere_through_edge(
            cap.radius,
            sphere_start,
            delta,
            v0,
            v0_v2,
            surface_flags,
            contents,
            trace,
        ) {
            EdgeHit::Hits => {
                trace.walkable = u8::from(edge_walkable[1]);
                return;
            }
            EdgeHit::MayV0 => {
                vert_to_check = Some(v0);
                vert_walkable &= edge_walkable[1];
            }
            EdgeHit::MayV1 => {
                vert_to_check = Some(v2);
                vert_walkable &= edge_walkable[1];
            }
            EdgeHit::Miss => {}
        }
    }
    if proj > (u - neg_v) {
        missed_edge = true;
        let v1_v2 = [v1[0] - v2[0], v1[1] - v2[1], v1[2] - v2[2]];
        match sphere_through_edge(
            cap.radius,
            sphere_start,
            delta,
            v1,
            v1_v2,
            surface_flags,
            contents,
            trace,
        ) {
            EdgeHit::Hits => {
                trace.walkable = u8::from(edge_walkable[0]);
                return;
            }
            EdgeHit::MayV0 => {
                vert_to_check = Some(v1);
                vert_walkable &= edge_walkable[0];
            }
            EdgeHit::MayV1 => {
                vert_to_check = Some(v2);
                vert_walkable &= edge_walkable[0];
            }
            EdgeHit::Miss => {}
        }
    }

    if missed_edge {
        if let Some(vert) = vert_to_check {
            sphere_through_vertex(
                cap.radius,
                sphere_start,
                delta,
                vert,
                vert_walkable,
                surface_flags,
                contents,
                trace,
            );
        }
        return;
    }

    let frac = if hit_frac < 0.0 { 0.0 } else { hit_frac };
    if frac >= trace.fraction {
        return;
    }
    trace.fraction = frac;
    trace.normal = normal;
    trace.startsolid = u8::from(start_solid);
    trace.contents = contents;
    trace.surface_flags = surface_flags;
    trace.hit_type = HITTYPE_ENTITY;
    trace.hit_id = ENTITYNUM_WORLD;
    trace.walkable = 0;
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum EdgeHit {
    Miss,
    Hits,
    MayV0,
    MayV1,
}

fn sphere_through_edge(
    radius: f32,
    sphere_start: [f32; 3],
    delta: [f32; 3],
    v0: [f32; 3],
    v0_v1: [f32; 3],
    surface_flags: u32,
    contents: u32,
    trace: &mut Trace,
) -> EdgeHit {
    let start_delta = [
        sphere_start[0] - v0[0],
        sphere_start[1] - v0[1],
        sphere_start[2] - v0[2],
    ];
    let perpendicular = cross(v0_v1, delta);
    let scaled_dist = dot(start_delta, perpendicular);
    let perp_len_sq = len_sq(perpendicular);
    let r = radius + SURFACE_CLIP_EPSILON;
    let disc = r * r * perp_len_sq - scaled_dist * scaled_dist;
    if disc <= 0.0 {
        return EdgeHit::Miss;
    }
    let edge_len_sq = len_sq(v0_v1);
    if perp_len_sq <= 0.0 || edge_len_sq <= 0.0 {
        return EdgeHit::Miss;
    }
    let f = sqrtf(edge_len_sq * disc) / perp_len_sq;
    let edge_cross = cross(start_delta, v0_v1);
    let t_scaled = dot(edge_cross, perpendicular);
    let t = t_scaled / perp_len_sq;
    let frac_leave = t + f;
    if frac_leave < 0.0 {
        return EdgeHit::Miss;
    }
    let frac_enter = t - f;
    if frac_enter >= trace.fraction {
        return EdgeHit::Miss;
    }
    if frac_enter < 0.0 {
        let scaled_proj = -dot(start_delta, v0_v1);
        if scaled_proj <= 0.0 {
            return EdgeHit::MayV0;
        }
        if scaled_proj >= edge_len_sq {
            return EdgeHit::MayV1;
        }
        let along = scaled_proj / edge_len_sq;
        let mut normal = [
            along * v0_v1[0] + start_delta[0],
            along * v0_v1[1] + start_delta[1],
            along * v0_v1[2] + start_delta[2],
        ];
        if dot(normal, delta) >= 0.0 {
            return EdgeHit::Miss;
        }
        normalize_to(&mut normal);
        let inner_disc = radius * radius * perp_len_sq - scaled_dist * scaled_dist;
        trace.fraction = 0.0;
        trace.normal = normal;
        trace.startsolid = u8::from(edge_len_sq * inner_disc > t_scaled * t_scaled);
        trace.contents = contents;
        trace.surface_flags = surface_flags;
        trace.hit_type = HITTYPE_ENTITY;
        trace.hit_id = ENTITYNUM_WORLD;
        return EdgeHit::Hits;
    }
    let hit_delta = [
        frac_enter * delta[0] + start_delta[0],
        frac_enter * delta[1] + start_delta[1],
        frac_enter * delta[2] + start_delta[2],
    ];
    let scaled_proj = -dot(hit_delta, v0_v1);
    if scaled_proj <= 0.0 {
        return EdgeHit::MayV0;
    }
    if scaled_proj >= edge_len_sq {
        return EdgeHit::MayV1;
    }
    let scaled_normal = [
        (scaled_proj / edge_len_sq) * v0_v1[0] + hit_delta[0],
        (scaled_proj / edge_len_sq) * v0_v1[1] + hit_delta[1],
        (scaled_proj / edge_len_sq) * v0_v1[2] + hit_delta[2],
    ];
    let inv_r = 1.0 / r;
    trace.fraction = frac_enter;
    trace.normal = [
        scaled_normal[0] * inv_r,
        scaled_normal[1] * inv_r,
        scaled_normal[2] * inv_r,
    ];
    trace.contents = contents;
    trace.surface_flags = surface_flags;
    trace.hit_type = HITTYPE_ENTITY;
    trace.hit_id = ENTITYNUM_WORLD;
    EdgeHit::Hits
}

fn sphere_through_vertex(
    radius: f32,
    sphere_start: [f32; 3],
    delta: [f32; 3],
    vert: [f32; 3],
    is_walkable: bool,
    surface_flags: u32,
    contents: u32,
    trace: &mut Trace,
) {
    let start_delta = [
        sphere_start[0] - vert[0],
        sphere_start[1] - vert[1],
        sphere_start[2] - vert[2],
    ];
    let b = dot(delta, start_delta);
    if b >= 0.0 {
        return;
    }
    let start_len_sq = len_sq(start_delta);
    let r = radius + SURFACE_CLIP_EPSILON;
    let c = start_len_sq - r * r;
    let (frac, normal) = if c > 0.0 {
        let a = len_sq(delta);
        let b_sq = b * b;
        let disc = b_sq - a * c;
        if disc < b_sq * EQUAL_EPSILON {
            return;
        }
        let frac = (-sqrtf(disc) - b) / a;
        if frac >= trace.fraction {
            return;
        }
        let inv_r = 1.0 / r;
        let mut normal = [
            (start_delta[0] + frac * delta[0]) * inv_r,
            (start_delta[1] + frac * delta[1]) * inv_r,
            (start_delta[2] + frac * delta[2]) * inv_r,
        ];
        let approx_recip_len = (3.0 - len_sq(normal)) * 0.5;
        normal = normal.map(|c| c * approx_recip_len);
        (frac, normal)
    } else {
        let mut normal = start_delta;
        normalize_to(&mut normal);
        if start_len_sq < radius * radius {
            trace.startsolid = 1;
        }
        (0.0, normal)
    };
    trace.fraction = frac;
    trace.normal = normal;
    trace.contents = contents;
    trace.surface_flags = surface_flags;
    trace.hit_type = HITTYPE_ENTITY;
    trace.hit_id = ENTITYNUM_WORLD;
    trace.walkable = u8::from(is_walkable);
}

fn cross(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}

fn dot(a: [f32; 3], b: [f32; 3]) -> f32 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

fn len_sq(v: [f32; 3]) -> f32 {
    dot(v, v)
}

fn normalize_to(v: &mut [f32; 3]) -> f32 {
    let len = sqrtf(len_sq(*v));
    if len > 0.0 {
        v[0] /= len;
        v[1] /= len;
        v[2] /= len;
    }
    len
}

pub fn flatten_tri_surface_flags(
    aabb_leaves: &[(u16, i32)],
    partitions: &[(u8, i32)],
    material_surface_flags: &[u32],
    tri_count: usize,
) -> alloc::vec::Vec<u32> {
    let mut out = alloc::vec![0u32; tri_count];
    for &(mat, part_i) in aabb_leaves {
        if part_i < 0 {
            continue;
        }
        let Some(&(tri_n, first)) = partitions.get(part_i as usize) else {
            continue;
        };
        let sflags = material_surface_flags
            .get(mat as usize)
            .copied()
            .unwrap_or(0);
        if first < 0 {
            continue;
        }
        let start = first as usize;
        let end = start.saturating_add(tri_n as usize).min(tri_count);
        if let Some(slice) = out.get_mut(start..end) {
            for flags in slice {
                *flags = sflags;
            }
        }
    }
    out
}

pub fn flatten_tri_material_index(
    aabb_leaves: &[(u16, i32)],
    partitions: &[(u8, i32)],
    tri_count: usize,
) -> alloc::vec::Vec<u16> {
    let mut out = alloc::vec![u16::MAX; tri_count];
    for &(mat, part_i) in aabb_leaves {
        if part_i < 0 {
            continue;
        }
        let Some(&(tri_n, first)) = partitions.get(part_i as usize) else {
            continue;
        };
        if first < 0 {
            continue;
        }
        let start = first as usize;
        let end = start.saturating_add(tri_n as usize).min(tri_count);
        if let Some(slice) = out.get_mut(start..end) {
            for slot in slice {
                *slot = mat;
            }
        }
    }
    out
}
