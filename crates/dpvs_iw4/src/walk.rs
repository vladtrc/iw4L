use crate::aabb::MAX_CLIP_PLANES;
use crate::admit::portal_admits_eye;
use crate::chop::chop_portal;
use crate::clip::{PortalBevels, portal_clip_planes, portal_vert_hull_uv};
use crate::convex_hull::{PortalHullPoints, add_vert_to_portal_hull_points, com_convex_hull};
use crate::portal::{portal_behind_any_plane, portal_behind_plane, portal_eye_dist};
use crate::portal_heap::{
    HULL_POOL_NULL, PortalHeapNode, QUEUED_PORTAL_POOL, furthest_point_on_winding, heap_pop,
    heap_push,
};
use crate::stats::WalkStats;
use crate::vis::VisBits;

#[derive(Clone, Copy, Debug)]
pub struct CellClipPlanes {
    pub planes: [[f32; 4]; MAX_CLIP_PLANES],
    pub plane_count: u8,
    pub frustum_plane_count: u8,
}

impl CellClipPlanes {
    pub const EMPTY: Self = Self {
        planes: [[0.0; 4]; MAX_CLIP_PLANES],
        plane_count: 0,
        frustum_plane_count: 0,
    };

    #[must_use]
    pub fn as_slice(&self) -> &[[f32; 4]] {
        let n = usize::from(self.plane_count).min(MAX_CLIP_PLANES);
        &self.planes[..n]
    }
}

pub struct WalkScratch {
    pub queue: [Queued; QUEUED_PORTAL_POOL],
    pub heap: [PortalHeapNode; QUEUED_PORTAL_POOL],
    pub heap_n: usize,

    pub free_head: u16,

    pub next_free: [u16; QUEUED_PORTAL_POOL],
    pub path: [bool; 512],
    pub buf_a: [[f32; 3]; 128],
    pub buf_b: [[f32; 3]; 128],
}

impl WalkScratch {
    pub const fn new() -> Self {
        Self {
            queue: [Queued::EMPTY; QUEUED_PORTAL_POOL],
            heap: [PortalHeapNode { slot: 0, dist: 0.0 }; QUEUED_PORTAL_POOL],
            heap_n: 0,
            free_head: 0,
            next_free: [0; QUEUED_PORTAL_POOL],
            path: [false; 512],
            buf_a: [[0.0; 3]; 128],
            buf_b: [[0.0; 3]; 128],
        }
    }
}

impl Default for WalkScratch {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Clone, Copy, Debug)]
pub struct PortalView<'a> {
    pub plane: [f32; 4],

    pub neighbor: u16,

    pub vertices: &'a [[f32; 3]],

    pub hull_axis: Option<[[f32; 3]; 2]>,
}

pub struct CellPortalGraph<'a> {
    pub portals: &'a [&'a [PortalView<'a>]],
}

#[derive(Clone, Copy, Debug)]
pub struct Queued {
    pub cell: u16,
    pub parent_plane: [f32; 4],
    pub has_parent: bool,

    pub clip_n: u8,
    pub clip: [[f32; 4]; MAX_CLIP_PLANES],
    pub hull: PortalHullPoints,
    pub hull_axis: Option<[[f32; 3]; 2]>,
    pub from_cell: u16,
    pub edge: u16,

    pub parent_idx: u16,

    pub ancestor_bits: [u64; 8],
}

impl Queued {
    pub(crate) const EMPTY: Self = Self {
        cell: 0,
        parent_plane: [0.0; 4],
        has_parent: false,
        clip_n: 0,
        clip: [[0.0; 4]; MAX_CLIP_PLANES],
        hull: PortalHullPoints::EMPTY,
        hull_axis: None,
        from_cell: 0,
        edge: 0,
        parent_idx: u16::MAX,
        ancestor_bits: [0; 8],
    };
}

#[inline]
pub fn effective_portal_walk_limit(limit: u32) -> Option<u32> {
    if limit == 0 { None } else { Some(limit) }
}

fn queued_clip_planes<'a>(item: &'a Queued, camera: &'a [[f32; 4]]) -> &'a [[f32; 4]] {
    let n = usize::from(item.clip_n);
    if n == 0 {
        camera
    } else {
        &item.clip[..n.min(MAX_CLIP_PLANES)]
    }
}

fn record_cell_clip(item: &Queued, camera: &[[f32; 4]], slot: &mut CellClipPlanes) {
    if item.clip_n == 0 {
        let n = camera.len().min(MAX_CLIP_PLANES);
        slot.planes[..n].copy_from_slice(&camera[..n]);
        slot.plane_count = n as u8;
        slot.frustum_plane_count = n as u8;
    } else {
        let n = usize::from(item.clip_n).min(MAX_CLIP_PLANES);
        slot.planes[..n].copy_from_slice(&item.clip[..n]);
        slot.plane_count = n as u8;
        slot.frustum_plane_count = 0;
    }
}

