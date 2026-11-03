use asset_world::*;

pub fn capture_xmodel_collision(
    catalog: &mut XModelCollCatalog,
    stream: &ext_iw5_fastfile::ZoneStream<'_>,
    slot: ext_iw5_fastfile::Ptr,
    insert_slot: Option<ext_iw5_fastfile::Ptr>,
) {
    let Some(g) = stream.latest_xmodel() else {
        return;
    };
    let name = g
        .name
        .and_then(|p| stream.cstr(p).ok())
        .unwrap_or("")
        .to_owned();
    let surfs = match g.coll_surfs {
        Some(arr) => read_iw5_xmodel_coll_surfs(stream, arr, g.num_coll_surfs.max(0) as usize),
        None => Vec::new(),
    };
    catalog.insert_captured(
        asset_material::AssetPointerIdentity {
            block: slot.block,
            offset: slot.offset,
        },
        insert_slot.map(|p| asset_material::AssetPointerIdentity {
            block: p.block,
            offset: p.offset,
        }),
        name,
        clipmap_iw4::XModelColl {
            coll_lod: g.coll_lod,
            contents: g.contents,
            surfs,
        },
    );
}
pub fn attach_iw5_static_models(
    s: &ext_iw5_fastfile::ZoneStream<'_>,
    g: ext_iw5_fastfile::ClipMapGeometry,
    catalog: &XModelCollCatalog,
    out: &mut ClipCollision,
) {
    extract_iw5_static_models(s, g, catalog, out);
    if out.static_models.len() != g.static_model_count {
        out.coverage = CollisionCoverage::Unvalidated;
    }
}

fn read_iw5_xmodel_coll_surfs(
    s: &ext_iw5_fastfile::ZoneStream<'_>,
    arr: ext_iw5_fastfile::Ptr,
    num: usize,
) -> Vec<clipmap_iw4::XModelCollSurf> {
    use ext_iw5_fastfile::ZonePtr;
    use ext_iw5_fastfile::size as iw5sz;
    (0..num)
        .filter_map(|i| {
            let row = arr.at(i * s.layout(iw5sz::XMODEL_COLL_SURF, 48));
            let bounds = s.layout(8, 12);
            let midpoint = [
                s.f32_at(row, bounds).ok()?,
                s.f32_at(row, bounds + 4).ok()?,
                s.f32_at(row, bounds + 8).ok()?,
            ];
            let half_size = [
                s.f32_at(row, bounds + 12).ok()?,
                s.f32_at(row, bounds + 16).ok()?,
                s.f32_at(row, bounds + 20).ok()?,
            ];
            let bone_idx = s.i32_at(row, s.layout(32, 36)).ok()?;
            let contents = s.u32_at(row, s.layout(36, 40)).ok()?;
            let surf_flags = s.u32_at(row, s.layout(40, 44)).ok()?;
            let num_tris = s.i32_at(row, s.layout(4, 8)).ok()?.max(0) as usize;
            if num_tris > 1_000_000 {
                return None;
            }
            let tris = match s.ptr_at(row, 0).ok()? {
                ZonePtr::Offset(p) => read_iw5_xmodel_coll_tris(s, s.resolve_alias(p), num_tris),
                _ => Vec::new(),
            };
            Some(clipmap_iw4::XModelCollSurf {
                tris,
                midpoint,
                half_size,
                bone_idx,
                contents,
                surf_flags,
            })
        })
        .collect()
}

fn read_iw5_xmodel_coll_tris(
    s: &ext_iw5_fastfile::ZoneStream<'_>,
    arr: ext_iw5_fastfile::Ptr,
    num: usize,
) -> Vec<clipmap_iw4::XModelCollTri> {
    use ext_iw5_fastfile::size as iw5sz;
    (0..num)
        .filter_map(|i| {
            let t = arr.at(i * iw5sz::XMODEL_COLL_TRI);
            let f = |off: usize| s.f32_at(t, off).ok();
            Some(clipmap_iw4::XModelCollTri {
                plane: [f(0)?, f(4)?, f(8)?, f(12)?],
                svec: [f(16)?, f(20)?, f(24)?, f(28)?],
                tvec: [f(32)?, f(36)?, f(40)?, f(44)?],
            })
        })
        .collect()
}

