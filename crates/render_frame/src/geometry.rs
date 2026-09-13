#[repr(C)]
#[derive(Clone, Copy, Debug, bytemuck::Pod, bytemuck::Zeroable)]
pub struct SmodelVertex {
    pub position: [f32; 3],
    pub normal: [f32; 3],
    pub color: [f32; 4],
    pub uv0: [f32; 2],
}

#[repr(C)]
#[derive(Clone, Copy, Debug, bytemuck::Pod, bytemuck::Zeroable)]
pub struct WorldVertex {
    pub position: [f32; 3],
    pub normal: [f32; 3],
    pub tangent: [f32; 4],
    pub color: [f32; 4],
    pub uv0: [f32; 2],
    pub uv1: [f32; 2],
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RetailPackedVertexRefusal {
    ForeignLayout { source_layout: &'static str },
    VertexCountMismatch { retail: usize, decoded: usize },
    RetailStrideMismatch { table_stride: Option<u16> },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RetailWorldVertexRefusal {
    ForeignLayout { source_layout: &'static str },
    VertexCountMismatch { retail: usize, decoded: usize },
    RetailStrideMismatch { table_stride: Option<u16> },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct SurfaceReflectionProbeId(pub u8);

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct SurfaceLightmapId(pub u8);

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct SurfaceSamplerInputs {
    pub reflection_probe: Option<SurfaceReflectionProbeId>,
    pub primary_lightmap: Option<SurfaceLightmapId>,
    pub secondary_lightmap: Option<SurfaceLightmapId>,
}

pub const DYNAMIC_INDEX_BUFFER_CAPACITY: u32 = 0x100000;

#[must_use]
pub const fn xmodel_tess_info_packed_arm(tess_info_byte_10: u8) -> bool {
    matches!(tess_info_byte_10, 1 | 3 | 4)
}

#[must_use]
pub const fn xmodel_tess_info_vert_decl_type(tess_info_byte_10: u8) -> u8 {
    2 - xmodel_tess_info_packed_arm(tess_info_byte_10) as u8
}
