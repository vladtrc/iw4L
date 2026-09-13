pub const CODE_MESH_VERT_CAP: u32 = 0x4000;

pub const CODE_MESH_INDEX_CAP: u32 = 0x6000;

pub const CODE_MESH_ARGS_CAP: u32 = 0x100;

pub const CODE_MESH_VERT_STRIDE: u32 = 0x20;

pub const CODE_MESH_ARGS_STRIDE: u32 = 0x10;

pub const CODE_MESH_WARN_VERTS: u32 = 0x24;

pub const CODE_MESH_WARN_INDS: u32 = 0x23;

pub const CODE_MESH_WARN_ARGS: u32 = 0x25;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct GfxMeshData {
    pub vert_used: u32,

    pub index_used: u32,

    pub args_used: u32,

    pub vert_cap: u32,

    pub index_cap: u32,

    pub args_cap: u32,

    pub warn_verts: u32,

    pub warn_inds: u32,

    pub warn_args: u32,
}

impl GfxMeshData {
    pub fn code_mesh() -> Self {
        Self {
            vert_used: 0,
            index_used: 0,
            args_used: 0,
            vert_cap: 0x4000,
            index_cap: 0x6000,
            args_cap: 0x100,
            warn_verts: 0x24,
            warn_inds: 0x23,
            warn_args: 0x25,
        }
    }
}

impl Default for GfxMeshData {
    fn default() -> Self {
        Self::code_mesh()
    }
}

pub fn r_reserve_code_mesh_verts(mesh: &mut GfxMeshData, count: u32) -> Option<u16> {
    let next = mesh.vert_used.wrapping_add(count);
    if mesh.vert_cap < next {
        return None;
    }
    let base = mesh.vert_used as u16;
    mesh.vert_used = next;
    Some(base)
}

pub fn r_reserve_code_mesh_indices(mesh: &mut GfxMeshData, count: u32) -> Option<u32> {
    let next = mesh.index_used.wrapping_add(count);
    if mesh.index_cap < next {
        return None;
    }
    let base = mesh.index_used;
    mesh.index_used = next;
    Some(base)
}

pub fn r_shrink_code_mesh_verts(mesh: &mut GfxMeshData, count: u32) {
    mesh.vert_used = mesh.vert_used.wrapping_sub(count);
}

pub fn r_get_code_mesh_verts(verts_base: usize, base_vertex: u16) -> usize {
    verts_base.wrapping_add(usize::from(base_vertex).wrapping_mul(0x20))
}

pub fn r_get_code_mesh_args(args_base: usize, slot: u32) -> usize {
    (slot as usize).wrapping_mul(0x10).wrapping_add(args_base)
}

pub fn r_reserve_code_mesh(
    mesh: &mut GfxMeshData,
    verts: u32,
    inds: u32,
    args: u32,
) -> Option<(u16, u32, Option<u32>)> {
    if mesh.vert_cap < mesh.vert_used.wrapping_add(verts) {
        return None;
    }
    if mesh.index_cap < mesh.index_used.wrapping_add(inds) {
        return None;
    }
    if args != 0 && mesh.args_cap < mesh.args_used.wrapping_add(args) {
        return None;
    }
    let vert_base = mesh.vert_used as u16;
    let index_base = mesh.index_used;
    mesh.vert_used = mesh.vert_used.wrapping_add(verts);
    mesh.index_used = mesh.index_used.wrapping_add(inds);
    let arg_base = if args != 0 {
        let base = mesh.args_used;
        mesh.args_used = mesh.args_used.wrapping_add(args);
        Some(base)
    } else {
        None
    };
    Some((vert_base, index_base, arg_base))
}