pub fn build_iw5_clip_collision(
    s: &ext_iw5_fastfile::ZoneStream<'_>,
    g: ext_iw5_fastfile::ClipMapGeometry,
) -> Result<ClipCollision, ClipCollisionError> {
    use ext_iw5_fastfile::size as iw5sz;

    let brushes_ptr = g.brushes.ok_or(ClipCollisionError::MissingTables)?;
    let bounds_ptr = g.brush_bounds.ok_or(ClipCollisionError::MissingTables)?;
    let contents_ptr = g.brush_contents.ok_or(ClipCollisionError::MissingTables)?;
    let sides_base = g.brush_sides;
    let materials = g.materials;

    let mut out = ClipCollision {
        brushes: Vec::with_capacity(g.brush_count),
        ..ClipCollision::default()
    };

    for i in 0..g.brush_count {
        let brush = brushes_ptr.at(i * s.layout(iw5sz::CBRUSH, 48));
        let bound = bounds_ptr.at(i * iw5sz::BOUNDS);
        let contents = s
            .u32_at(contents_ptr, i * 4)
            .map_err(|_| ClipCollisionError::Truncated)?;

        let mid = [
            s.f32_at(bound, 0)
                .map_err(|_| ClipCollisionError::Truncated)?,
            s.f32_at(bound, 4)
                .map_err(|_| ClipCollisionError::Truncated)?,
            s.f32_at(bound, 8)
                .map_err(|_| ClipCollisionError::Truncated)?,
        ];
        let half = [
            s.f32_at(bound, 12)
                .map_err(|_| ClipCollisionError::Truncated)?,
            s.f32_at(bound, 16)
                .map_err(|_| ClipCollisionError::Truncated)?,
            s.f32_at(bound, 20)
                .map_err(|_| ClipCollisionError::Truncated)?,
        ];
        let mins = [mid[0] - half[0], mid[1] - half[1], mid[2] - half[2]];
        let maxs = [mid[0] + half[0], mid[1] + half[1], mid[2] + half[2]];

        let mut planes = Vec::with_capacity(6);
        planes.push([1.0, 0.0, 0.0, maxs[0]]);
        planes.push([-1.0, 0.0, 0.0, -mins[0]]);
        planes.push([0.0, 1.0, 0.0, maxs[1]]);
        planes.push([0.0, -1.0, 0.0, -mins[1]]);
        planes.push([0.0, 0.0, 1.0, maxs[2]]);
        planes.push([0.0, 0.0, -1.0, -mins[2]]);

        let mut axial_mat = [0u16; 6];
        for i in 0..6 {
            axial_mat[i] = s
                .u16_at(brush, s.layout(12, 24) + i * 2)
                .map_err(|_| ClipCollisionError::Truncated)?;
        }
        let mut plane_surface_flags = Vec::with_capacity(6);
        plane_surface_flags.extend_from_slice(&axial_plane_flags(axial_mat, |idx| {
            iw5_material_flags(s, materials, idx)
        }));

        let numsides = s
            .u16_at(brush, 0)
            .map_err(|_| ClipCollisionError::Truncated)? as usize;

        let glass_encoded = s
            .u16_at(brush, 2)
            .map_err(|_| ClipCollisionError::Truncated)?;
        if let (Some(sides_all), Some(first)) = (sides_base, iw5_side_first(s, brush, sides_base)?)
        {
            for j in 0..numsides {
                let side = sides_all.at((first + j) * s.layout(iw5sz::CBRUSH_SIDE, 16));
                if let Some(plane) = iw5_read_side_plane(s, side)? {
                    planes.push(plane);
                    let mat = s
                        .u16_at(side, s.layout(4, 8))
                        .map_err(|_| ClipCollisionError::Truncated)?
                        as usize;
                    plane_surface_flags.push(iw5_material_flags(s, materials, mat));
                }
            }
        }

        debug_assert_eq!(planes.len(), plane_surface_flags.len());
        out.brushes.push(ClipBrush {
            planes,
            contents,
            plane_surface_flags,
            glass_encoded,
        });
    }

    extract_iw5_bsp_tables(s, g, &mut out)?;
    extract_iw5_mesh_tables(s, g, &mut out)?;
    extract_iw5_cmodels(s, g, &mut out)?;
    gate_leafbrush_index_fits(out.leafbrushes.len())?;

    out.validate(crate::ID)?;
    Ok(out)
}