pub(crate) fn reset_hull_pool(scratch: &mut WalkScratch) {
    scratch.heap_n = 0;
    scratch.free_head = 0;
    let last = QUEUED_PORTAL_POOL - 1;
    let mut i = 0;
    while i < last {
        scratch.next_free[i] = (i + 1) as u16;
        i += 1;
    }
    scratch.next_free[last] = HULL_POOL_NULL;
}

pub(crate) fn alloc_hull_slot(scratch: &mut WalkScratch) -> Option<usize> {
    let head = scratch.free_head;
    if head == HULL_POOL_NULL {
        return None;
    }
    let slot = usize::from(head);
    scratch.free_head = scratch.next_free[slot];
    Some(slot)
}

pub(crate) fn free_hull_slot(scratch: &mut WalkScratch, slot: usize) {
    scratch.next_free[slot] = scratch.free_head;
    scratch.free_head = slot as u16;
}

fn ancestor_set(bits: &mut [u64; 8], cell: usize) {
    if cell < 512 {
        bits[cell / 64] |= 1u64 << (cell % 64);
    }
}

pub(crate) fn mark_seed_ancestor(q: &mut Queued, cell: usize) {
    ancestor_set(&mut q.ancestor_bits, cell);
}

fn ancestor_get(bits: &[u64; 8], cell: usize) -> bool {
    cell < 512 && (bits[cell / 64] & (1u64 << (cell % 64))) != 0
}

pub(crate) fn child_ancestor_bits(parent: &Queued, child_cell: u16) -> [u64; 8] {
    let mut bits = parent.ancestor_bits;
    ancestor_set(&mut bits, usize::from(child_cell));
    bits
}

pub(crate) fn cell_is_ancestor(item: &Queued, neighbor: usize) -> bool {
    ancestor_get(&item.ancestor_bits, neighbor)
}

pub(crate) fn find_queued_portal(
    scratch: &WalkScratch,
    from_cell: u16,
    edge: u16,
) -> Option<usize> {
    (0..scratch.heap_n).find_map(|i| {
        let slot = usize::from(scratch.heap[i].slot);
        let q = scratch.queue[slot];
        (q.from_cell == from_cell && q.edge == edge && q.hull_axis.is_some()).then_some(slot)
    })
}

pub(crate) fn try_enqueue(
    scratch: &mut WalkScratch,
    q: Queued,
    verts: &[[f32; 3]],
    view_plane: [f32; 4],
    stats: &mut WalkStats,
) {
    if let Some(slot) = alloc_hull_slot(scratch) {
        let dist = furthest_point_on_winding(view_plane, verts);
        scratch.queue[slot] = q;
        if !heap_push(
            &mut scratch.heap,
            &mut scratch.heap_n,
            PortalHeapNode {
                slot: slot as u16,
                dist,
            },
        ) {
            free_hull_slot(scratch, slot);
            stats.walk_limit_hits += 1;
            return;
        }
        stats.enqueued += 1;
    } else {
        stats.walk_limit_hits += 1;
    }
}

pub(crate) fn add_winding_to_hull(q: &mut Queued, verts: &[[f32; 3]]) -> bool {
    let Some(axis) = q.hull_axis else {
        return true;
    };
    for v in verts {
        if !add_vert_to_portal_hull_points(&mut q.hull, portal_vert_hull_uv(*v, axis)) {
            return false;
        }
    }
    true
}

fn finalize_queued_hull_clip(
    item: &mut Queued,
    eye: [f32; 3],
    near: Option<[f32; 4]>,
    far: Option<[f32; 4]>,
    bevels: Option<&PortalBevels>,
) -> bool {
    let Some(axis) = item.hull_axis else {
        return true;
    };
    let n = usize::from(item.hull.count);
    if n < 3 {
        return false;
    }
    let mut compact = [[0.0f32; 2]; crate::COM_CONVEX_HULL_MAX];
    let h = com_convex_hull(&item.hull.points[..n], &mut compact);
    if h == 0 {
        return false;
    }
    let origin = crate::portal_hull_origin(item.parent_plane);
    let mut winding = [[0.0f32; 3]; crate::COM_CONVEX_HULL_MAX];
    for i in 0..h {
        winding[i] = crate::portal_hull_point(origin, axis, compact[i][0], compact[i][1]);
    }
    let clip_n = portal_clip_planes(&winding[..h], eye, false, near, far, &mut item.clip, bevels);
    if clip_n == 0 {
        return false;
    }
    item.clip_n = clip_n as u8;
    true
}

