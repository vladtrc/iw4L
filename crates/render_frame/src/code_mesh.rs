pub const CODE_MESH_VERT_CAP: u32 = 0x4000;

pub const CODE_MESH_INDEX_CAP: u32 = 0x6000;

pub const CODE_MESH_ARGS_CAP: u32 = 0x100;

pub const CODE_MESH_VERT_STRIDE: u32 = 0x20;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct GfxMeshData {
    pub vert_used: u32,

    pub index_used: u32,

    pub args_used: u32,

    pub vert_cap: u32,

    pub index_cap: u32,

    pub args_cap: u32,
}

impl GfxMeshData {
    pub fn code_mesh() -> Self {
        Self {
            vert_used: 0,
            index_used: 0,
            args_used: 0,
            vert_cap: CODE_MESH_VERT_CAP,
            index_cap: CODE_MESH_INDEX_CAP,
            args_cap: CODE_MESH_ARGS_CAP,
        }
    }
}

impl Default for GfxMeshData {
    fn default() -> Self {
        Self::code_mesh()
    }
}

// Keep the whole reservation in the address space of its u16 vertex base.
fn next_vertex(mesh: &GfxMeshData, count: u32) -> Option<(u16, u32)> {
    let base = u16::try_from(mesh.vert_used).ok()?;
    let next = mesh.vert_used.checked_add(count)?;
    (next <= mesh.vert_cap && next <= u32::from(u16::MAX) + 1).then_some((base, next))
}

pub fn reserve_code_mesh_verts(mesh: &mut GfxMeshData, count: u32) -> Option<u16> {
    let (base, next) = next_vertex(mesh, count)?;
    mesh.vert_used = next;
    Some(base)
}

pub fn reserve_code_mesh_indices(mesh: &mut GfxMeshData, count: u32) -> Option<u32> {
    let next = mesh.index_used.checked_add(count)?;
    if next > mesh.index_cap {
        return None;
    }
    let base = mesh.index_used;
    mesh.index_used = next;
    Some(base)
}

/// Reserves all three ranges together, leaving counters unchanged on refusal.
pub fn reserve_code_mesh(
    mesh: &mut GfxMeshData,
    verts: u32,
    inds: u32,
    args: u32,
) -> Option<(u16, u32, Option<u32>)> {
    let (vert_base, vert_next) = next_vertex(mesh, verts)?;
    let index_next = mesh.index_used.checked_add(inds)?;
    let args_next = mesh.args_used.checked_add(args)?;
    if index_next > mesh.index_cap || args_next > mesh.args_cap {
        return None;
    }
    let index_base = mesh.index_used;
    let arg_base = (args != 0).then_some(mesh.args_used);
    mesh.vert_used = vert_next;
    mesh.index_used = index_next;
    mesh.args_used = args_next;
    Some((vert_base, index_base, arg_base))
}