fn extract_iw5_bsp_tables(
    s: &ext_iw5_fastfile::ZoneStream<'_>,
    g: ext_iw5_fastfile::ClipMapGeometry,
    out: &mut ClipCollision,
) -> Result<(), ClipCollisionError> {
    use ext_iw5_fastfile::ZonePtr;
    use ext_iw5_fastfile::size as iw5sz;

    let Some(nodes_ptr) = g.nodes else {
        return Ok(());
    };
    let Some(leaves_ptr) = g.leaves else {
        return Ok(());
    };
    let Some(leafbrushes_ptr) = g.leafbrushes else {
        return Ok(());
    };

    out.nodes.reserve(g.node_count);
    for i in 0..g.node_count {
        let node = nodes_ptr.at(i * s.layout(iw5sz::C_NODE, 16));
        let plane = match s
            .ptr_at(node, 0)
            .map_err(|_| ClipCollisionError::Truncated)?
        {
            ZonePtr::Offset(p) => [
                s.f32_at(p, 0).map_err(|_| ClipCollisionError::Truncated)?,
                s.f32_at(p, 4).map_err(|_| ClipCollisionError::Truncated)?,
                s.f32_at(p, 8).map_err(|_| ClipCollisionError::Truncated)?,
                s.f32_at(p, 12).map_err(|_| ClipCollisionError::Truncated)?,
            ],
            _ => [0.0, 0.0, 1.0, 0.0],
        };
        let children = s.layout(4, 8);
        let c0 = s
            .i16_at(node, children)
            .map_err(|_| ClipCollisionError::Truncated)? as i32;
        let c1 = s
            .i16_at(node, children + 2)
            .map_err(|_| ClipCollisionError::Truncated)? as i32;
        out.nodes.push(ClipBspNode {
            plane,
            children: [c0, c1],
        });
    }

    let lb_nodes = g.leafbrush_nodes;
    out.leaves.reserve(g.leaf_count);
    for i in 0..g.leaf_count {
        let leaf = leaves_ptr.at(i * iw5sz::C_LEAF);
        let lb_index = s
            .i32_at(leaf, 36)
            .map_err(|_| ClipCollisionError::Truncated)?;
        let first_len = out.leafbrushes.len();
        if lb_index >= 0 {
            if let Some(lb_base) = lb_nodes {
                append_iw5_leafbrush_node(s, lb_base, lb_index as usize, &mut out.leafbrushes)?;
            }
        }
        let (first, num) = leafbrush_range(first_len, out.leafbrushes.len())?;
        let first_coll_aabb_index = s
            .u16_at(leaf, 0)
            .map_err(|_| ClipCollisionError::Truncated)?;
        let coll_aabb_count = s
            .u16_at(leaf, 2)
            .map_err(|_| ClipCollisionError::Truncated)?;
        out.leaves.push(ClipBspLeaf {
            first_brush: first,
            num_brushes: num,
            first_coll_aabb_index,
            coll_aabb_count,
        });
    }

    if out.leafbrushes.is_empty() && g.leafbrush_count > 0 {
        for i in 0..g.leafbrush_count {
            let id = s
                .u16_at(leafbrushes_ptr, i * 2)
                .map_err(|_| ClipCollisionError::Truncated)?;
            out.leafbrushes.push(id);
        }
    }

    Ok(())
}

fn append_iw5_leafbrush_node(
    s: &ext_iw5_fastfile::ZoneStream<'_>,
    lb_base: ext_iw5_fastfile::Ptr,
    index: usize,
    out: &mut Vec<u16>,
) -> Result<(), ClipCollisionError> {
    append_iw5_leafbrush_node_at(s, lb_base, index, 0, out)
}

