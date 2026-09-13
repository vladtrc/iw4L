use crate::chop::chop_portal;
use crate::convex_hull::PortalHullPoints;
use crate::portal::{portal_behind_any_plane, portal_behind_plane, portal_eye_dist};
use crate::portal_heap::heap_pop;
use crate::stats::WalkStats;
use crate::vis::{msb_get, msb_set, words_for_bits};
use crate::walk::{
    CellPortalGraph, Queued, WalkScratch, add_winding_to_hull, cell_is_ancestor,
    finalize_queued_hull_no_frustum, find_queued_portal, free_hull_slot, reset_hull_pool,
    try_enqueue,
};

#[inline]
pub fn cell_caster_row_words(cell_count: usize) -> usize {
    words_for_bits(cell_count.max(1))
}

#[inline]
pub fn cell_caster_matrix_words(cell_count: usize) -> usize {
    cell_count.saturating_mul(cell_caster_row_words(cell_count))
}

#[inline]
fn portal_dir_faces_away(plane: [f32; 4], view_dir: [f32; 3]) -> bool {
    portal_eye_dist([plane[0], plane[1], plane[2], 0.0], view_dir) > 0.0
}

pub fn visit_portals_no_frustum(
    graph: &CellPortalGraph<'_>,
    seed: usize,
    view_dir: [f32; 3],
    out_msb: &mut [u32],
    scratch: &mut WalkScratch,
) -> WalkStats {
    let mut stats = WalkStats::default();
    for w in out_msb.iter_mut() {
        *w = 0;
    }
    for p in scratch.path.iter_mut() {
        *p = false;
    }
    if graph.portals.is_empty() || seed >= graph.portals.len() {
        return stats;
    }
    if seed >= scratch.path.len() {
        return stats;
    }

    scratch.path[seed] = true;
    msb_set(out_msb, seed);
    stats.cells_visited = 1;
    reset_hull_pool(scratch);
    let mut seed_q = Queued {
        cell: seed as u16,
        parent_plane: [0.0; 4],
        has_parent: false,
        clip_n: 0,
        clip: [[0.0; 4]; crate::aabb::MAX_CLIP_PLANES],
        ..Queued::EMPTY
    };
    crate::walk::mark_seed_ancestor(&mut seed_q, seed);
    let view_plane = [view_dir[0], view_dir[1], view_dir[2], 0.0];
    let mut first = true;

    loop {
        let (mut item, recycle) = if first {
            first = false;
            (seed_q, None)
        } else {
            let Some(node) = heap_pop(&mut scratch.heap, &mut scratch.heap_n) else {
                break;
            };
            let slot = usize::from(node.slot);
            (scratch.queue[slot], Some(slot))
        };
        if recycle.is_some() {
            if item.hull_axis.is_some() && !finalize_queued_hull_no_frustum(&mut item, view_dir) {
                stats.hull_fail += 1;
                if let Some(slot) = recycle {
                    free_hull_slot(scratch, slot);
                }
                continue;
            }
            if !msb_get(out_msb, usize::from(item.cell)) {
                msb_set(out_msb, usize::from(item.cell));
                stats.cells_visited = stats.cells_visited.saturating_add(1);
            }
        }
        let cell = usize::from(item.cell);
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
            if portal_dir_faces_away(edge.plane, view_dir) {
                stats.skip_facing += 1;
                continue;
            }
            let parent_clip: &[[f32; 4]] = if item.clip_n > 0 {
                &item.clip[..usize::from(item.clip_n).min(item.clip.len())]
            } else {
                &[]
            };
            if !parent_clip.is_empty() && portal_behind_any_plane(edge.vertices, parent_clip) {
                stats.chop_empty += 1;
                continue;
            }
            let mut wind = [[0.0f32; 3]; crate::COM_CONVEX_HULL_MAX];
            let wn = if item.has_parent || !parent_clip.is_empty() {
                if item.has_parent && portal_behind_plane(item.parent_plane, edge.vertices) {
                    stats.chop_empty += 1;
                    continue;
                }
                let chopped = chop_portal(
                    edge.vertices,
                    if item.has_parent {
                        Some(item.parent_plane)
                    } else {
                        None
                    },
                    parent_clip,
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
                    clip: [[0.0; 4]; crate::aabb::MAX_CLIP_PLANES],
                    hull: PortalHullPoints::EMPTY,
                    hull_axis: Some(axis),
                    from_cell: from,
                    edge: eidx,
                    parent_idx: u16::MAX,
                    ancestor_bits: crate::walk::child_ancestor_bits(&item, edge.neighbor),
                };
                if !add_winding_to_hull(&mut q, &wind[..wn]) {
                    stats.hull_overflow += 1;
                    continue;
                }
                try_enqueue(scratch, q, edge.vertices, view_plane, &mut stats);
                continue;
            }
            stats.child_planes_unported += 1;
            try_enqueue(
                scratch,
                Queued {
                    cell: edge.neighbor,
                    parent_plane: edge.plane,
                    has_parent: true,
                    clip_n: 0,
                    clip: [[0.0; 4]; crate::aabb::MAX_CLIP_PLANES],
                    parent_idx: u16::MAX,
                    ancestor_bits: crate::walk::child_ancestor_bits(&item, edge.neighbor),
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

pub fn generate_shadow_map_caster_cells(
    graph: &CellPortalGraph<'_>,
    view_dir: [f32; 3],
    out: &mut [u32],
) {
    let cell_count = graph.portals.len();
    let row = cell_caster_row_words(cell_count);
    let need = cell_caster_matrix_words(cell_count);
    for w in out.iter_mut() {
        *w = 0;
    }
    if need == 0 || out.len() < need {
        return;
    }
    let mut scratch = WalkScratch::new();
    for seed in 0..cell_count {
        let start = seed * row;
        visit_portals_no_frustum(
            graph,
            seed,
            view_dir,
            &mut out[start..start + row],
            &mut scratch,
        );
    }
}

pub fn or_caster_rows_for_visible(
    matrix: &[u32],
    cell_count: usize,
    visible_lsb: &[u32],
    vis_all: bool,
    out_msb: &mut [u32],
) -> u32 {
    let row = cell_caster_row_words(cell_count);
    for w in out_msb.iter_mut() {
        *w = 0;
    }
    if row == 0 || matrix.len() < cell_count * row || out_msb.len() < row {
        return 0;
    }
    for seed in 0..cell_count {
        let visible = vis_all
            || visible_lsb
                .get(seed / 32)
                .is_some_and(|w| (*w & (1u32 << (seed % 32))) != 0);
        if !visible {
            continue;
        }
        let start = seed * row;
        for (dst, src) in out_msb.iter_mut().zip(matrix[start..start + row].iter()) {
            *dst |= *src;
        }
    }
    let mut n = 0u32;
    for cell in 0..cell_count {
        if msb_get(out_msb, cell) {
            n = n.saturating_add(1);
        }
    }
    n
}
