use bevy::mesh::{Indices, VertexAttributeValues};
use bevy::prelude::*;
use render_frame::SmodelVertex;

pub fn install_retained_packed(
    packed_ok: bool,
    packed: Vec<[u8; asset_iw4::size::GFX_PACKED_VERTEX]>,
    decoded_count: usize,
    empty: &'static str,
    missing: &'static str,
) -> asset_world::PackedVertexPayload {
    if packed_ok && packed.len() == decoded_count && !packed.is_empty() {
        asset_world::PackedVertexPayload::Iw4(packed)
    } else if decoded_count == 0 {
        asset_world::PackedVertexPayload::Unavailable {
            source_layout: empty,
        }
    } else {
        asset_world::PackedVertexPayload::Unavailable {
            source_layout: missing,
        }
    }
}

pub(crate) fn f32x3(values: &VertexAttributeValues) -> Option<&[[f32; 3]]> {
    match values {
        VertexAttributeValues::Float32x3(v) => Some(v.as_slice()),
        _ => None,
    }
}

pub(crate) fn f32x2(values: &VertexAttributeValues) -> Option<&[[f32; 2]]> {
    match values {
        VertexAttributeValues::Float32x2(v) => Some(v.as_slice()),
        _ => None,
    }
}

pub(crate) fn f32x4(values: &VertexAttributeValues) -> Option<&[[f32; 4]]> {
    match values {
        VertexAttributeValues::Float32x4(v) => Some(v.as_slice()),
        _ => None,
    }
}

pub fn append_mesh(
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
    for (i, positions_entry) in positions[..n].iter().enumerate() {
        vertices.push(SmodelVertex {
            position: (*positions_entry),
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
