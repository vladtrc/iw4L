use asset_material::{AssetPointerIdentity, MaterialCatalog};
use asset_model::{normalize_or_up, unpack_color, unpack_packed_tex_coords, unpack_unit_vec};
use asset_world::*;
use bevy::asset::RenderAssetUsages;
use bevy::prelude::*;
use bevy::render::mesh::{Indices, PrimitiveTopology};
pub fn build_t5_static_model_instances(
    stream: &ext_t5_fastfile::ZoneStream<'_>,
    world: ext_t5_fastfile::GfxWorldGeometry,
) -> Result<Vec<StaticModelInstance>, ModelMeshError> {
    let (Some(draw_insts), Some(_insts)) = (world.smodel_draw_insts, world.smodel_insts) else {
        return Err(ModelMeshError::MissingStaticModelInstances);
    };
    const DRAW_INST: usize = 76;

    const CULL_DIST_OFF: usize = 0x00;
    const PLACEMENT_OFF: usize = 0x04;
    const ORIGIN_OFF: usize = PLACEMENT_OFF;
    const AXIS_OFF: usize = PLACEMENT_OFF + 12;
    const SCALE_OFF: usize = PLACEMENT_OFF + 0x30;
    const MODEL_OFF: usize = 0x38;
    let mut out = Vec::with_capacity(world.smodel_count);
    for index in 0..world.smodel_count {
        let inst = draw_insts.at(index * DRAW_INST);
        let mut axis = [[0.0; 3]; 3];
        for (row, values) in axis.iter_mut().enumerate() {
            for (column, value) in values.iter_mut().enumerate() {
                *value = stream
                    .f32_at(inst, AXIS_OFF + (row * 3 + column) * 4)
                    .map_err(|_| ModelMeshError::MissingStaticModelInstances)?;
            }
        }
        let model = match stream
            .ptr_at(inst, MODEL_OFF)
            .map_err(|_| ModelMeshError::MissingStaticModelInstances)?
        {
            ext_t5_fastfile::ZonePtr::Offset(p) => p,
            _ => {
                continue;
            }
        };
        out.push(StaticModelInstance {
            model_slot: AssetPointerIdentity {
                block: model.block,
                offset: model.offset,
            },
            origin: [
                stream
                    .f32_at(inst, ORIGIN_OFF)
                    .map_err(|_| ModelMeshError::MissingStaticModelInstances)?,
                stream
                    .f32_at(inst, ORIGIN_OFF + 4)
                    .map_err(|_| ModelMeshError::MissingStaticModelInstances)?,
                stream
                    .f32_at(inst, ORIGIN_OFF + 8)
                    .map_err(|_| ModelMeshError::MissingStaticModelInstances)?,
            ],
            axis,
            scale: stream
                .f32_at(inst, SCALE_OFF)
                .map_err(|_| ModelMeshError::MissingStaticModelInstances)?,

            cull_dist: {
                let dist = stream
                    .f32_at(inst, CULL_DIST_OFF)
                    .map_err(|_| ModelMeshError::MissingStaticModelInstances)?;
                if dist.is_finite() && (0.0..=f32::from(u16::MAX)).contains(&dist) {
                    dist as u16
                } else {
                    0
                }
            },

            reflection_probe_index: stream
                .u8_at(inst, 0x4a)
                .map_err(|_| ModelMeshError::MissingStaticModelInstances)?,
            primary_light_index: stream
                .u8_at(inst, 0x4b)
                .map_err(|_| ModelMeshError::MissingStaticModelInstances)?,

            flags: 0,
        });
    }
    Ok(out)
}

