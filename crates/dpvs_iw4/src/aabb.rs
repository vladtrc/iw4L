#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Bounds {
    mid: [f32; 3],
    half: [f32; 3],
}

impl Bounds {
    #[must_use]
    pub const fn from_mid_half(mid: [f32; 3], half: [f32; 3]) -> Self {
        Self { mid, half }
    }

    #[must_use]
    pub fn from_mins_maxs(mins: [f32; 3], maxs: [f32; 3]) -> Self {
        Self {
            mid: [
                (mins[0] + maxs[0]) * 0.5,
                (mins[1] + maxs[1]) * 0.5,
                (mins[2] + maxs[2]) * 0.5,
            ],
            half: [
                (maxs[0] - mins[0]) * 0.5,
                (maxs[1] - mins[1]) * 0.5,
                (maxs[2] - mins[2]) * 0.5,
            ],
        }
    }

    #[must_use]
    pub const fn mid(self) -> [f32; 3] {
        self.mid
    }

    #[must_use]
    pub const fn half(self) -> [f32; 3] {
        self.half
    }

    #[must_use]
    pub fn mins(self) -> [f32; 3] {
        [
            self.mid[0] - self.half[0],
            self.mid[1] - self.half[1],
            self.mid[2] - self.half[2],
        ]
    }

    #[must_use]
    pub fn maxs(self) -> [f32; 3] {
        [
            self.mid[0] + self.half[0],
            self.mid[1] + self.half[1],
            self.mid[2] + self.half[2],
        ]
    }

    pub const CLEARED: Self = Self {
        mid: [0.0; 3],
        half: [-131072.0; 3],
    };

    #[must_use]
    pub fn is_cleared(self) -> bool {
        self.mid == Self::CLEARED.mid && self.half == Self::CLEARED.half
    }

    #[must_use]
    pub fn negative_half_axis(self) -> Option<usize> {
        let mut axis = 0;
        while axis < 3 {
            if self.half[axis] < 0.0 {
                return Some(axis);
            }
            axis += 1;
        }
        None
    }
}

#[must_use]
pub fn bounds_from_origin_axis(origin: [f32; 3], axis: [[f32; 3]; 3], local: Bounds) -> Bounds {
    let m = local.mid();
    let h = local.half();
    Bounds::from_mid_half(
        [
            origin[0] + m[0] * axis[0][0] + m[1] * axis[1][0] + m[2] * axis[2][0],
            origin[1] + m[0] * axis[0][1] + m[1] * axis[1][1] + m[2] * axis[2][1],
            origin[2] + m[0] * axis[0][2] + m[1] * axis[1][2] + m[2] * axis[2][2],
        ],
        [
            h[0] * abs(axis[0][0]) + h[1] * abs(axis[1][0]) + h[2] * abs(axis[2][0]),
            h[0] * abs(axis[0][1]) + h[1] * abs(axis[1][1]) + h[2] * abs(axis[2][1]),
            h[0] * abs(axis[0][2]) + h[1] * abs(axis[1][2]) + h[2] * abs(axis[2][2]),
        ],
    )
}

#[inline]
fn abs(v: f32) -> f32 {
    if v < 0.0 { -v } else { v }
}

pub const AABB_NODE_STRIDE: usize = 44;

pub const MAX_CLIP_PLANES: usize = 16;

#[derive(Clone, Copy, Debug)]
pub struct AabbNodeView {
    pub bounds: Bounds,

    pub child_count: u16,

    pub children_offset: i32,

    pub start_surf: u16,

    pub surface_count: u16,

    pub start_surf_no_decal: u16,

    pub surface_count_no_decal: u16,

    pub smodel_index_start: u32,

    pub smodel_index_count: u16,
}

#[derive(Clone, Copy, Debug)]
pub struct AabbTreeCull<'a> {
    pub nodes: &'a [AabbNodeView],

    pub smodel_indexes: &'a [u16],

    pub sorted_surf_index: &'a [u16],

    pub surfaces_bounds: &'a [Bounds],

    pub smodel_bounds: &'a [Bounds],

    pub draw_decals: bool,
}

