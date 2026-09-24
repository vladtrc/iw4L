use bevy::mesh::{Indices, VertexAttributeValues};
use bevy::prelude::*;
use render_frame::SmodelVertex;
use render_scene::SmodelPassMaterial;

use crate::model_draw::{FxModelAssetDraw, FxModelDrawPlan};

pub const XMODEL_PACKED_UNAVAILABLE: &str =
    "xmodel merge GfxPackedVertex missing or count-mismatched; decoded float is not packed VB";
pub const XMODEL_PACKED_EMPTY_PLAN: &str = "xmodel plan has no vertices";

fn install_retained_packed(
    packed_ok: bool,
    packed: Vec<[u8; asset_iw4::size::GFX_PACKED_VERTEX]>,
    decoded_count: usize,
    empty: &'static str,
    missing: &'static str,
) -> assets::RetailPackedVertexPayload {
    if packed_ok && packed.len() == decoded_count && !packed.is_empty() {
        assets::RetailPackedVertexPayload::Iw4(packed)
    } else if decoded_count == 0 {
        assets::RetailPackedVertexPayload::Unavailable {
            source_layout: empty,
        }
    } else {
        assets::RetailPackedVertexPayload::Unavailable {
            source_layout: missing,
        }
    }
}

fn f32x3(values: &VertexAttributeValues) -> Option<&[[f32; 3]]> {
    match values {
        VertexAttributeValues::Float32x3(v) => Some(v.as_slice()),
        _ => None,
    }
}

fn f32x2(values: &VertexAttributeValues) -> Option<&[[f32; 2]]> {
    match values {
        VertexAttributeValues::Float32x2(v) => Some(v.as_slice()),
        _ => None,
    }
}

fn f32x4(values: &VertexAttributeValues) -> Option<&[[f32; 4]]> {
    match values {
        VertexAttributeValues::Float32x4(v) => Some(v.as_slice()),
        _ => None,
    }
}

fn append_mesh(
    mesh: &Mesh,
    vertices: &mut Vec<SmodelVertex>,
    indices: &mut Vec<u32>,
) -> Option<(u32, u32)> {
    let positions = mesh.attribute(Mesh::ATTRIBUTE_POSITION).and_then(f32x3)?;
    let normals = mesh.attribute(Mesh::ATTRIBUTE_NORMAL).and_then(f32x3);
    let uvs = mesh.attribute(Mesh::ATTRIBUTE_UV_0).and_then(f32x2);
    let colors = mesh.attribute(Mesh::ATTRIBUTE_COLOR).and_then(f32x4);
    let mesh_indices = mesh.indices()?;
    let count = match mesh_indices {
        Indices::U32(ix) => ix.len(),
        Indices::U16(ix) => ix.len(),
    };
    let n = positions.len();
    if n == 0 || count == 0 {
        return None;
    }
    let indices_in_range = match mesh_indices {
        Indices::U32(ix) => ix.iter().all(|&index| (index as usize) < n),
        Indices::U16(ix) => ix.iter().all(|&index| usize::from(index) < n),
    };
    if !indices_in_range {
        return None;
    }

    let base = vertices.len() as u32;
    for i in 0..n {
        vertices.push(SmodelVertex {
            position: positions[i],
            normal: normals
                .and_then(|a| a.get(i).copied())
                .unwrap_or([0.0, 0.0, 1.0]),
            color: colors.and_then(|a| a.get(i).copied()).unwrap_or([1.0; 4]),
            uv0: uvs.and_then(|a| a.get(i).copied()).unwrap_or([0.0; 2]),
        });
    }
    let index_start = indices.len() as u32;
    match mesh_indices {
        Indices::U32(ix) => indices.extend(ix.iter().map(|&index| base + index)),
        Indices::U16(ix) => indices.extend(ix.iter().map(|&index| base + u32::from(index))),
    }
    Some((index_start, count as u32))
}

pub fn append_fx_model_asset(
    plan: &mut FxModelDrawPlan,
    model_index: usize,
    lod: u8,
    surfaces: &[render_anim::fpv_pose::PosedModelSurface],
    materials: &[Option<SmodelPassMaterial>],
) -> Vec<(u32, u32)> {
    let geometry = std::sync::Arc::make_mut(&mut plan.geometry);
    let mut asset_surfaces = Vec::new();
    let vertices_empty = geometry.vertices.is_empty();
    let mut packed_ok = vertices_empty
        || matches!(
            geometry.packed_vertices,
            assets::RetailPackedVertexPayload::Iw4(_)
        );
    let mut packed = match std::mem::replace(
        &mut geometry.packed_vertices,
        assets::RetailPackedVertexPayload::Unavailable {
            source_layout: XMODEL_PACKED_UNAVAILABLE,
        },
    ) {
        assets::RetailPackedVertexPayload::Iw4(rows) => rows,
        assets::RetailPackedVertexPayload::Unavailable { .. } => Vec::new(),
    };
    for (surface, material) in surfaces.iter().zip(materials) {
        let Some(material) = material else { continue };
        let before = geometry.vertices.len();
        let Some((start, count)) =
            append_mesh(&surface.mesh, &mut geometry.vertices, &mut geometry.indices)
        else {
            continue;
        };
        let decoded = geometry.vertices.len() - before;
        if packed_ok && surface.packed_vertices.len() == decoded {
            packed.extend_from_slice(&surface.packed_vertices);
        } else {
            packed_ok = false;
            packed.clear();
        }
        let surface_index = geometry.surface_ranges.len() as u32;
        geometry.surface_ranges.push((start, count));
        let material_index = geometry.materials.len() as u32;
        geometry.materials.push(material.clone());
        asset_surfaces.push((surface_index, material_index));
    }
    geometry.packed_vertices = install_retained_packed(
        packed_ok,
        packed,
        geometry.vertices.len(),
        XMODEL_PACKED_EMPTY_PLAN,
        XMODEL_PACKED_UNAVAILABLE,
    );
    geometry.assets.push(FxModelAssetDraw {
        model_index,
        lod,
        surfaces: asset_surfaces.clone(),
    });
    let topology = {
        let mut revisions = render_frame::SourceRevisions::default();
        revisions.set_topology_from(
            &geometry.indices,
            &geometry.surface_ranges,
            geometry.vertices.len(),
        );
        revisions.topology
    };
    let mut rev = plan.revision;
    plan.revisions.topology = topology;
    plan.revisions.bump_packed_write(&mut rev);
    plan.revision = rev;
    asset_surfaces
}