pub fn build_t5_xmodel_mesh(
    stream: &ext_t5_fastfile::ZoneStream<'_>,
    geometry: ext_t5_fastfile::XModelGeometry,
    materials: Option<&MaterialCatalog>,
) -> Result<ModelMesh, ModelMeshError> {
    let name = geometry
        .name
        .map(|ptr| stream.cstr(ptr))
        .transpose()
        .map_err(|_| ModelMeshError::MissingName)?
        .ok_or(ModelMeshError::MissingName)?
        .to_owned();
    let surfaces = geometry.surfaces.ok_or(ModelMeshError::MissingSurfaces)?;

    let mut lod_surfaces: [Vec<ModelSurfaceDraw>; 4] = Default::default();
    let mut total_vertices = 0usize;
    let mut total_triangles = 0usize;
    for lod in 0..4 {
        let (first_surface, surface_count) = geometry.lod_surf_span[lod];
        let first_surface = usize::from(first_surface);
        if surface_count == 0 || first_surface + usize::from(surface_count) > geometry.surface_count
        {
            continue;
        }
        let (draws, verts, tris) = decode_t5_xsurfaces(
            stream,
            surfaces,
            first_surface,
            usize::from(surface_count),
            geometry.material_handles,
            materials,
        )?;
        total_vertices += verts;
        total_triangles += tris;
        lod_surfaces[lod] = draws;
    }
    if lod_surfaces.iter().all(|lod| lod.is_empty()) {
        return Err(ModelMeshError::MissingSurfaces);
    }

    Ok(ModelMesh {
        name,
        lod_surfaces,
        vertices: total_vertices,
        triangles: total_triangles,
        lod_smc: None,
        lod: Some(asset_model::ModelLodSelector {
            family: crate::ID,
            lod_start: 0,
            num_lods: geometry.num_lods,
            lod_dist: geometry.lod_dist,
        }),
    })
}

#[allow(clippy::type_complexity)]
fn decode_t5_xsurfaces(
    stream: &ext_t5_fastfile::ZoneStream<'_>,
    surfaces: ext_t5_fastfile::Ptr,
    first_surface: usize,
    surface_count: usize,
    material_handles: Option<ext_t5_fastfile::Ptr>,
    materials: Option<&MaterialCatalog>,
) -> Result<(Vec<ModelSurfaceDraw>, usize, usize), ModelMeshError> {
    const XSURFACE: usize = 68;
    const VERT_COUNT_OFF: usize = 4;
    const TRI_COUNT_OFF: usize = 6;
    const TRIANGLES_OFF: usize = 12;
    const VERTICES_OFF: usize = 32;
    const PACKED_VERTEX: usize = 32;

    let end_surface = first_surface + surface_count;
    let mut draws = Vec::with_capacity(surface_count);
    let mut total_vertices = 0usize;
    let mut total_triangles = 0usize;

    for surface_index in first_surface..end_surface {
        let surface = surfaces.at(surface_index * XSURFACE);
        let vertex_count = stream
            .u16_at(surface, VERT_COUNT_OFF)
            .map_err(|_| ModelMeshError::MissingSurfaces)? as usize;
        let tri_count = stream
            .u16_at(surface, TRI_COUNT_OFF)
            .map_err(|_| ModelMeshError::MissingSurfaces)? as usize;
        let vertices = resolved_ptr_t5(stream, surface, VERTICES_OFF)?;
        let triangles = resolved_ptr_t5(stream, surface, TRIANGLES_OFF)?;

        let mut positions = Vec::with_capacity(vertex_count);
        let mut normals = Vec::with_capacity(vertex_count);
        let mut colors = Vec::with_capacity(vertex_count);
        let mut uvs = Vec::with_capacity(vertex_count);
        let mut packed_vertices = Vec::with_capacity(vertex_count);
        let mut indices = Vec::with_capacity(tri_count * 3);

        for vertex_index in 0..vertex_count {
            let vertex = vertices.at(vertex_index * PACKED_VERTEX);
            let mut packed = [0u8; PACKED_VERTEX];
            for (offset, byte) in packed.iter_mut().enumerate() {
                *byte = stream
                    .u8_at(vertex, offset)
                    .map_err(|_| ModelMeshError::MissingSurfaces)?;
            }
            packed_vertices.push(packed);
            positions.push([
                stream
                    .f32_at(vertex, 0)
                    .map_err(|_| ModelMeshError::MissingSurfaces)?,
                stream
                    .f32_at(vertex, 4)
                    .map_err(|_| ModelMeshError::MissingSurfaces)?,
                stream
                    .f32_at(vertex, 8)
                    .map_err(|_| ModelMeshError::MissingSurfaces)?,
            ]);
            colors.push(unpack_color(
                stream
                    .u32_at(vertex, 16)
                    .map_err(|_| ModelMeshError::MissingSurfaces)?,
            ));
            uvs.push(unpack_packed_tex_coords(
                stream
                    .u32_at(vertex, 20)
                    .map_err(|_| ModelMeshError::MissingSurfaces)?,
            ));
            normals.push(normalize_or_up(unpack_unit_vec(
                stream
                    .u32_at(vertex, 24)
                    .map_err(|_| ModelMeshError::MissingSurfaces)?,
            )));
        }
        for index_offset in 0..tri_count * 3 {
            let index = stream
                .u16_at(triangles, index_offset * 2)
                .map_err(|_| ModelMeshError::MissingSurfaces)?;
            if usize::from(index) >= vertex_count {
                return Err(ModelMeshError::InvalidIndex {
                    surface: surface_index,
                    index,
                    vertices: vertex_count,
                });
            }
            indices.push(u32::from(index));
        }

        let mut mesh = Mesh::new(
            PrimitiveTopology::TriangleList,
            RenderAssetUsages::MAIN_WORLD | RenderAssetUsages::RENDER_WORLD,
        );
        mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, positions);
        mesh.insert_attribute(Mesh::ATTRIBUTE_NORMAL, normals);
        mesh.insert_attribute(Mesh::ATTRIBUTE_UV_0, uvs);
        mesh.insert_attribute(Mesh::ATTRIBUTE_COLOR, colors);
        mesh.insert_indices(Indices::U32(indices));

        let material = material_handles.and_then(|handles| {
            let slot = AssetPointerIdentity {
                block: handles.block,
                offset: handles.offset + (surface_index * 4) as u32,
            };
            materials?.material_index(slot).map(|i| i.get())
        });

        total_vertices += vertex_count;
        total_triangles += tri_count;
        draws.push(ModelSurfaceDraw {
            mesh,
            material,
            packed_vertices,

            xsurface_plus_1: Some(u8::from(
                stream
                    .u16_at(surface, 2)
                    .map_err(|_| ModelMeshError::MissingSurfaces)?
                    & 0x80
                    != 0,
            )),
            xsurface_base_index: 0,
            xsurface_vert_offset: 0,
            collision: capture_t5_xsurface_collision(stream, surface, surface_index, tri_count)?,
        });
    }

    Ok((draws, total_vertices, total_triangles))
}