pub(crate) fn finalize_queued_hull_no_frustum(item: &mut Queued, view_dir: [f32; 3]) -> bool {
    let Some(axis) = item.hull_axis else {
        return true;
    };
    let n = usize::from(item.hull.count);
    if n < 3 {
        return false;
    }
    let mut compact = [[0.0f32; 2]; crate::COM_CONVEX_HULL_MAX];
    let h = com_convex_hull(&item.hull.points[..n], &mut compact);
    if h == 0 {
        return false;
    }
    let origin = crate::portal_hull_origin(item.parent_plane);
    let mut winding = [[0.0f32; 3]; crate::COM_CONVEX_HULL_MAX];
    for i in 0..h {
        winding[i] = crate::portal_hull_point(origin, axis, compact[i][0], compact[i][1]);
    }
    let clip_n =
        crate::portal_clip_planes_no_frustum(&winding[..h], view_dir, true, &mut item.clip);
    item.clip_n = clip_n.min(255) as u8;
    true
}

pub fn visit_cells(
    graph: &CellPortalGraph<'_>,
    camera_cell: usize,
    eye: [f32; 3],
    view_plane: [f32; 4],
    clip_planes: &[[f32; 4]],
    walk_limit: u32,
    out: &mut VisBits<'_>,
    scratch: &mut WalkScratch,
    mut cell_clips: Option<&mut [CellClipPlanes]>,
    bevels: Option<&PortalBevels>,
) -> WalkStats {
    let mut stats = WalkStats::default();
    let near = clip_planes.first().copied();
    let far = clip_planes.get(1).copied();
    out.clear();
    for p in scratch.path.iter_mut() {
        *p = false;
    }
    if graph.portals.is_empty() || camera_cell >= graph.portals.len() {
        return stats;
    }
    if camera_cell >= scratch.path.len() {
        return stats;
    }

    reset_hull_pool(scratch);
    scratch.path[camera_cell] = true;
    let mut seed = Queued {
        cell: camera_cell as u16,
        parent_plane: [0.0; 4],
        has_parent: false,
        clip_n: 0,
        clip: [[0.0; 4]; MAX_CLIP_PLANES],
        ..Queued::EMPTY
    };
    ancestor_set(&mut seed.ancestor_bits, camera_cell);
    let limit = effective_portal_walk_limit(walk_limit);

    let mut popped = 0u32;
    let mut first = true;

    loop {
        let (mut item, recycle) = if first {
            first = false;
            (seed, None)
        } else {
            let Some(node) = heap_pop(&mut scratch.heap, &mut scratch.heap_n) else {
                break;
            };
            popped += 1;
            if limit == Some(popped) {
                stats.walk_limit_hits += 1;
                break;
            }
            let slot = usize::from(node.slot);
            (scratch.queue[slot], Some(slot))
        };
        if recycle.is_some() {
            if item.hull_axis.is_some() {
                if !finalize_queued_hull_clip(&mut item, eye, near, far, bevels) {
                    stats.hull_fail += 1;
                    if let Some(slot) = recycle {
                        free_hull_slot(scratch, slot);
                    }
                    continue;
                }
                if usize::from(item.hull.count) >= 11 && bevels.is_none() {
                    stats.child_clip_bevel_unread += 1;
                }
            }
        }
        let cell = usize::from(item.cell);
        if !out.get(cell) {
            out.set(cell);
            stats.cells_visited += 1;
            if let Some(slots) = cell_clips.as_mut() {
                if let Some(slot) = slots.get_mut(cell) {
                    record_cell_clip(&item, clip_planes, slot);
                }
            }
        }
        let cell_planes = queued_clip_planes(&item, clip_planes);
        let Some(edges) = graph.portals.get(cell).copied() else {
            if let Some(slot) = recycle {
                free_hull_slot(scratch, slot);
            }
            continue;
        };
        for (edge_i, edge) in edges.iter().enumerate() {
            stats.portals_seen += 1;
            let neighbor = usize::from(edge.neighbor);
            if neighbor >= graph.portals.len() || neighbor >= scratch.path.len() {
                continue;
            }
            if cell_is_ancestor(&item, neighbor) {
                stats.skip_ancestor += 1;
                continue;
            }
            let dist = portal_eye_dist(edge.plane, eye);
            if dist > 0.0 {
                stats.skip_facing += 1;
                continue;
            }
            if portal_behind_any_plane(edge.vertices, cell_planes) {
                stats.skip_clip += 1;
                continue;
            }
            let mut wind = [[0.0f32; 3]; crate::COM_CONVEX_HULL_MAX];
            let wn = if item.has_parent {
                if portal_behind_plane(item.parent_plane, edge.vertices) {
                    stats.chop_empty += 1;
                    continue;
                }
                let chopped = chop_portal(
                    edge.vertices,
                    Some(item.parent_plane),
                    cell_planes,
                    &mut scratch.buf_a,
                    &mut scratch.buf_b,
                );
                if chopped < 3 {
                    stats.chop_empty += 1;
                    continue;
                }
                let n = chopped.min(crate::COM_CONVEX_HULL_MAX);
                wind[..n].copy_from_slice(&scratch.buf_a[..n]);
                n
            } else {
                let n = edge.vertices.len().min(crate::COM_CONVEX_HULL_MAX);
                wind[..n].copy_from_slice(&edge.vertices[..n]);
                n
            };
            if !portal_admits_eye(edge.plane, &wind[..wn], eye) {
                stats.hull_fail += 1;
                continue;
            }
            if dist > -0.125 {
                stats.stepped_through += 1;
            }
            if let Some(axis) = edge.hull_axis {
                let from = cell as u16;
                let eidx = edge_i as u16;
                if let Some(qi) = find_queued_portal(scratch, from, eidx) {
                    stats.hull_union += 1;
                    if !add_winding_to_hull(&mut scratch.queue[qi], &wind[..wn]) {
                        stats.hull_overflow += 1;
                    }
                    continue;
                }
                let mut q = Queued {
                    cell: edge.neighbor,
                    parent_plane: edge.plane,
                    has_parent: true,
                    clip_n: 0,
                    clip: [[0.0; 4]; MAX_CLIP_PLANES],
                    hull: PortalHullPoints::EMPTY,
                    hull_axis: Some(axis),
                    from_cell: from,
                    edge: eidx,
                    parent_idx: u16::MAX,
                    ancestor_bits: child_ancestor_bits(&item, edge.neighbor),
                };
                if !add_winding_to_hull(&mut q, &wind[..wn]) {
                    stats.hull_overflow += 1;
                    continue;
                }
                try_enqueue(scratch, q, edge.vertices, view_plane, &mut stats);
                continue;
            }
            let mut child_clip = [[0.0f32; 4]; MAX_CLIP_PLANES];
            let clip_n =
                portal_clip_planes(&wind[..wn], eye, false, near, far, &mut child_clip, bevels);
            if clip_n == 0 {
                continue;
            }
            if wn >= 11 && bevels.is_none() {
                stats.child_clip_bevel_unread += 1;
            }
            try_enqueue(
                scratch,
                Queued {
                    cell: edge.neighbor,
                    parent_plane: edge.plane,
                    has_parent: true,
                    clip_n: clip_n as u8,
                    clip: child_clip,
                    parent_idx: u16::MAX,
                    ancestor_bits: child_ancestor_bits(&item, edge.neighbor),
                    ..Queued::EMPTY
                },
                edge.vertices,
                view_plane,
                &mut stats,
            );
        }
        if let Some(slot) = recycle {
            free_hull_slot(scratch, slot);
        }
    }

    for p in scratch.path.iter_mut() {
        *p = false;
    }
    stats
}

