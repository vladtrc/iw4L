use asset_world::*;

const T5_BRUSH: usize = 96;
const T5_BRUSH_MINS: usize = 0;
const T5_BRUSH_CONTENTS: usize = 12;
const T5_BRUSH_MAXS: usize = 16;
const T5_BRUSH_NUMSIDES: usize = 28;
const T5_BRUSH_SIDES: usize = 32;

const T5_BRUSH_AXIAL_SFLAGS: usize = 0x3c;
const T5_CBRUSH_SIDE: usize = 12;

const T5_CBRUSH_SIDE_SFLAGS: usize = 8;

pub fn treyarch_surface_flags(flags: u32) -> u32 {
    let mantle_on = flags & 0x0400_0000 != 0;
    let mantle_over = flags & 0x0800_0000 != 0;
    (flags & !0x1e00_0000)
        | if mantle_on { 0x0200_0000 } else { 0 }
        | if mantle_over { 0x0400_0000 } else { 0 }
}

pub fn build_t5_clip_collision(
    s: &ext_t5_fastfile::ZoneStream<'_>,
    g: ext_t5_fastfile::ClipMapGeometry,
) -> Result<ClipCollision, ClipCollisionError> {
    let brushes_ptr = g.brushes.ok_or(ClipCollisionError::MissingTables)?;
    let mut out = ClipCollision {
        brushes: Vec::with_capacity(g.brush_count),
        ..ClipCollision::default()
    };

    for i in 0..g.brush_count {
        let brush = brushes_ptr.at(i * T5_BRUSH);
        let mins = [
            s.f32_at(brush, T5_BRUSH_MINS)
                .map_err(|_| ClipCollisionError::Truncated)?,
            s.f32_at(brush, T5_BRUSH_MINS + 4)
                .map_err(|_| ClipCollisionError::Truncated)?,
            s.f32_at(brush, T5_BRUSH_MINS + 8)
                .map_err(|_| ClipCollisionError::Truncated)?,
        ];
        let maxs = [
            s.f32_at(brush, T5_BRUSH_MAXS)
                .map_err(|_| ClipCollisionError::Truncated)?,
            s.f32_at(brush, T5_BRUSH_MAXS + 4)
                .map_err(|_| ClipCollisionError::Truncated)?,
            s.f32_at(brush, T5_BRUSH_MAXS + 8)
                .map_err(|_| ClipCollisionError::Truncated)?,
        ];
        let contents = s
            .u32_at(brush, T5_BRUSH_CONTENTS)
            .map_err(|_| ClipCollisionError::Truncated)?;

        let mut planes = Vec::with_capacity(6);
        planes.push([1.0, 0.0, 0.0, maxs[0]]);
        planes.push([-1.0, 0.0, 0.0, -mins[0]]);
        planes.push([0.0, 1.0, 0.0, maxs[1]]);
        planes.push([0.0, -1.0, 0.0, -mins[1]]);
        planes.push([0.0, 0.0, 1.0, maxs[2]]);
        planes.push([0.0, 0.0, -1.0, -mins[2]]);

        let mut axial_sflags = [0u32; 6];
        for (i, slot) in axial_sflags.iter_mut().enumerate() {
            *slot = s
                .u32_at(brush, T5_BRUSH_AXIAL_SFLAGS + i * 4)
                .map_err(|_| ClipCollisionError::Truncated)?;
        }
        let mut plane_surface_flags = Vec::with_capacity(6);
        plane_surface_flags.extend_from_slice(&t5_axial_plane_flags(
            axial_sflags.map(treyarch_surface_flags),
        ));

        let numsides = s
            .u32_at(brush, T5_BRUSH_NUMSIDES)
            .map_err(|_| ClipCollisionError::Truncated)? as usize;
        if let Ok(ext_t5_fastfile::ZonePtr::Offset(sides)) = s.ptr_at(brush, T5_BRUSH_SIDES) {
            for j in 0..numsides {
                let side = sides.at(j * T5_CBRUSH_SIDE);
                if let Some(plane) = read_t5_side_plane(s, side)? {
                    planes.push(plane);
                    plane_surface_flags.push(treyarch_surface_flags(
                        s.u32_at(side, T5_CBRUSH_SIDE_SFLAGS)
                            .map_err(|_| ClipCollisionError::Truncated)?,
                    ));
                }
            }
        }

        debug_assert_eq!(planes.len(), plane_surface_flags.len());
        out.brushes.push(ClipBrush {
            planes,
            contents,
            plane_surface_flags,

            glass_encoded: 0,
        });
    }

    extract_t5_mesh_tables(s, g, &mut out)?;
    extract_t5_bsp_tables(s, g, &mut out)?;

    out.validate(crate::ID)?;
    Ok(out)
}