fn append_iw5_leafbrush_node_at(
    s: &ext_iw5_fastfile::ZoneStream<'_>,
    lb_base: ext_iw5_fastfile::Ptr,
    index: usize,
    depth: usize,
    out: &mut Vec<u16>,
) -> Result<(), ClipCollisionError> {
    use ext_iw5_fastfile::ZonePtr;
    use ext_iw5_fastfile::size as iw5sz;

    if depth > LEAFBRUSH_NODE_MAX_DEPTH {
        return Ok(());
    }
    let node = lb_base.at(index * s.layout(iw5sz::C_LEAF_BRUSH_NODE, 24));
    let count = s
        .i16_at(node, 2)
        .map_err(|_| ClipCollisionError::Truncated)?;
    if count > 0 {
        if let ZonePtr::Offset(brushes) = s
            .ptr_at(node, 8)
            .map_err(|_| ClipCollisionError::Truncated)?
        {
            for j in 0..count as usize {
                out.push(
                    s.u16_at(brushes, j * 2)
                        .map_err(|_| ClipCollisionError::Truncated)?,
                );
            }
        }
        return Ok(());
    }
    let off0 = s
        .u16_at(node, 16)
        .map_err(|_| ClipCollisionError::Truncated)? as usize;
    let off1 = s
        .u16_at(node, 18)
        .map_err(|_| ClipCollisionError::Truncated)? as usize;
    for next in leafbrush_node_visits(index, count, off0, off1) {
        append_iw5_leafbrush_node_at(s, lb_base, next, depth + 1, out)?;
    }
    Ok(())
}

fn extract_iw5_mesh_tables(
    s: &ext_iw5_fastfile::ZoneStream<'_>,
    g: ext_iw5_fastfile::ClipMapGeometry,
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
    extract_iw5_mesh_materials(s, g, out)?;
    extract_iw5_aabb_forest(s, g, out)?;
    Ok(())
}

fn extract_iw5_aabb_forest(
    s: &ext_iw5_fastfile::ZoneStream<'_>,
    g: ext_iw5_fastfile::ClipMapGeometry,
    out: &mut ClipCollision,
) -> Result<(), ClipCollisionError> {
    let mesh = std::sync::Arc::make_mut(&mut out.mesh);
    use ext_iw5_fastfile::size as iw5sz;

    if let Some(parts) = g.collision_partitions {
        mesh.partitions.reserve(g.partition_count);
        for i in 0..g.partition_count {
            let p = parts.at(i * s.layout(iw5sz::COLLISION_PARTITION, 16));
            let tri_n = s.u8_at(p, 0).map_err(|_| ClipCollisionError::Truncated)?;
            let seg = s.u8_at(p, 2).map_err(|_| ClipCollisionError::Truncated)?;
            let first = s.i32_at(p, 4).map_err(|_| ClipCollisionError::Truncated)?;
            let border_n = s.u8_at(p, 1).map_err(|_| ClipCollisionError::Truncated)?;
            let first_border = mesh.borders.len() as u32;
            if border_n != 0
                && let ext_iw5_fastfile::ZonePtr::Offset(b) =
                    s.ptr_at(p, 8).map_err(|_| ClipCollisionError::Truncated)?
            {
                let b = s.resolve_alias(b);
                for k in 0..border_n as usize {
                    let f = |o: usize| {
                        s.f32_at(b, k * 28 + o)
                            .map_err(|_| ClipCollisionError::Truncated)
                    };
                    mesh.borders.push(clipmap_iw4::ClipBorder {
                        dist_eq: [f(0)?, f(4)?, f(8)?],
                        z_base: f(12)?,
                        z_slope: f(16)?,
                        start: f(20)?,
                        length: f(24)?,
                    });
                }
            }
            let border_count = (mesh.borders.len() as u32 - first_border) as u8;
            mesh.partitions.push(clipmap_iw4::ClipPartition {
                tri_count: tri_n,
                first_tri: first,
                first_vert_segment: seg,
                border_count,
                first_border,
            });
        }
    }
    if let Some(trees) = g.collision_aabb_trees {
        mesh.aabb_trees.reserve(g.aabb_tree_count);
        for i in 0..g.aabb_tree_count {
            let node = trees.at(i * iw5sz::COLLISION_AABB_TREE);
            let origin = [
                s.f32_at(node, 0)
                    .map_err(|_| ClipCollisionError::Truncated)?,
                s.f32_at(node, 4)
                    .map_err(|_| ClipCollisionError::Truncated)?,
                s.f32_at(node, 8)
                    .map_err(|_| ClipCollisionError::Truncated)?,
            ];
            let material_index = s
                .u16_at(node, 12)
                .map_err(|_| ClipCollisionError::Truncated)?;
            let child_count = s
                .u16_at(node, 14)
                .map_err(|_| ClipCollisionError::Truncated)?;
            let half_size = [
                s.f32_at(node, 16)
                    .map_err(|_| ClipCollisionError::Truncated)?,
                s.f32_at(node, 20)
                    .map_err(|_| ClipCollisionError::Truncated)?,
                s.f32_at(node, 24)
                    .map_err(|_| ClipCollisionError::Truncated)?,
            ];
            let u = s
                .i32_at(node, 28)
                .map_err(|_| ClipCollisionError::Truncated)?;
            mesh.aabb_trees.push(clipmap_iw4::ClipAabbNode {
                origin,
                half_size,
                material_index,
                child_count,
                u,
            });
        }
    }
    mesh.aabb_roots = clipmap_iw4::aabb_forest_roots(&mesh.aabb_trees);
    Ok(())
}

