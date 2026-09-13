use crate::{GFX_MARK_SURF_INDEX_LIMIT, GFX_MARK_SURF_LIMIT, GFX_MARK_SURF_VERT_LIMIT};

pub const GFX_MARK_SURF_STRIDE: usize = 0x10;

pub const GFX_MARK_SURF_TECHNIQUE_NIBBLE: u8 = 0xc;

pub const GFX_MARK_MESH_VERTEX_STRIDE: usize = 0x2c;

pub const GFX_MARK_MESH_BINORMAL_SIGN: f32 = -1.0;

pub const R_WARN_GFX_MARK_VERT_LIMIT: u32 = 0x2c;

pub const R_WARN_GFX_MARK_INDEX_LIMIT: u32 = 0x2b;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct GfxMarkMeshBudget {
    pub surf_n: u32,
    pub vert_n: u32,
    pub index_n: u32,
    pub surf_warn: u32,
    pub vert_warn: u32,
    pub index_warn: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GfxMarkMeshRefuse {
    SurfLimit,
    VertLimit,
    IndexLimit,
}

impl GfxMarkMeshRefuse {
    pub const fn warn_id(self) -> u32 {
        match self {
            Self::SurfLimit => crate::R_WARN_GFX_MARK_SURF_LIMIT,
            Self::VertLimit => R_WARN_GFX_MARK_VERT_LIMIT,
            Self::IndexLimit => R_WARN_GFX_MARK_INDEX_LIMIT,
        }
    }
}

#[inline]
pub const fn fx_mark_mesh_index_reserve(old_index_n: u32, tri_count: u8) -> u32 {
    let new_n = old_index_n.wrapping_add((tri_count as u32).wrapping_mul(3));
    let aligned_new = (new_n.wrapping_add(1)) & !1;
    let aligned_old = (old_index_n.wrapping_add(1)) & !1;
    aligned_new.wrapping_sub(aligned_old)
}

#[inline]
pub const fn fx_mark_context_is_world_list(context0: u8) -> bool {
    !matches!(context0, 1 | 2 | 3 | 4)
}

pub fn r_reserve_mark_mesh_verts(
    budget: &mut GfxMarkMeshBudget,
    vert_count: u16,
) -> Result<u16, GfxMarkMeshRefuse> {
    let next = budget.vert_n.saturating_add(u32::from(vert_count));
    if next > GFX_MARK_SURF_VERT_LIMIT {
        budget.vert_warn = budget.vert_warn.saturating_add(1);
        return Err(GfxMarkMeshRefuse::VertLimit);
    }
    let base = budget.vert_n as u16;
    budget.vert_n = next;
    Ok(base)
}

pub fn r_reserve_mark_mesh_indices(
    budget: &mut GfxMarkMeshBudget,
    index_count: u32,
) -> Result<u32, GfxMarkMeshRefuse> {
    let next = budget.index_n.saturating_add(index_count);
    if next > GFX_MARK_SURF_INDEX_LIMIT {
        budget.index_warn = budget.index_warn.saturating_add(1);
        return Err(GfxMarkMeshRefuse::IndexLimit);
    }
    let base = budget.index_n;
    budget.index_n = next;
    Ok(base)
}

pub fn r_add_mark_mesh_draw_surf(
    budget: &mut GfxMarkMeshBudget,
    _index_count: u32,
) -> Result<u32, GfxMarkMeshRefuse> {
    if budget.surf_n > GFX_MARK_SURF_LIMIT - 1 {
        budget.surf_warn = budget.surf_warn.saturating_add(1);
        return Err(GfxMarkMeshRefuse::SurfLimit);
    }
    let slot = budget.surf_n;
    budget.surf_n = budget.surf_n.saturating_add(1);
    Ok(slot)
}

pub fn fx_pack_mark_world_vertex(
    point: &crate::FxMarkStagingPoint,
    origin: [f32; 3],
    radius: f32,
    tex_coord_axis: [f32; 3],
    native_color: u32,
) -> [u8; GFX_MARK_MESH_VERTEX_STRIDE] {
    let delta = [
        point.xyz[0] - origin[0],
        point.xyz[1] - origin[1],
        point.xyz[2] - origin[2],
    ];
    let binormal = vec3_cross(tex_coord_axis, point.normal);
    let scale = if radius == 0.0 { 0.0 } else { 0.5 / radius };
    let tex_u = vec3_dot(delta, tex_coord_axis) * scale + 0.5;
    let tex_v = vec3_dot(delta, binormal) * scale + 0.5;
    let mut row = [0u8; GFX_MARK_MESH_VERTEX_STRIDE];
    row[0x00..0x04].copy_from_slice(&point.xyz[0].to_le_bytes());
    row[0x04..0x08].copy_from_slice(&point.xyz[1].to_le_bytes());
    row[0x08..0x0c].copy_from_slice(&point.xyz[2].to_le_bytes());
    row[0x0c..0x10].copy_from_slice(&GFX_MARK_MESH_BINORMAL_SIGN.to_le_bytes());
    row[0x10..0x14].copy_from_slice(&native_color.to_le_bytes());
    row[0x14..0x18].copy_from_slice(&tex_u.to_le_bytes());
    row[0x18..0x1c].copy_from_slice(&tex_v.to_le_bytes());
    row[0x1c..0x20].copy_from_slice(&point.lmap_coord[0].to_le_bytes());
    row[0x20..0x24].copy_from_slice(&point.lmap_coord[1].to_le_bytes());
    row[0x24..0x28].copy_from_slice(&pack_unit_vec(point.normal).to_le_bytes());
    row[0x28..0x2c].copy_from_slice(&pack_unit_vec(tex_coord_axis).to_le_bytes());
    row
}

pub fn fx_pack_mark_model_vertex(
    point: &crate::FxMarkStagingPoint,
    origin: [f32; 3],
    radius: f32,
    tex_coord_axis: [f32; 3],
    native_color: u32,
) -> [u8; GFX_MARK_MESH_VERTEX_STRIDE] {
    let world = fx_pack_mark_world_vertex(point, origin, radius, tex_coord_axis, native_color);
    let mut row = [0; GFX_MARK_MESH_VERTEX_STRIDE];
    row[..0x14].copy_from_slice(&world[..0x14]);
    let pack = |bits: u32| {
        let mut q = (bits.wrapping_mul(2) ^ 0x8000_3fff) as i32 >> 14;
        if q >= 0x3fff {
            q = 0x3fff;
        } else if q < -0x3fff {
            q = 0;
        }
        (q as u32 & 0x3fff) | (bits >> 16 & 0xc000)
    };
    let u = pack(u32::from_le_bytes(world[0x14..0x18].try_into().unwrap()));
    let v = pack(u32::from_le_bytes(world[0x18..0x1c].try_into().unwrap()));
    row[0x14..0x18].copy_from_slice(&(u << 16 | v).to_le_bytes());
    row[0x18..0x20].copy_from_slice(&world[0x24..0x2c]);
    row
}

#[inline]
fn vec3_dot(a: [f32; 3], b: [f32; 3]) -> f32 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

#[inline]
fn vec3_cross(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}

#[inline]
fn pack_unit_vec(n: [f32; 3]) -> u32 {
    let component = |v: f32| ((v * 127.0 + 127.5) as i32).clamp(0, 255) as u8;
    u32::from_le_bytes([component(n[0]), component(n[1]), component(n[2]), 63])
}

pub fn fx_generate_mark_verts_begin(
    budget: &mut GfxMarkMeshBudget,
    point_count: i16,
    tri_count: u8,
) -> Result<(u16, u32), GfxMarkMeshRefuse> {
    let points = if point_count < 0 {
        0
    } else {
        point_count as u16
    };
    let base_vert = r_reserve_mark_mesh_verts(budget, points)?;
    let reserve = fx_mark_mesh_index_reserve(budget.index_n, tri_count);
    let base_index = r_reserve_mark_mesh_indices(budget, reserve)?;
    Ok((base_vert, base_index))
}