fn t5_axial_plane_flags(axial_sflags: [u32; 6]) -> [u32; 6] {
    [
        axial_sflags[3],
        axial_sflags[0],
        axial_sflags[4],
        axial_sflags[1],
        axial_sflags[5],
        axial_sflags[2],
    ]
}

fn extract_t5_mesh_tables(
    s: &ext_t5_fastfile::ZoneStream<'_>,
    g: ext_t5_fastfile::ClipMapGeometry,
    out: &mut ClipCollision,
) -> Result<(), ClipCollisionError> {
    let mesh = std::sync::Arc::make_mut(&mut out.mesh);
    if (g.vert_count != 0 && g.verts.is_none())
        || (g.tri_count != 0 && g.tri_indices.is_none())
        || (g.material_count != 0 && g.materials.is_none())
    {
        return Err(ClipCollisionError::MissingTables);
    }
    let Some(verts_ptr) = g.verts else {
        return Ok(());
    };
    mesh.verts.reserve(g.vert_count);
    for i in 0..g.vert_count {
        let v = verts_ptr.at(i * 12);
        mesh.verts.push([
            s.f32_at(v, 0).map_err(|_| ClipCollisionError::Truncated)?,
            s.f32_at(v, 4).map_err(|_| ClipCollisionError::Truncated)?,
            s.f32_at(v, 8).map_err(|_| ClipCollisionError::Truncated)?,
        ]);
    }
    if let Some(idx_ptr) = g.tri_indices {
        let n = g.tri_count.saturating_mul(3);
        mesh.tri_indices.reserve(n);
        for i in 0..n {
            let id = s
                .u16_at(idx_ptr, i * 2)
                .map_err(|_| ClipCollisionError::Truncated)?;
            mesh.tri_indices.push(id);
        }
    }
    if let Some(walk_ptr) = g.tri_edge_is_walkable {
        let n = g.tri_count.saturating_mul(3).div_ceil(32) * 4;
        mesh.tri_edge_is_walkable.reserve(n);
        for i in 0..n {
            let b = s
                .u8_at(walk_ptr, i)
                .map_err(|_| ClipCollisionError::Truncated)?;
            mesh.tri_edge_is_walkable.push(b);
        }
    }
    use ext_t5_fastfile::size as t5;
    if let Some(materials) = g.materials {
        for i in 0..g.material_count {
            let p = materials.at(i * t5::DMATERIAL);
            let bytes = s
                .slice_at(p, 0, 64)
                .map_err(|_| ClipCollisionError::Truncated)?;
            let end = bytes.iter().position(|&b| b == 0).unwrap_or(bytes.len());
            out.materials.push(ClipMapMaterial {
                name: String::from_utf8_lossy(&bytes[..end]).into_owned(),
                source_surface_flags: s.u32_at(p, 64).map_err(|_| ClipCollisionError::Truncated)?,
                surface_flags: treyarch_surface_flags(
                    s.u32_at(p, 64).map_err(|_| ClipCollisionError::Truncated)?,
                ),
                content_flags: s.u32_at(p, 68).map_err(|_| ClipCollisionError::Truncated)?,
            });
        }
    }
    if let Some(parts) = g.collision_partitions {
        for i in 0..g.partition_count {
            let p = parts.at(i * t5::COLLISION_PARTITION);
            mesh.partitions.push(clipmap_iw4::ClipPartition {
                tri_count: s.u8_at(p, 0).map_err(|_| ClipCollisionError::Truncated)?,
                first_tri: s.i32_at(p, 4).map_err(|_| ClipCollisionError::Truncated)?,
                first_vert_segment: 0,
                border_count: 0,
                first_border: 0,
            });
        }
    }
    if let Some(trees) = g.collision_aabb_trees {
        for i in 0..g.aabb_tree_count {
            let p = trees.at(i * t5::COLLISION_AABB_TREE);
            mesh.aabb_trees.push(clipmap_iw4::ClipAabbNode {
                origin: [
                    s.f32_at(p, 0).map_err(|_| ClipCollisionError::Truncated)?,
                    s.f32_at(p, 4).map_err(|_| ClipCollisionError::Truncated)?,
                    s.f32_at(p, 8).map_err(|_| ClipCollisionError::Truncated)?,
                ],
                half_size: [
                    s.f32_at(p, 16).map_err(|_| ClipCollisionError::Truncated)?,
                    s.f32_at(p, 20).map_err(|_| ClipCollisionError::Truncated)?,
                    s.f32_at(p, 24).map_err(|_| ClipCollisionError::Truncated)?,
                ],
                material_index: s.u16_at(p, 12).map_err(|_| ClipCollisionError::Truncated)?,
                child_count: s.u16_at(p, 14).map_err(|_| ClipCollisionError::Truncated)?,
                u: s.i32_at(p, 28).map_err(|_| ClipCollisionError::Truncated)?,
            });
        }
    }
    let leaves: Vec<_> = mesh
        .aabb_trees
        .iter()
        .filter(|n| n.child_count == 0)
        .map(|n| (n.material_index, n.u))
        .collect();
    let partitions: Vec<_> = mesh
        .partitions
        .iter()
        .map(|p| (p.tri_count, p.first_tri))
        .collect();
    let sflags: Vec<_> = out.materials.iter().map(|m| m.surface_flags).collect();
    let cflags: Vec<_> = out.materials.iter().map(|m| m.content_flags).collect();
    mesh.tri_surface_flags =
        clipmap_iw4::flatten_tri_surface_flags(&leaves, &partitions, &sflags, g.tri_count);
    mesh.tri_content_flags =
        clipmap_iw4::flatten_tri_surface_flags(&leaves, &partitions, &cflags, g.tri_count);
    out.tri_material_index =
        clipmap_iw4::flatten_tri_material_index(&leaves, &partitions, g.tri_count);
    Ok(())
}