fn extract_iw5_mesh_materials(
    s: &ext_iw5_fastfile::ZoneStream<'_>,
    g: ext_iw5_fastfile::ClipMapGeometry,
    out: &mut ClipCollision,
) -> Result<(), ClipCollisionError> {
    let mesh = std::sync::Arc::make_mut(&mut out.mesh);
    use ext_iw5_fastfile::ZonePtr;
    use ext_iw5_fastfile::size as iw5sz;

    out.materials = {
        let mut mats = Vec::with_capacity(g.material_count);
        if let Some(base) = g.materials {
            for i in 0..g.material_count {
                let slot = base.at(i * s.layout(iw5sz::CLIP_MATERIAL, 16));
                let name = match s
                    .ptr_at(slot, 0)
                    .map_err(|_| ClipCollisionError::Truncated)?
                {
                    ZonePtr::Offset(p) => s.cstr(s.resolve_alias(p)).unwrap_or("").to_owned(),
                    _ => String::new(),
                };
                let surface_flags = s
                    .u32_at(slot, s.layout(4, 8))
                    .map_err(|_| ClipCollisionError::Truncated)?;
                let content_flags = s
                    .u32_at(slot, s.layout(8, 12))
                    .map_err(|_| ClipCollisionError::Truncated)?;
                mats.push(ClipMapMaterial {
                    name,
                    source_surface_flags: surface_flags,
                    surface_flags,
                    content_flags,
                });
            }
        } else {
            if g.material_count != 0 {
                return Err(ClipCollisionError::MissingTables);
            }
        }
        mats
    };
    if g.tri_count == 0 {
        return Ok(());
    }
    let mut material_sflags = vec![0u32; g.material_count];
    let mut material_cflags = vec![0u32; g.material_count];
    for (i, mat) in out.materials.iter().enumerate() {
        if let Some(slot) = material_sflags.get_mut(i) {
            *slot = mat.surface_flags;
        }
        if let Some(slot) = material_cflags.get_mut(i) {
            *slot = mat.content_flags;
        }
    }
    let mut partitions = vec![(0u8, 0i32); g.partition_count];
    if let Some(parts) = g.collision_partitions {
        for (i, slot) in partitions.iter_mut().enumerate() {
            let p = parts.at(i * s.layout(iw5sz::COLLISION_PARTITION, 16));
            let tri_n = s.u8_at(p, 0).map_err(|_| ClipCollisionError::Truncated)?;
            let first = s.i32_at(p, 4).map_err(|_| ClipCollisionError::Truncated)?;
            *slot = (tri_n, first);
        }
    }
    let mut leaves = Vec::new();
    if let Some(trees) = g.collision_aabb_trees {
        for i in 0..g.aabb_tree_count {
            let node = trees.at(i * iw5sz::COLLISION_AABB_TREE);
            let child_count = s
                .u16_at(node, 14)
                .map_err(|_| ClipCollisionError::Truncated)?;
            if child_count != 0 {
                continue;
            }
            let mat = s
                .u16_at(node, 12)
                .map_err(|_| ClipCollisionError::Truncated)?;
            let part = s
                .i32_at(node, 28)
                .map_err(|_| ClipCollisionError::Truncated)?;
            leaves.push((mat, part));
        }
    }
    mesh.tri_surface_flags =
        clipmap_iw4::flatten_tri_surface_flags(&leaves, &partitions, &material_sflags, g.tri_count);
    mesh.tri_content_flags =
        clipmap_iw4::flatten_tri_surface_flags(&leaves, &partitions, &material_cflags, g.tri_count);
    out.tri_material_index =
        clipmap_iw4::flatten_tri_material_index(&leaves, &partitions, g.tri_count);
    Ok(())
}

