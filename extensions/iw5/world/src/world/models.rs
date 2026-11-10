use asset_material::{AssetPointerIdentity, MaterialCatalog};
use asset_model::{normalize_or_up, unpack_color, unpack_packed_tex_coords, unpack_unit_vec};
use asset_world::*;
use bevy::asset::RenderAssetUsages;
use bevy::prelude::*;
use bevy::render::mesh::{Indices, PrimitiveTopology};
use lighting_iw4::GFX_STATIC_MODEL_DRAW_INST_FLAGS;
pub fn build_iw5_static_model_instances(
    stream: &ext_iw5_fastfile::ZoneStream<'_>,
    world: ext_iw5_fastfile::GfxWorldGeometry,
) -> Result<Vec<StaticModelInstance>, ModelMeshError> {
    let (Some(draw_insts), Some(_insts)) = (world.smodel_draw_insts, world.smodel_insts) else {
        return Err(ModelMeshError::MissingStaticModelInstances);
    };
    let draw_inst = stream.layout(ext_iw5_fastfile::size::GFX_STATIC_MODEL_DRAW_INST, 88);
    let model_off = stream.layout(0x34, 56);
    let mut out = Vec::with_capacity(world.smodel_count);
    for index in 0..world.smodel_count {
        let inst = draw_insts.at(index * draw_inst);
        let mut axis = [[0.0; 3]; 3];
        for (row, values) in axis.iter_mut().enumerate() {
            for (column, value) in values.iter_mut().enumerate() {
                *value = stream
                    .f32_at(inst, 12 + (row * 3 + column) * 4)
                    .map_err(|_| ModelMeshError::MissingStaticModelInstances)?;
            }
        }
        out.push(StaticModelInstance {
            model_slot: AssetPointerIdentity {
                block: inst.block,
                offset: inst.offset + model_off as u32,
            },
            origin: [
                stream
                    .f32_at(inst, 0)
                    .map_err(|_| ModelMeshError::MissingStaticModelInstances)?,
                stream
                    .f32_at(inst, 4)
                    .map_err(|_| ModelMeshError::MissingStaticModelInstances)?,
                stream
                    .f32_at(inst, 8)
                    .map_err(|_| ModelMeshError::MissingStaticModelInstances)?,
            ],
            axis,
            scale: stream
                .f32_at(inst, 0x30)
                .map_err(|_| ModelMeshError::MissingStaticModelInstances)?,
            cull_dist: stream
                .u16_at(inst, stream.layout(0x38, 64))
                .map_err(|_| ModelMeshError::MissingStaticModelInstances)?,
            reflection_probe_index: stream
                .u8_at(inst, stream.layout(0x3c, 68))
                .map_err(|_| ModelMeshError::MissingStaticModelInstances)?,
            primary_light_index: stream
                .u8_at(inst, stream.layout(0x3d, 69))
                .map_err(|_| ModelMeshError::MissingStaticModelInstances)?,
            flags: stream
                .u8_at(inst, stream.layout(GFX_STATIC_MODEL_DRAW_INST_FLAGS, 70))
                .map_err(|_| ModelMeshError::MissingStaticModelInstances)?,
        });
    }
    Ok(out)
}

pub fn build_iw5_xmodel_mesh(
    stream: &ext_iw5_fastfile::ZoneStream<'_>,
    geometry: ext_iw5_fastfile::XModelGeometry,
    materials: Option<&MaterialCatalog>,
) -> Result<ModelMesh, ModelMeshError> {
    use ext_iw5_fastfile::size as iw5sz;

    let name = geometry
        .name
        .map(|ptr| stream.cstr(ptr))
        .transpose()
        .map_err(|_| ModelMeshError::MissingName)?
        .ok_or(ModelMeshError::MissingName)?
        .to_owned();
    let surfaces = geometry.surfaces.ok_or(ModelMeshError::MissingSurfaces)?;

    let mut draws = Vec::with_capacity(geometry.surface_count);
    let mut total_vertices = 0usize;
    let mut total_triangles = 0usize;

    for surface_index in 0..geometry.surface_count {
        let surface = surfaces.at(surface_index * stream.layout(iw5sz::XSURFACE, 88));
        let vertex_count = stream
            .u16_at(surface, 2)
            .map_err(|_| ModelMeshError::MissingSurfaces)? as usize;
        let tri_count = stream
            .u16_at(surface, 4)
            .map_err(|_| ModelMeshError::MissingSurfaces)? as usize;
        let vertices = resolved_ptr_iw5(
            stream,
            surface,
            stream.layout(iw5sz::XSURFACE_VERTS0_OFF, 40),
        )?;
        let triangles = resolved_ptr_iw5(stream, surface, iw5sz::XSURFACE_TRI_INDICES_OFF)?;
        let xsurface_flags = stream
            .u8_at(surface, iw5sz::XSURFACE_FLAGS_OFF)
            .map_err(|_| ModelMeshError::MissingSurfaces)?;

        let mut positions = Vec::with_capacity(vertex_count);
        let mut normals = Vec::with_capacity(vertex_count);
        let mut colors = Vec::with_capacity(vertex_count);
        let mut uvs = Vec::with_capacity(vertex_count);
        let mut packed_vertices = Vec::with_capacity(vertex_count);
        let mut indices = Vec::with_capacity(tri_count * 3);

        for vertex_index in 0..vertex_count {
            let vertex = vertices.at(vertex_index * iw5sz::GFX_PACKED_VERTEX);
            let mut packed = [0u8; iw5sz::GFX_PACKED_VERTEX];
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

        let material = geometry.material_handles.and_then(|handles| {
            let slot = AssetPointerIdentity {
                block: handles.block,
                offset: handles.offset + (surface_index * stream.pointer_bytes()) as u32,
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
                xsurface_flags & iw5sz::XSURFACE_FLAG_DEFORMED != 0,
            )),
            xsurface_base_index: 0,
            xsurface_vert_offset: 0,
            collision: XSurfaceCollisionPayload::Unavailable {
                source_layout: "IW5 XSurface collision layout not retained",
            },
        });
    }

    Ok(ModelMesh {
        name,
        lod_surfaces: {
            let mut lods: [Vec<ModelSurfaceDraw>; 4] = Default::default();
            lods[0] = draws;
            lods
        },
        vertices: total_vertices,
        triangles: total_triangles,
        lod_smc: None,

        lod: None,
    })
}

fn resolved_ptr_iw5(
    stream: &ext_iw5_fastfile::ZoneStream<'_>,
    parent: ext_iw5_fastfile::Ptr,
    field: usize,
) -> Result<ext_iw5_fastfile::Ptr, ModelMeshError> {
    match stream
        .ptr_at(parent, field)
        .map_err(|_| ModelMeshError::MissingSurfaces)?
    {
        ext_iw5_fastfile::ZonePtr::Offset(ptr) => Ok(stream.resolve_alias(ptr)),
        _ => Err(ModelMeshError::MissingSurfaces),
    }
}