fn capture_t5_xsurface_collision(
    stream: &ext_t5_fastfile::ZoneStream<'_>,
    surface: ext_t5_fastfile::Ptr,
    surface_index: usize,
    surface_tri_count: usize,
) -> Result<XSurfaceCollisionPayload, ModelMeshError> {
    let count = stream.u8_at(surface, 1).map_err(native_error)? as usize;
    if count == 0 {
        return Ok(XSurfaceCollisionPayload::Iw4(Vec::new()));
    }
    let lists = resolved_ptr_t5(stream, surface, 40)?;
    stream
        .slice_at(lists, 0, count * ext_t5_fastfile::size::XRIGID_VERT_LIST)
        .map_err(native_error)?;
    let mut trees = Vec::with_capacity(count);
    for list_index in 0..count {
        let list = lists.at(list_index * ext_t5_fastfile::size::XRIGID_VERT_LIST);
        let tri_offset = stream.u16_at(list, 4).map_err(native_error)?;
        let tri_count = stream.u16_at(list, 6).map_err(native_error)?;
        let tri_end = usize::from(tri_offset)
            .checked_add(usize::from(tri_count))
            .filter(|end| *end <= surface_tri_count)
            .ok_or(ModelMeshError::InvalidCollisionTree {
                surface: surface_index,
                vert_list: list_index,
                reason: asset_iw4::XSurfaceCollisionRangeError::RigidTriangleRange,
            })?;
        let tree = match stream.ptr_at(list, 8).map_err(native_error)? {
            ext_t5_fastfile::ZonePtr::Null => None,
            ext_t5_fastfile::ZonePtr::Offset(tree) => {
                let tree = capture_t5_collision_tree(stream, stream.resolve_alias(tree))?;
                asset_iw4::validate_xsurface_collision_ranges(
                    &tree.nodes,
                    &tree.leafs,
                    usize::from(tri_offset),
                    tri_end,
                )
                .map_err(|reason| ModelMeshError::InvalidCollisionTree {
                    surface: surface_index,
                    vert_list: list_index,
                    reason,
                })?;
                Some(tree)
            }
            ext_t5_fastfile::ZonePtr::Following | ext_t5_fastfile::ZonePtr::Insert => {
                return Err(ModelMeshError::MissingSurfaces);
            }
        };
        trees.push(OwnedXRigidVertListCollision {
            tri_offset,
            tri_count,
            tree,
        });
    }
    Ok(XSurfaceCollisionPayload::Iw4(trees))
}