fn extract_iw5_cmodels(
    s: &ext_iw5_fastfile::ZoneStream<'_>,
    g: ext_iw5_fastfile::ClipMapGeometry,
    out: &mut ClipCollision,
) -> Result<(), ClipCollisionError> {
    use ext_iw5_fastfile::size as iw5sz;

    let Some(cmodels_ptr) = g.cmodels else {
        return Ok(());
    };
    let lb_nodes = g.leafbrush_nodes;
    out.cmodels.reserve(g.cmodel_count);
    for i in 0..g.cmodel_count {
        let cm = cmodels_ptr.at(i * s.layout(iw5sz::CMODEL, 80));
        let mid = [
            s.f32_at(cm, 0).map_err(|_| ClipCollisionError::Truncated)?,
            s.f32_at(cm, 4).map_err(|_| ClipCollisionError::Truncated)?,
            s.f32_at(cm, 8).map_err(|_| ClipCollisionError::Truncated)?,
        ];
        let half = [
            s.f32_at(cm, 12)
                .map_err(|_| ClipCollisionError::Truncated)?,
            s.f32_at(cm, 16)
                .map_err(|_| ClipCollisionError::Truncated)?,
            s.f32_at(cm, 20)
                .map_err(|_| ClipCollisionError::Truncated)?,
        ];
        let mins = [mid[0] - half[0], mid[1] - half[1], mid[2] - half[2]];
        let maxs = [mid[0] + half[0], mid[1] + half[1], mid[2] + half[2]];
        let radius = s
            .f32_at(cm, 24)
            .map_err(|_| ClipCollisionError::Truncated)?;
        let lb_index = s
            .i32_at(cm, s.layout(0x44, 76))
            .map_err(|_| ClipCollisionError::Truncated)?;
        let first_len = out.leafbrushes.len();
        if lb_index >= 0 {
            if let Some(lb_base) = lb_nodes {
                if append_iw5_leafbrush_node(s, lb_base, lb_index as usize, &mut out.leafbrushes)
                    .is_err()
                {
                    out.leafbrushes.truncate(first_len);
                }
            }
        }
        let (first, num) = leafbrush_range(first_len, out.leafbrushes.len())?;
        out.cmodels.push(ClipCmodel {
            mins,
            maxs,
            radius,
            first_brush: first,
            num_brushes: num,
        });
    }
    Ok(())
}

fn extract_iw5_static_models(
    s: &ext_iw5_fastfile::ZoneStream<'_>,
    g: ext_iw5_fastfile::ClipMapGeometry,
    catalog: &XModelCollCatalog,
    out: &mut ClipCollision,
) {
    use ext_iw5_fastfile::size as iw5sz;
    let Some(arr) = g.static_models else {
        return;
    };
    out.static_models.reserve(g.static_model_count);
    for i in 0..g.static_model_count {
        let sm = arr.at(i * s.layout(iw5sz::C_STATIC_MODEL, 80));
        if let Some(placed) = read_iw5_placed_static_model(s, sm, i as u32, catalog) {
            out.static_models.push(placed);
        }
    }
}

