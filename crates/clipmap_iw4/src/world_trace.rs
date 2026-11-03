use alloc::vec::Vec;
use trace_iw4::{Trace, trace_box_into};

use crate::{
    BrushView, ClipLeaf, ClipMapRef, ClipMeshRef, TraceExtents, leaves_have_coll_aabb,
    trace_linear_with_glass, trace_through_mesh_into, walk_clip_tree,
};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ClipWorldWinner {
    #[default]
    Open,
    Brush,
    Mesh,
    StaticModel,
}

impl ClipWorldWinner {
    pub const fn as_dump(self) -> Option<&'static str> {
        match self {
            Self::Open => None,
            Self::Brush => Some("brush"),
            Self::Mesh => Some("mesh"),
            Self::StaticModel => Some("smodel"),
        }
    }
}

pub fn trace_brush_and_mesh<B: BrushView>(
    map: &ClipMapRef<'_, B>,
    mesh: &ClipMeshRef<'_>,
    ext: &TraceExtents,
    glass_is_solid: &dyn Fn(u16) -> bool,
) -> Trace {
    trace_brush_and_mesh_with_winner(map, mesh, ext, glass_is_solid).0
}

pub fn trace_brush_and_mesh_with_winner<B: BrushView>(
    map: &ClipMapRef<'_, B>,
    mesh: &ClipMeshRef<'_>,
    ext: &TraceExtents,
    glass_is_solid: &dyn Fn(u16) -> bool,
) -> (Trace, ClipWorldWinner) {
    let mut best = Trace {
        fraction: 1.0,
        endpos: ext.end,
        ..Trace::default()
    };
    let mut winner = ClipWorldWinner::Open;
    let no_brushes = map.brushes.is_empty();
    let no_mesh = mesh.tri_indices.len() < 3;
    if no_brushes && no_mesh {
        return (best, winner);
    }

    if map.nodes.is_empty() || map.leaves.is_empty() {
        if !no_brushes {
            let brush = trace_linear_with_glass(map, ext, glass_is_solid);
            absorb_hit(&mut best, &brush, ClipWorldWinner::Brush, &mut winner);
        }
        if !no_mesh {
            let f0 = best.fraction;
            trace_through_mesh_into(mesh, ext, &mut best);
            if best.fraction < f0 {
                winner = ClipWorldWinner::Mesh;
            }
        }
        finalize_winner(&best, &mut winner);
        return (best, winner);
    }

    let has_leaf_aabbs = !no_mesh && leaves_have_coll_aabb(map.leaves);
    let frac = core::cell::Cell::new(1.0_f32);
    let mut scratch = Vec::new();
    walk_clip_tree(map, ext, &|| frac.get(), &mut |leaf| {
        if best.fraction == 0.0 {
            frac.set(0.0);
            return;
        }
        if !no_brushes {
            let f0 = best.fraction;
            trace_leaf_brushes_into(map, leaf, ext, glass_is_solid, &mut best);
            if best.fraction < f0 {
                winner = ClipWorldWinner::Brush;
            }
            if best.fraction == 0.0 {
                frac.set(0.0);
                return;
            }
        }
        if !no_mesh {
            let f0 = best.fraction;
            trace_leaf_mesh_into(mesh, leaf, ext, has_leaf_aabbs, &mut best, &mut scratch);
            if best.fraction < f0 {
                winner = ClipWorldWinner::Mesh;
            }
        }
        frac.set(best.fraction);
    });
    if !no_mesh {
        finish_mesh_forest_fallback(mesh, has_leaf_aabbs, ext, &mut best, &mut winner);
    }
    finalize_winner(&best, &mut winner);
    (best, winner)
}

pub fn trace_leaf_brushes_into<B: BrushView>(
    map: &ClipMapRef<'_, B>,
    leaf: &ClipLeaf,
    ext: &TraceExtents,
    glass_is_solid: &dyn Fn(u16) -> bool,
    best: &mut Trace,
) {
    let first = leaf.first_brush as usize;
    let last = first + leaf.num_brushes as usize;
    let Some(ids) = map.leafbrushes.get(first..last) else {
        return;
    };
    let selected = ids
        .iter()
        .filter_map(|&id| map.brushes.get(id as usize))
        .filter(|b| crate::brush_glass_allowed(*b, glass_is_solid))
        .map(|b| crate::brush_ref(b));
    let _ = trace_box_into(
        selected, ext.start, ext.end, ext.mins, ext.maxs, ext.mask, best,
    );
}

pub fn trace_leaf_mesh_into(
    mesh: &ClipMeshRef<'_>,
    leaf: &ClipLeaf,
    ext: &TraceExtents,
    has_leaf_aabbs: bool,
    best: &mut Trace,
    scratch: &mut Vec<u16>,
) {
    if !has_leaf_aabbs {
        return;
    }
    scratch.clear();
    for k in 0..leaf.coll_aabb_count {
        scratch.push(leaf.first_coll_aabb_index.saturating_add(k));
    }
    let staged = ClipMeshRef {
        verts: mesh.verts,
        tri_indices: mesh.tri_indices,
        tri_edge_is_walkable: mesh.tri_edge_is_walkable,
        tri_surface_flags: mesh.tri_surface_flags,
        tri_content_flags: mesh.tri_content_flags,
        aabb_trees: mesh.aabb_trees,
        borders: mesh.borders,
        partitions: mesh.partitions,
        aabb_roots: scratch.as_slice(),
    };
    trace_through_mesh_into(&staged, ext, best);
}

pub fn finish_mesh_forest_fallback(
    mesh: &ClipMeshRef<'_>,
    has_leaf_aabbs: bool,
    ext: &TraceExtents,
    best: &mut Trace,
    winner: &mut ClipWorldWinner,
) {
    if has_leaf_aabbs {
        return;
    }
    if mesh.tri_indices.len() < 3 {
        return;
    }
    let f0 = best.fraction;
    trace_through_mesh_into(mesh, ext, best);
    if best.fraction < f0 {
        *winner = ClipWorldWinner::Mesh;
    }
}

pub fn combine_brush_mesh(mut brush: Trace, mesh: Option<Trace>) -> (Trace, ClipWorldWinner) {
    let mut winner = if brush.fraction < 1.0 {
        ClipWorldWinner::Brush
    } else {
        ClipWorldWinner::Open
    };
    if let Some(mesh_hit) = mesh {
        absorb_hit(&mut brush, &mesh_hit, ClipWorldWinner::Mesh, &mut winner);
    }
    finalize_winner(&brush, &mut winner);
    (brush, winner)
}

fn absorb_hit(best: &mut Trace, other: &Trace, src: ClipWorldWinner, winner: &mut ClipWorldWinner) {
    let startsolid = best.startsolid | other.startsolid;
    let allsolid = best.allsolid | other.allsolid;
    if other.fraction < best.fraction {
        *best = *other;
        *winner = src;
    }
    best.startsolid = startsolid;
    best.allsolid = allsolid;
}

fn finalize_winner(best: &Trace, winner: &mut ClipWorldWinner) {
    if best.fraction >= 1.0 {
        *winner = if best.startsolid != 0 {
            ClipWorldWinner::Brush
        } else {
            ClipWorldWinner::Open
        };
    }
}