#[inline]
fn node_span(tree: &AabbTreeCull<'_>, node: &AabbNodeView) -> (u16, u16) {
    if tree.draw_decals {
        (node.start_surf, node.surface_count)
    } else {
        (node.start_surf_no_decal, node.surface_count_no_decal)
    }
}

pub struct DpvsVisData<'a> {
    pub surface_vis: &'a mut [u8],
    pub smodel_vis: &'a mut [u8],
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct AabbCullStats {
    pub nodes_visited: u32,

    pub nodes_rejected: u32,

    pub nodes_accepted_whole: u32,

    pub planes_dropped: u32,
    pub surfaces_admitted: u32,
    pub surfaces_rejected_by_bounds: u32,

    pub surfaces_admitted_unbounded: u32,
    pub smodels_admitted: u32,
    pub smodels_rejected_by_bounds: u32,

    pub smodels_admitted_unbounded: u32,

    pub clip_planes_overflow: u32,
}

#[inline]
fn plane_dist_radius(bounds: Bounds, plane: [f32; 4]) -> (f32, f32) {
    let (mid, half) = (bounds.mid(), bounds.half());
    let dist = plane[0] * mid[0] + plane[1] * mid[1] + plane[2] * mid[2] + plane[3];
    let radius = half[0] * abs(plane[0]) + half[1] * abs(plane[1]) + half[2] * abs(plane[2]);
    (dist, radius)
}

#[inline]
fn bounds_outside(bounds: Bounds, plane: [f32; 4]) -> bool {
    if bounds.is_cleared() {
        return true;
    }
    let (dist, radius) = plane_dist_radius(bounds, plane);
    dist + radius <= 0.0
}

#[inline]
fn bounds_straddle(bounds: Bounds, plane: [f32; 4]) -> bool {
    let (dist, radius) = plane_dist_radius(bounds, plane);
    dist - radius < 0.0
}

#[inline]
pub fn bounds_culled(bounds: Bounds, planes: &[[f32; 4]]) -> bool {
    planes.iter().any(|&plane| bounds_outside(bounds, plane))
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct SkyCullStats {
    pub already_visible: u32,
    pub admitted: u32,
    pub rejected_by_bounds: u32,

    pub admitted_unbounded: u32,

    pub missing_slot: u32,
}

pub fn add_sky_surfaces_dpvs(
    sky_start_surfs: &[u32],
    surfaces_bounds: &[Bounds],
    planes_without_far: &[[f32; 4]],
    surface_vis: &mut [u8],
) -> SkyCullStats {
    let mut stats = SkyCullStats::default();
    for &surf in sky_start_surfs {
        let surf = surf as usize;
        let Some(slot) = surface_vis.get_mut(surf) else {
            stats.missing_slot = stats.missing_slot.saturating_add(1);
            continue;
        };
        if *slot != 0 {
            stats.already_visible = stats.already_visible.saturating_add(1);
            continue;
        }
        match surfaces_bounds.get(surf) {
            Some(&bounds) if !planes_without_far.is_empty() => {
                if bounds_culled(bounds, planes_without_far) {
                    stats.rejected_by_bounds = stats.rejected_by_bounds.saturating_add(1);
                    continue;
                }
                stats.admitted = stats.admitted.saturating_add(1);
            }
            Some(_) => stats.admitted = stats.admitted.saturating_add(1),
            None => stats.admitted_unbounded = stats.admitted_unbounded.saturating_add(1),
        }
        *slot = 1;
    }
    stats
}

pub fn add_aabb_tree_surfaces_in_frustum(
    tree: &AabbTreeCull<'_>,
    clip_planes: &[[f32; 4]],
    out: &mut DpvsVisData<'_>,
    stats: &mut AabbCullStats,
) {
    if tree.nodes.is_empty() {
        return;
    }
    let mut root = [[0.0f32; 4]; MAX_CLIP_PLANES];
    let count = clip_planes.len().min(MAX_CLIP_PLANES);
    if clip_planes.len() > MAX_CLIP_PLANES {
        stats.clip_planes_overflow += (clip_planes.len() - MAX_CLIP_PLANES) as u32;
    }
    root[..count].copy_from_slice(&clip_planes[..count]);
    add_node(tree, 0, &root[..count], out, stats);
}

pub struct AabbSphereBits<'a> {
    pub nodes: &'a [AabbNodeView],
    pub origin: [f32; 3],
    pub radius_sq: f32,
    pub surf_bits: &'a mut [u32],
    pub surf_bit_count: u32,
    pub smodel_indexes: &'a [u16],
    pub smodel_bits: Option<&'a mut [u32]>,
    pub smodel_bit_count: u32,
}