fn read_iw5_placed_static_model(
    s: &ext_iw5_fastfile::ZoneStream<'_>,
    sm: ext_iw5_fastfile::Ptr,
    index: u32,
    catalog: &XModelCollCatalog,
) -> Option<ClipPlacedStaticModel> {
    use ext_iw5_fastfile::ZonePtr;
    let body = match s.ptr_at(sm, 0).ok()? {
        ZonePtr::Offset(p) => s.resolve_alias(p),
        _ => return None,
    };
    let captured = catalog.get_at(body.block, body.offset)?;
    let origin_off = s.layout(4, 8);
    let origin = [
        s.f32_at(sm, origin_off).ok()?,
        s.f32_at(sm, origin_off + 4).ok()?,
        s.f32_at(sm, origin_off + 8).ok()?,
    ];
    let axis_off = s.layout(0x10, 20);
    let mut inv_scaled_axis = [[0.0_f32; 3]; 3];
    for row in 0..3 {
        for col in 0..3 {
            inv_scaled_axis[row][col] = s.f32_at(sm, axis_off + (row * 3 + col) * 4).ok()?;
        }
    }
    let abs_bounds = s.layout(0x34, 56);
    let bounds_mid = [
        s.f32_at(sm, abs_bounds).ok()?,
        s.f32_at(sm, abs_bounds + 4).ok()?,
        s.f32_at(sm, abs_bounds + 8).ok()?,
    ];
    let bounds_half = [
        s.f32_at(sm, abs_bounds + 12).ok()?,
        s.f32_at(sm, abs_bounds + 16).ok()?,
        s.f32_at(sm, abs_bounds + 20).ok()?,
    ];
    if !origin.iter().all(|v| v.is_finite())
        || !bounds_mid.iter().all(|v| v.is_finite())
        || !bounds_half.iter().all(|v| v.is_finite() && *v >= 0.0)
    {
        return None;
    }
    Some(ClipPlacedStaticModel {
        index,
        name: captured.name.clone(),
        model: clipmap_iw4::ClipStaticModel {
            origin,
            inv_scaled_axis,
            bounds_mid,
            bounds_half,
            coll: captured.coll.clone(),
        },
    })
}

fn iw5_side_first(
    s: &ext_iw5_fastfile::ZoneStream<'_>,
    brush: ext_iw5_fastfile::Ptr,
    sides_all: Option<ext_iw5_fastfile::Ptr>,
) -> Result<Option<usize>, ClipCollisionError> {
    use ext_iw5_fastfile::ZonePtr;
    use ext_iw5_fastfile::size as iw5sz;

    let Some(sides_all) = sides_all else {
        return Ok(None);
    };
    match s
        .ptr_at(brush, s.layout(4, 8))
        .map_err(|_| ClipCollisionError::Truncated)?
    {
        ZonePtr::Offset(p) if p.block == sides_all.block && p.offset >= sides_all.offset => Ok(
            Some((p.offset - sides_all.offset) as usize / s.layout(iw5sz::CBRUSH_SIDE, 16)),
        ),
        _ => Ok(None),
    }
}

fn iw5_read_side_plane(
    s: &ext_iw5_fastfile::ZoneStream<'_>,
    side: ext_iw5_fastfile::Ptr,
) -> Result<Option<[f32; 4]>, ClipCollisionError> {
    use ext_iw5_fastfile::ZonePtr;

    match s
        .ptr_at(side, 0)
        .map_err(|_| ClipCollisionError::Truncated)?
    {
        ZonePtr::Offset(plane) => Ok(Some([
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

fn iw5_material_flags(
    s: &ext_iw5_fastfile::ZoneStream<'_>,
    materials: Option<ext_iw5_fastfile::Ptr>,
    index: usize,
) -> u32 {
    use ext_iw5_fastfile::size as iw5sz;

    let Some(base) = materials else {
        return 0;
    };
    s.u32_at(
        base.at(index * s.layout(iw5sz::CLIP_MATERIAL, 16)),
        s.layout(4, 8),
    )
    .unwrap_or(0)
}