fn extract_t5_bsp_tables(
    s: &ext_t5_fastfile::ZoneStream<'_>,
    g: ext_t5_fastfile::ClipMapGeometry,
    out: &mut ClipCollision,
) -> Result<(), ClipCollisionError> {
    let mesh = std::sync::Arc::make_mut(&mut out.mesh);
    use ext_t5_fastfile::{ZonePtr, size as sz};
    let (Some(nodes), Some(leaves), Some(lb)) = (g.nodes, g.leaves, g.leafbrush_nodes) else {
        return Err(ClipCollisionError::MissingTables);
    };
    for i in 0..g.node_count {
        let p = nodes.at(i * sz::C_NODE);
        let ZonePtr::Offset(plane) = s.ptr_at(p, 0).map_err(|_| ClipCollisionError::Truncated)?
        else {
            return Err(ClipCollisionError::MissingTables);
        };
        out.nodes.push(ClipBspNode {
            plane: [
                s.f32_at(plane, 0)
                    .map_err(|_| ClipCollisionError::Truncated)?,
                s.f32_at(plane, 4)
                    .map_err(|_| ClipCollisionError::Truncated)?,
                s.f32_at(plane, 8)
                    .map_err(|_| ClipCollisionError::Truncated)?,
                s.f32_at(plane, 12)
                    .map_err(|_| ClipCollisionError::Truncated)?,
            ],
            children: [
                i32::from(s.i16_at(p, 4).map_err(|_| ClipCollisionError::Truncated)?),
                i32::from(s.i16_at(p, 6).map_err(|_| ClipCollisionError::Truncated)?),
            ],
        });
    }
    let mut roots = std::collections::BTreeSet::new();
    for i in 0..g.leaf_count {
        let p = leaves.at(i * sz::C_LEAF);
        let first_len = out.leafbrushes.len();
        let node = s.i32_at(p, 36).map_err(|_| ClipCollisionError::Truncated)?;
        if node > 0 {
            append_t5_leafbrush_node(s, lb, node as usize, &mut out.leafbrushes)?;
        }
        let (first_brush, num_brushes) = leafbrush_range(first_len, out.leafbrushes.len())?;
        let first_coll_aabb_index = s.u16_at(p, 0).map_err(|_| ClipCollisionError::Truncated)?;
        let coll_aabb_count = s.u16_at(p, 2).map_err(|_| ClipCollisionError::Truncated)?;
        for id in u32::from(first_coll_aabb_index)
            ..u32::from(first_coll_aabb_index) + u32::from(coll_aabb_count)
        {
            roots.insert(u16::try_from(id).map_err(|_| ClipCollisionError::Truncated)?);
        }
        out.leaves.push(ClipBspLeaf {
            first_brush,
            num_brushes,
            first_coll_aabb_index,
            coll_aabb_count,
        });
    }
    mesh.aabb_roots = roots.into_iter().collect();
    if let Some(models) = g.cmodels {
        for i in 0..g.cmodel_count {
            let p = models.at(i * sz::C_MODEL);
            let first_len = out.leafbrushes.len();
            let node = s.i32_at(p, 64).map_err(|_| ClipCollisionError::Truncated)?;
            if node > 0 {
                append_t5_leafbrush_node(s, lb, node as usize, &mut out.leafbrushes)?;
            }
            let (first_brush, num_brushes) = leafbrush_range(first_len, out.leafbrushes.len())?;
            out.cmodels.push(ClipCmodel {
                mins: [
                    s.f32_at(p, 0).map_err(|_| ClipCollisionError::Truncated)?,
                    s.f32_at(p, 4).map_err(|_| ClipCollisionError::Truncated)?,
                    s.f32_at(p, 8).map_err(|_| ClipCollisionError::Truncated)?,
                ],
                maxs: [
                    s.f32_at(p, 12).map_err(|_| ClipCollisionError::Truncated)?,
                    s.f32_at(p, 16).map_err(|_| ClipCollisionError::Truncated)?,
                    s.f32_at(p, 20).map_err(|_| ClipCollisionError::Truncated)?,
                ],
                radius: s.f32_at(p, 24).map_err(|_| ClipCollisionError::Truncated)?,
                first_brush,
                num_brushes,
            });
        }
    }
    Ok(())
}

