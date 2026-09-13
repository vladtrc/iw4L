use bevy::asset::RenderAssetUsages;
use bevy::prelude::*;
use bevy::render::mesh::{Indices, PrimitiveTopology};
use fastfile_iw4::{GfxWorldGeometry, ZoneStream};

use asset_iw4::size as sz;
pub use asset_model::{half_to_f32, unpack_packed_tex_coords};
pub(crate) use asset_model::{normalize_or_up, unpack_color, unpack_unit_vec};

pub fn bumped_world_normal(
    normal_ts: [f32; 2],
    geometric: [f32; 3],
    tangent: [f32; 4],
) -> [f32; 3] {
    let bitangent = [
        (geometric[1] * tangent[2] - geometric[2] * tangent[1]) * tangent[3],
        (geometric[2] * tangent[0] - geometric[0] * tangent[2]) * tangent[3],
        (geometric[0] * tangent[1] - geometric[1] * tangent[0]) * tangent[3],
    ];
    let bumped = [
        geometric[0] + normal_ts[0] * tangent[0] + normal_ts[1] * bitangent[0],
        geometric[1] + normal_ts[0] * tangent[1] + normal_ts[1] * bitangent[1],
        geometric[2] + normal_ts[0] * tangent[2] + normal_ts[1] * bitangent[2],
    ];
    normalize_or_up(bumped)
}

#[derive(Clone, Copy, Debug, Default)]
pub struct WorldMeshStats {
    pub vertices: usize,
    pub triangles: usize,
    pub surfaces: usize,

    pub skipped_surfaces: usize,

    pub sky_surfaces: usize,

    pub sky_material: Option<usize>,

    pub unrouted_surfaces: usize,

    pub undecided_state_bits_surfaces: usize,

    pub min: [f32; 3],
    pub max: [f32; 3],

    pub bounds: Option<[f32; 6]>,
}

#[derive(Debug)]
pub enum WorldMeshError {
    InvalidAabbChildrenOffset {
        offset: i32,
        stride: i32,
    },
    ConflictingVertexLayer {
        vertex: usize,
    },

    NoGeometry,

    NegativeBoundsHalf {
        table: BoundsTable,
        index: usize,
        axis: usize,
        half: f32,
    },
    Zone(fastfile_iw4::ZoneError),
    Iw5Zone(fastfile_iw5::ZoneError),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BoundsTable {
    AabbNode { cell: usize },

    Surface,

    SmodelInst,
}

impl std::fmt::Display for BoundsTable {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            BoundsTable::AabbNode { cell } => write!(f, "cell {cell} GfxAabbTree node"),
            BoundsTable::Surface => write!(f, "surface bounds"),
            BoundsTable::SmodelInst => write!(f, "GfxStaticModelInst"),
        }
    }
}

impl std::fmt::Display for WorldMeshError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            WorldMeshError::InvalidAabbChildrenOffset { offset, stride } => {
                write!(
                    f,
                    "AABB child byte displacement {offset} is not a multiple of source stride {stride}"
                )
            }
            WorldMeshError::ConflictingVertexLayer { vertex } => {
                write!(f, "T5 vertex {vertex} has conflicting layer records")
            }
            WorldMeshError::NoGeometry => write!(f, "zone has no GfxWorld geometry"),
            WorldMeshError::NegativeBoundsHalf {
                table,
                index,
                axis,
                half,
            } => write!(
                f,
                "{table} {index}: half[{axis}] = {half} is negative — the lane read \
                 this box in the wrong representation (mid/half vs mins/maxs)"
            ),
            WorldMeshError::Zone(e) => write!(f, "reading world geometry: {e}"),
            WorldMeshError::Iw5Zone(e) => write!(f, "reading IW5 world geometry: {e}"),
        }
    }
}

impl std::error::Error for WorldMeshError {}

impl From<fastfile_iw4::ZoneError> for WorldMeshError {
    fn from(e: fastfile_iw4::ZoneError) -> Self {
        Self::Zone(e)
    }
}

impl From<fastfile_iw5::ZoneError> for WorldMeshError {
    fn from(e: fastfile_iw5::ZoneError) -> Self {
        Self::Iw5Zone(e)
    }
}

pub fn build_world_mesh(
    s: &ZoneStream<'_>,
    geometry: GfxWorldGeometry,
) -> Result<(Mesh, WorldMeshStats), WorldMeshError> {
    let (Some(vertices), Some(indices)) = (geometry.vertices, geometry.indices) else {
        return Err(WorldMeshError::NoGeometry);
    };

    let mut positions = Vec::with_capacity(geometry.vertex_count);
    let mut normals = Vec::with_capacity(geometry.vertex_count);
    let mut colors = Vec::with_capacity(geometry.vertex_count);
    let mut min = [f32::INFINITY; 3];
    let mut max = [f32::NEG_INFINITY; 3];

    for i in 0..geometry.vertex_count {
        let v = vertices.at(i * sz::GFX_WORLD_VERTEX);
        let xyz = [s.f32_at(v, 0)?, s.f32_at(v, 4)?, s.f32_at(v, 8)?];
        for axis in 0..3 {
            min[axis] = min[axis].min(xyz[axis]);
            max[axis] = max[axis].max(xyz[axis]);
        }
        positions.push(xyz);
        normals.push(normalize_or_up(unpack_unit_vec(s.u32_at(v, 36)?)));
        colors.push(unpack_color(s.u32_at(v, 16)?));
    }

    let mut out_indices: Vec<u32> = Vec::new();
    let mut skipped_surfaces = 0usize;

    match geometry.surfaces {
        Some(surfaces) => {
            for i in 0..geometry.surface_count {
                let surface = surfaces.at(i * s.layout(sz::GFX_SURFACE, 32));
                let first_vertex = s.u32_at(surface, 4)? as usize;
                let vertex_count = s.u16_at(surface, 8)? as usize;
                let tri_count = s.u16_at(surface, 10)? as usize;
                let base_index = s.u32_at(surface, 12)? as usize;

                if first_vertex + vertex_count > geometry.vertex_count
                    || base_index + tri_count * 3 > geometry.index_count
                {
                    skipped_surfaces += 1;
                    continue;
                }

                for t in 0..tri_count * 3 {
                    let local = s.u16_at(indices, (base_index + t) * 2)? as usize;
                    let absolute = first_vertex + local;
                    if absolute >= geometry.vertex_count {
                        skipped_surfaces += 1;
                        break;
                    }
                    out_indices.push(absolute as u32);
                }
            }
        }
        None => return Err(WorldMeshError::NoGeometry),
    }

    let stats = WorldMeshStats {
        unrouted_surfaces: 0,
        undecided_state_bits_surfaces: 0,
        vertices: positions.len(),
        triangles: out_indices.len() / 3,
        surfaces: geometry.surface_count,
        skipped_surfaces,
        sky_surfaces: 0,
        sky_material: None,
        min,
        max,
        bounds: geometry.bounds.map(|bits| bits.map(f32::from_bits)),
    };

    let mut mesh = Mesh::new(
        PrimitiveTopology::TriangleList,
        RenderAssetUsages::default(),
    );
    mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, positions);
    mesh.insert_attribute(Mesh::ATTRIBUTE_NORMAL, normals);
    mesh.insert_attribute(Mesh::ATTRIBUTE_COLOR, colors);
    mesh.insert_indices(Indices::U32(out_indices));

    Ok((mesh, stats))
}