fn capture_t5_collision_tree(
    stream: &ext_t5_fastfile::ZoneStream<'_>,
    tree: ext_t5_fastfile::Ptr,
) -> Result<OwnedXSurfaceCollisionTree, ModelMeshError> {
    let mut trans = [0.0; 3];
    let mut scale = [0.0; 3];
    for axis in 0..3 {
        trans[axis] = stream.f32_at(tree, axis * 4).map_err(native_error)?;
        scale[axis] = stream.f32_at(tree, 12 + axis * 4).map_err(native_error)?;
    }
    let node_count = stream.u32_at(tree, 24).map_err(native_error)? as usize;
    let leaf_count = stream.u32_at(tree, 32).map_err(native_error)? as usize;
    let nodes_ptr = if node_count == 0 {
        None
    } else {
        let rows = resolved_ptr_t5(stream, tree, 28)?;
        stream
            .slice_at(
                rows,
                0,
                node_count * asset_iw4::size::XSURFACE_COLLISION_NODE,
            )
            .map_err(native_error)?;
        Some(rows)
    };
    let leafs_ptr = if leaf_count == 0 {
        None
    } else {
        let rows = resolved_ptr_t5(stream, tree, 36)?;
        stream
            .slice_at(
                rows,
                0,
                leaf_count * asset_iw4::size::XSURFACE_COLLISION_LEAF,
            )
            .map_err(native_error)?;
        Some(rows)
    };
    let mut nodes = Vec::with_capacity(node_count);
    if let Some(rows) = nodes_ptr {
        for index in 0..node_count {
            let row = rows.at(index * asset_iw4::size::XSURFACE_COLLISION_NODE);
            nodes.push(asset_iw4::XSurfaceCollisionNode {
                mins: [
                    stream.u16_at(row, 0).map_err(native_error)?,
                    stream.u16_at(row, 2).map_err(native_error)?,
                    stream.u16_at(row, 4).map_err(native_error)?,
                ],
                maxs: [
                    stream.u16_at(row, 6).map_err(native_error)?,
                    stream.u16_at(row, 8).map_err(native_error)?,
                    stream.u16_at(row, 10).map_err(native_error)?,
                ],
                child_begin_index: stream.u16_at(row, 12).map_err(native_error)?,
                child_count: stream.u16_at(row, 14).map_err(native_error)?,
            });
        }
    }
    let mut leafs = Vec::with_capacity(leaf_count);
    if let Some(rows) = leafs_ptr {
        for index in 0..leaf_count {
            leafs.push(asset_iw4::XSurfaceCollisionLeaf {
                triangle_begin_index: stream
                    .u16_at(rows, index * asset_iw4::size::XSURFACE_COLLISION_LEAF)
                    .map_err(native_error)?,
            });
        }
    }
    Ok(OwnedXSurfaceCollisionTree {
        trans,
        scale,
        nodes,
        leafs,
    })
}

fn resolved_ptr_t5(
    stream: &ext_t5_fastfile::ZoneStream<'_>,
    parent: ext_t5_fastfile::Ptr,
    field: usize,
) -> Result<ext_t5_fastfile::Ptr, ModelMeshError> {
    match stream
        .ptr_at(parent, field)
        .map_err(|_| ModelMeshError::MissingSurfaces)?
    {
        ext_t5_fastfile::ZonePtr::Offset(ptr) => Ok(stream.resolve_alias(ptr)),
        _ => Err(ModelMeshError::MissingSurfaces),
    }
}

fn native_error(error: ext_t5_fastfile::ZoneError) -> ModelMeshError {
    ModelMeshError::Source {
        family: crate::ID,
        detail: error.to_string(),
    }
}