fn append_t5_leafbrush_node(
    s: &ext_t5_fastfile::ZoneStream<'_>,
    base: ext_t5_fastfile::Ptr,
    root: usize,
    out: &mut Vec<u16>,
) -> Result<(), ClipCollisionError> {
    let mut pending = vec![(root, 0)];
    while let Some((index, depth)) = pending.pop() {
        if depth > LEAFBRUSH_NODE_MAX_DEPTH {
            return Err(ClipCollisionError::Truncated);
        }
        let p = base.at(index * ext_t5_fastfile::size::C_LEAF_BRUSH_NODE);
        let count = s.i16_at(p, 2).map_err(|_| ClipCollisionError::Truncated)?;
        if count > 0 {
            let ext_t5_fastfile::ZonePtr::Offset(brushes) =
                s.ptr_at(p, 8).map_err(|_| ClipCollisionError::Truncated)?
            else {
                return Err(ClipCollisionError::MissingTables);
            };
            for i in 0..count as usize {
                out.push(
                    s.u16_at(brushes, i * 2)
                        .map_err(|_| ClipCollisionError::Truncated)?,
                );
            }
        } else {
            let a = s.u16_at(p, 16).map_err(|_| ClipCollisionError::Truncated)? as usize;
            let b = s.u16_at(p, 18).map_err(|_| ClipCollisionError::Truncated)? as usize;
            pending.extend(
                leafbrush_node_visits(index, count, a, b)
                    .into_iter()
                    .map(|i| (i, depth + 1)),
            );
        }
    }
    Ok(())
}

fn read_t5_side_plane(
    s: &ext_t5_fastfile::ZoneStream<'_>,
    side: ext_t5_fastfile::Ptr,
) -> Result<Option<[f32; 4]>, ClipCollisionError> {
    match s
        .ptr_at(side, 0)
        .map_err(|_| ClipCollisionError::Truncated)?
    {
        ext_t5_fastfile::ZonePtr::Offset(plane) => Ok(Some([
            s.f32_at(plane, 0)
                .map_err(|_| ClipCollisionError::Truncated)?,
            s.f32_at(plane, 4)
                .map_err(|_| ClipCollisionError::Truncated)?,
            s.f32_at(plane, 8)
                .map_err(|_| ClipCollisionError::Truncated)?,
            s.f32_at(plane, 12)
                .map_err(|_| ClipCollisionError::Truncated)?,
        ])),
        _ => Ok(None),
    }
}