pub fn aabb_tree_set_sorted_span_bits(q: &mut AabbSphereBits<'_>) -> u32 {
    let before: u32 = q.surf_bits.iter().map(|w| w.count_ones()).sum();
    if q.nodes.is_empty() {
        return 0;
    }
    set_span_bits_r(q, 0, 0);
    let after: u32 = q.surf_bits.iter().map(|w| w.count_ones()).sum();
    after.saturating_sub(before)
}

fn sphere_hits_aabb(origin: [f32; 3], radius_sq: f32, mid: [f32; 3], half: [f32; 3]) -> bool {
    let mut acc = 0.0f32;
    let mut i = 0;
    while i < 3 {
        let mut a = abs(origin[i] - mid[i]) - half[i];
        if a < 0.0 {
            a = 0.0;
        }
        acc += a * a;
        i += 1;
    }
    acc <= radius_sq
}

fn set_span_bits_r(q: &mut AabbSphereBits<'_>, index: usize, depth: usize) {
    if depth > q.nodes.len() {
        return;
    }
    let Some(node) = q.nodes.get(index).copied() else {
        return;
    };
    if !sphere_hits_aabb(q.origin, q.radius_sq, node.bounds.mid(), node.bounds.half()) {
        return;
    }
    if node.child_count == 0 {
        let start = usize::from(node.start_surf);
        let count = usize::from(node.surface_count);
        let cap = q.surf_bit_count as usize;
        let mut i = 0;
        while i < count {
            let slot = start + i;
            if slot < cap {
                crate::vis::msb_set(q.surf_bits, slot);
            }
            i += 1;
        }
        if let Some(smodel_bits) = q.smodel_bits.as_mut() {
            let n = usize::from(node.smodel_index_count);
            let base = node.smodel_index_start as usize;
            let cap = q.smodel_bit_count as usize;
            let mut k = 0;
            while k < n {
                if let Some(&id) = q.smodel_indexes.get(base + k) {
                    let id = usize::from(id);
                    if id < cap {
                        crate::vis::msb_set(smodel_bits, id);
                    }
                }
                k += 1;
            }
        }
        return;
    }
    if node.children_offset <= 0 {
        return;
    }
    let child0 = index + (node.children_offset as usize) / AABB_NODE_STRIDE;
    let mut c = 0usize;
    while c < usize::from(node.child_count) {
        set_span_bits_r(q, child0 + c, depth + 1);
        c += 1;
    }
}

