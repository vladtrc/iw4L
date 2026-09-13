pub const TRIANGLES_LIST_ENTRY_STRIDE: usize = 0x18;

pub const SMODEL_RIGID_ENTRY_STRIDE: usize = 0x10;

pub const XMODEL_RIGID_ENTRY_STRIDE: usize = 0x10;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SmodelPretessRange {
    pub start: u32,
    pub count: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct GfxSmodelRigidEntry {
    pub packed_key: u32,

    pub tri_count: u16,

    pub index_byte_offset: u32,

    pub lighting_handle: u16,
}

pub const fn smodel_rigid_index_run_continues(
    accum_byte: u32,
    accum_tri: u32,
    next: &GfxSmodelRigidEntry,
) -> bool {
    accum_byte + accum_tri * 6 == next.index_byte_offset
}

#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct GfxXModelRigidEntry {
    pub packed_key: u32,

    pub index_byte_offset: u32,

    pub tri_count: u16,

    pub tess_info_byte_10: u8,

    _pad_0b: u8,

    pub lighting_handle: u32,
}

impl GfxXModelRigidEntry {
    pub const fn new(
        packed_key: u32,
        index_byte_offset: u32,
        tri_count: u16,
        tess_info_byte_10: u8,
        lighting_handle: u32,
    ) -> Self {
        Self {
            packed_key,
            index_byte_offset,
            tri_count,
            tess_info_byte_10,
            _pad_0b: 0,
            lighting_handle,
        }
    }
}

pub const fn xmodel_rigid_index_run_continues(
    accum_byte: u32,
    accum_tri: u32,
    next: &GfxXModelRigidEntry,
) -> bool {
    accum_byte + accum_tri * 6 == next.index_byte_offset
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct GfxTrianglesListEntry {
    pub sort_key: u32,

    pub tri_count: u16,

    pub base_index: u32,

    pub first_vertex: u32,

    pub vertex_count: u32,
}

pub const fn triangles_list_run_continues(
    accum_base_index: u32,
    accum_tri_count: u32,
    accum_first_vertex: u32,
    next: &GfxTrianglesListEntry,
) -> bool {
    accum_base_index + accum_tri_count * 3 == next.base_index
        && accum_first_vertex == next.first_vertex
}
