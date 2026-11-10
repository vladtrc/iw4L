use crate::{ClipCollision, WorldRegion};
use clipmap_iw4::{ClipAabbNode, ClipLeaf, ClipMeshTables, ClipNode, VERTS_PER_SEGMENT};
use std::collections::{BTreeSet, HashMap};

pub fn crop_collision(
    clip: &mut ClipCollision,
    region: WorldRegion,
    family: asset_core::FamilyId,
) -> Result<(), String> {
    let mut in_world = vec![false; clip.brushes.len()];
    for leaf in &clip.leaves {
        let first = leaf.first_brush as usize;
        for id in &clip.leafbrushes[first..first + leaf.num_brushes as usize] {
            in_world[*id as usize] = true;
        }
    }
    // Submodel brushes are entity-relative; their entity decides whether they stay.
    let mut remap = vec![None; clip.brushes.len()];
    let mut brushes = Vec::new();
    for (i, mut brush) in std::mem::take(&mut clip.brushes).into_iter().enumerate() {
        if in_world[i] {
            if brush.planes.iter().any(|p| {
                let low: f32 = (0..3)
                    .map(|axis| {
                        p[axis]
                            * if p[axis] >= 0. {
                                region.min[axis]
                            } else {
                                region.max[axis]
                            }
                    })
                    .sum();
                low > p[3] + 0.125
            }) {
                continue;
            }
            for axis in 0..3 {
                let mut plane = [0.; 4];
                plane[axis] = 1.;
                plane[3] = region.max[axis];
                brush.planes.push(plane);
                brush.plane_surface_flags.push(0);
                plane[axis] = -1.;
                plane[3] = -region.min[axis];
                brush.planes.push(plane);
                brush.plane_surface_flags.push(0);
            }
        }
        remap[i] = Some(u16::try_from(brushes.len()).map_err(|_| "region brush index overflow")?);
        brushes.push(brush);
    }
    let old_ids = std::mem::take(&mut clip.leafbrushes);
    let world_ids: BTreeSet<u16> = (0..remap.len())
        .filter(|&i| in_world[i])
        .filter_map(|i| remap[i])
        .collect();
    clip.brushes = brushes;
    clip.leafbrushes = world_ids.into_iter().collect();
    let root_brushes = u16::try_from(clip.leafbrushes.len()).map_err(|_| "region leaf overflow")?;
    for model in &mut clip.cmodels {
        let old_first = model.first_brush as usize;
        let old_count = model.num_brushes as usize;
        model.first_brush = clip.leafbrushes.len() as u32;
        for id in &old_ids[old_first..old_first + old_count] {
            if let Some(new) = remap[*id as usize] {
                clip.leafbrushes.push(new);
            }
        }
        model.num_brushes = u16::try_from(clip.leafbrushes.len() - model.first_brush as usize)
            .map_err(|_| "region cmodel overflow")?;
    }
    if let Some(root) = clip.cmodels.first_mut() {
        root.mins = region.min;
        root.maxs = region.max;
    }

    let old = &clip.mesh;
    let part_materials: HashMap<i32, _> = old
        .aabb_trees
        .iter()
        .rev()
        .filter(|n| n.child_count == 0)
        .map(|n| (n.u, n.material_index))
        .collect();
    let mut mesh = ClipMeshTables::default();
    let mut tri_materials = Vec::new();
    for (part_id, part) in old.partitions.iter().enumerate() {
        let base = part.first_vert_segment as usize * VERTS_PER_SEGMENT;
        let mut triangles = Vec::new();
        let mut min = [f32::INFINITY; 3];
        let mut max = [f32::NEG_INFINITY; 3];
        for ti in part.first_tri as usize..part.first_tri as usize + part.tri_count as usize {
            let indices = &old.tri_indices[ti * 3..ti * 3 + 3];
            let mut lo = [f32::INFINITY; 3];
            let mut hi = [f32::NEG_INFINITY; 3];
            for index in indices {
                let v = old.verts[base + *index as usize];
                for axis in 0..3 {
                    lo[axis] = lo[axis].min(v[axis]);
                    hi[axis] = hi[axis].max(v[axis]);
                }
            }
            if region.intersects(lo, hi) {
                triangles.push(ti);
                for axis in 0..3 {
                    min[axis] = min[axis].min(lo[axis]);
                    max[axis] = max[axis].max(hi[axis]);
                }
            }
        }
        if triangles.is_empty() {
            continue;
        }
        let segment = mesh.verts.len() / VERTS_PER_SEGMENT;
        let vertex_base = segment * VERTS_PER_SEGMENT;
        let segment =
            u8::try_from(segment).map_err(|_| "region collision vertex segment overflow")?;
        let mut vertex_remap = HashMap::new();
        let mut kept = *part;
        kept.first_vert_segment = segment;
        kept.first_tri = mesh.tri_count() as i32;
        kept.tri_count = triangles.len() as u8;
        kept.first_border = mesh.borders.len() as u32;
        mesh.borders.extend_from_slice(
            &old.borders[part.first_border as usize
                ..part.first_border as usize + part.border_count as usize],
        );
        for ti in triangles {
            let next_triangle = mesh.tri_count();
            for old_index in &old.tri_indices[ti * 3..ti * 3 + 3] {
                let absolute = base + usize::from(*old_index);
                let next = u16::try_from(mesh.verts.len() - vertex_base)
                    .map_err(|_| "region partition vertex overflow")?;
                let index = *vertex_remap.entry(absolute).or_insert_with(|| {
                    mesh.verts.push(old.verts[absolute]);
                    next
                });
                mesh.tri_indices.push(index);
            }
            mesh.tri_edge_is_walkable
                .resize((mesh.tri_count() * 3).div_ceil(8), 0);
            for edge in 0..3 {
                let old_bit = ti * 3 + edge;
                if old
                    .tri_edge_is_walkable
                    .get(old_bit / 8)
                    .is_some_and(|byte| byte & (1 << (old_bit % 8)) != 0)
                {
                    let bit = next_triangle * 3 + edge;
                    mesh.tri_edge_is_walkable[bit / 8] |= 1 << (bit % 8);
                }
            }
            mesh.tri_surface_flags.push(old.tri_surface_flags[ti]);
            mesh.tri_content_flags.push(old.tri_content_flags[ti]);
            tri_materials.push(clip.tri_material_index.get(ti).copied().unwrap_or(0));
        }
        let material_index = part_materials.get(&(part_id as i32)).copied().unwrap_or(0);
        let node = ClipAabbNode {
            origin: std::array::from_fn(|i| (min[i] + max[i]) * 0.5),
            half_size: std::array::from_fn(|i| (max[i] - min[i]) * 0.5),
            material_index,
            child_count: 0,
            u: mesh.partitions.len() as i32,
        };
        mesh.aabb_roots
            .push(u16::try_from(mesh.aabb_trees.len()).map_err(|_| "region AABB overflow")?);
        mesh.aabb_trees.push(node);
        mesh.partitions.push(kept);
    }
    clip.tri_material_index = tri_materials;
    let aabb_count = u16::try_from(mesh.aabb_trees.len()).map_err(|_| "region mesh overflow")?;
    clip.mesh = std::sync::Arc::new(mesh);
    clip.nodes = vec![ClipNode {
        plane: [1., 0., 0., 0.],
        children: [-1, -1],
    }];
    clip.leaves = vec![ClipLeaf {
        first_brush: 0,
        num_brushes: root_brushes,
        first_coll_aabb_index: 0,
        coll_aabb_count: aabb_count,
    }];
    clip.static_models.retain(|m| {
        let lo = std::array::from_fn(|i| m.model.bounds_mid[i] - m.model.bounds_half[i]);
        let hi = std::array::from_fn(|i| m.model.bounds_mid[i] + m.model.bounds_half[i]);
        region.intersects(lo, hi)
    });
    clip.validate(family)
        .map_err(|e| format!("region collision: {e}"))
}