fn add_node(
    tree: &AabbTreeCull<'_>,
    index: usize,
    clip_planes: &[[f32; 4]],
    out: &mut DpvsVisData<'_>,
    stats: &mut AabbCullStats,
) {
    let Some(node) = tree.nodes.get(index) else {
        return;
    };
    stats.nodes_visited += 1;
    let bounds = node.bounds;

    let mut child = [[0.0f32; 4]; MAX_CLIP_PLANES];
    let mut child_count = 0usize;
    for &plane in clip_planes {
        if bounds_outside(bounds, plane) {
            stats.nodes_rejected += 1;
            return;
        }
        if bounds_straddle(bounds, plane) {
            child[child_count] = plane;
            child_count += 1;
        } else {
            stats.planes_dropped += 1;
        }
    }
    let child = &child[..child_count];

    if child.is_empty() {
        stats.nodes_accepted_whole += 1;
        let (start, count) = node_span(tree, node);
        admit_span(tree, start, count, &[], out, stats);
        admit_smodels(tree, node, &[], out, stats);
        return;
    }

    if node.child_count != 0 {
        if node.children_offset > 0 {
            let child0 = index + (node.children_offset as usize) / AABB_NODE_STRIDE;
            for c in 0..usize::from(node.child_count) {
                add_node(tree, child0 + c, child, out, stats);
            }
        }
        return;
    }

    let (start, count) = node_span(tree, node);
    admit_span(tree, start, count, child, out, stats);
    admit_smodels(tree, node, child, out, stats);
}

fn admit_span(
    tree: &AabbTreeCull<'_>,
    start: u16,
    count: u16,
    clip_planes: &[[f32; 4]],
    out: &mut DpvsVisData<'_>,
    stats: &mut AabbCullStats,
) {
    let start = usize::from(start);
    for i in 0..usize::from(count) {
        let Some(&surf) = tree.sorted_surf_index.get(start + i) else {
            break;
        };
        let surf = usize::from(surf);
        let Some(slot) = out.surface_vis.get(surf) else {
            continue;
        };
        if *slot != 0 {
            continue;
        }
        match tree.surfaces_bounds.get(surf) {
            Some(&bounds) if !clip_planes.is_empty() => {
                if bounds_culled(bounds, clip_planes) {
                    stats.surfaces_rejected_by_bounds += 1;
                    continue;
                }
                stats.surfaces_admitted += 1;
            }
            Some(_) => stats.surfaces_admitted += 1,

            None => stats.surfaces_admitted_unbounded += 1,
        }
        if let Some(slot) = out.surface_vis.get_mut(surf) {
            *slot = 1;
        }
    }
}

fn admit_smodels(
    tree: &AabbTreeCull<'_>,
    node: &AabbNodeView,
    clip_planes: &[[f32; 4]],
    out: &mut DpvsVisData<'_>,
    stats: &mut AabbCullStats,
) {
    let start = node.smodel_index_start as usize;
    for i in 0..usize::from(node.smodel_index_count) {
        let Some(&smodel) = tree.smodel_indexes.get(start + i) else {
            break;
        };
        let smodel = usize::from(smodel);
        let Some(slot) = out.smodel_vis.get(smodel) else {
            continue;
        };
        if *slot != 0 {
            continue;
        }
        match tree.smodel_bounds.get(smodel) {
            Some(&bounds) if !clip_planes.is_empty() => {
                if bounds_culled(bounds, clip_planes) {
                    stats.smodels_rejected_by_bounds += 1;
                    continue;
                }
                stats.smodels_admitted += 1;
            }
            Some(_) => stats.smodels_admitted += 1,
            None => stats.smodels_admitted_unbounded += 1,
        }
        if let Some(slot) = out.smodel_vis.get_mut(smodel) {
            *slot = 1;
        }
    }
}

pub fn admit_cell_root_span(
    sorted_surf_index: &[u16],
    start: u16,
    count: u16,
    out: &mut DpvsVisData<'_>,
    stats: &mut AabbCullStats,
) {
    let start = usize::from(start);
    for i in 0..usize::from(count) {
        let Some(&surf) = sorted_surf_index.get(start + i) else {
            break;
        };
        let surf = usize::from(surf);
        if let Some(slot) = out.surface_vis.get_mut(surf)
            && *slot == 0
        {
            *slot = 1;
            stats.surfaces_admitted_unbounded += 1;
        }
    }
}