pub fn visit_cells_facing(
    graph: &CellPortalGraph<'_>,
    camera_cell: usize,
    eye: [f32; 3],
    out: &mut VisBits<'_>,
    queue: &mut [usize],
) -> WalkStats {
    let mut stats = WalkStats::default();
    out.clear();
    if graph.portals.is_empty() || camera_cell >= graph.portals.len() || queue.is_empty() {
        return stats;
    }

    out.set(camera_cell);
    stats.cells_visited = 1;
    queue[0] = camera_cell;
    let mut head = 0usize;
    let mut tail = 1usize;

    while head < tail {
        let cell = queue[head];
        head += 1;
        let Some(edges) = graph.portals.get(cell).copied() else {
            continue;
        };
        for edge in edges {
            stats.portals_seen += 1;
            let neighbor = usize::from(edge.neighbor);
            if neighbor >= graph.portals.len() {
                continue;
            }
            if out.get(neighbor) {
                stats.skip_ancestor += 1;
                continue;
            }
            if portal_eye_dist(edge.plane, eye) > 0.0 {
                stats.skip_facing += 1;
                continue;
            }
            stats.enqueued += 1;
            out.set(neighbor);
            stats.cells_visited += 1;
            if tail < queue.len() {
                queue[tail] = neighbor;
                tail += 1;
            } else {
                stats.walk_limit_hits += 1;
            }
        }
    }
    stats
}
