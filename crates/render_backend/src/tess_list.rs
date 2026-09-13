pub use render_frame::{
    GfxSmodelRigidEntry, GfxTrianglesListEntry, GfxXModelRigidEntry, SMODEL_RIGID_ENTRY_STRIDE,
    TRIANGLES_LIST_ENTRY_STRIDE, XMODEL_RIGID_ENTRY_STRIDE, smodel_rigid_index_run_continues,
    triangles_list_run_continues, xmodel_rigid_index_run_continues, xmodel_tess_info_packed_arm,
    xmodel_tess_info_vert_decl_type,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SmodelRigidFlush {
    pub index_byte_offset: u32,
    pub tri_count: u32,

    pub entry_start: u32,
    pub entry_count: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SmodelRigidListStep {
    pub cur: usize,

    pub more: bool,

    pub consumed: bool,
}

pub fn r_tess_static_model_rigid_draw_surf_list(
    entries: &[GfxSmodelRigidEntry],
    cur: usize,
    setup_ok: bool,
) -> SmodelRigidListStep {
    let end = entries.len();
    debug_assert!(cur < end);
    let key = entries[cur].packed_key;
    let mut next = cur + 1;
    while next < end && entries[next].packed_key == key {
        next += 1;
    }
    SmodelRigidListStep {
        cur: next,
        more: next != end,
        consumed: setup_ok,
    }
}

pub fn r_tess_static_model_rigid_draw_surf_lighting(
    run: &[GfxSmodelRigidEntry],
    tech_type_lit: bool,
) -> Vec<SmodelRigidFlush> {
    let mut flushes = Vec::new();
    if run.is_empty() {
        return flushes;
    }
    let mut accum_byte = 0;
    let mut accum_tri = 0u32;
    let mut entry_start = 0usize;
    let mut lighting = 0u16;
    for (entry_index, entry) in run.iter().enumerate() {
        if tech_type_lit && entry.lighting_handle != lighting {
            if accum_tri != 0 {
                flushes.push(SmodelRigidFlush {
                    index_byte_offset: accum_byte,
                    tri_count: accum_tri,
                    entry_start: entry_start as u32,
                    entry_count: (entry_index - entry_start) as u32,
                });
                accum_tri = 0;
                entry_start = entry_index;
            }
            lighting = entry.lighting_handle;
        }
        if !smodel_rigid_index_run_continues(accum_byte, accum_tri, entry) {
            if accum_tri != 0 {
                flushes.push(SmodelRigidFlush {
                    index_byte_offset: accum_byte,
                    tri_count: accum_tri,
                    entry_start: entry_start as u32,
                    entry_count: (entry_index - entry_start) as u32,
                });
                entry_start = entry_index;
            }
            accum_byte = entry.index_byte_offset;
            accum_tri = 0;
        }
        accum_tri += u32::from(entry.tri_count);
    }
    if accum_tri != 0 {
        flushes.push(SmodelRigidFlush {
            index_byte_offset: accum_byte,
            tri_count: accum_tri,
            entry_start: entry_start as u32,
            entry_count: (run.len() - entry_start) as u32,
        });
    }
    flushes
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct XModelRigidFlush {
    pub index_byte_offset: u32,
    pub tri_count: u32,

    pub entry_start: u32,
    pub entry_count: u32,
}

pub fn smodel_rigid_list_step(
    entries: &[GfxXModelRigidEntry],
    cur: usize,
    setup_ok: bool,
) -> SmodelRigidListStep {
    let end = entries.len();
    debug_assert!(cur < end);
    let key = entries[cur].packed_key;
    let mut next = cur + 1;
    while next < end && entries[next].packed_key == key {
        next += 1;
    }
    SmodelRigidListStep {
        cur: next,
        more: next != end,
        consumed: setup_ok,
    }
}

pub fn r_tess_xmodel_rigid_draw_surf_lighting(
    run: &[GfxXModelRigidEntry],
    tech_type_lit: bool,
) -> Vec<XModelRigidFlush> {
    let mut flushes = Vec::new();
    if run.is_empty() {
        return flushes;
    }
    let mut accum_byte = 0;
    let mut accum_tri = 0u32;
    let mut entry_start = 0usize;
    let mut lighting = 0u32;
    for (entry_index, entry) in run.iter().enumerate() {
        if tech_type_lit && entry.lighting_handle != lighting {
            if accum_tri != 0 {
                flushes.push(XModelRigidFlush {
                    index_byte_offset: accum_byte,
                    tri_count: accum_tri,
                    entry_start: entry_start as u32,
                    entry_count: (entry_index - entry_start) as u32,
                });
                accum_tri = 0;
                entry_start = entry_index;
            }
            lighting = entry.lighting_handle;
        }
        if !xmodel_rigid_index_run_continues(accum_byte, accum_tri, entry) {
            if accum_tri != 0 {
                flushes.push(XModelRigidFlush {
                    index_byte_offset: accum_byte,
                    tri_count: accum_tri,
                    entry_start: entry_start as u32,
                    entry_count: (entry_index - entry_start) as u32,
                });
                entry_start = entry_index;
            }
            accum_byte = entry.index_byte_offset;
            accum_tri = 0;
        }
        accum_tri += u32::from(entry.tri_count);
    }
    if accum_tri != 0 {
        flushes.push(XModelRigidFlush {
            index_byte_offset: accum_byte,
            tri_count: accum_tri,
            entry_start: entry_start as u32,
            entry_count: (run.len() - entry_start) as u32,
        });
    }
    flushes
}

pub const XMODEL_TESS_LIGHTMAP_CODE_TEXTURE: u8 = 0x62;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct XmodelTessLightmapBinds {
    pub primary: bool,
    pub secondary: bool,
}

#[must_use]
pub fn xmodel_tess_lightmap_binds(custom_sampler_flags: u8) -> XmodelTessLightmapBinds {
    XmodelTessLightmapBinds {
        primary: custom_sampler_flags & 2 != 0,
        secondary: custom_sampler_flags & 4 != 0,
    }
}

pub const XMODEL_TESS_TEXTURE_CACHE_BASE: u32 = 0x50;

pub const XMODEL_TESS_SET_TEXTURE_VTBL: u32 = 0x104;

#[must_use]
pub fn xmodel_tess_texture_cache_dirty(cached: u32, image: u32) -> bool {
    cached != image
}

#[must_use]
pub fn xmodel_tess_texture_cache_off(stage: u32) -> u32 {
    XMODEL_TESS_TEXTURE_CACHE_BASE.saturating_add(stage.saturating_mul(4))
}

pub const XMODEL_TESS_SAMPLER_PACKED_BASE: u32 = 0x10;

pub const XMODEL_TESS_SET_SAMPLER_VTBL: u32 = 0x114;

pub const XMODEL_TESS_SAMPLER_DEVICE_XOR: u32 = 0xf00 | 0xf000 | 0xf0000 | 0x3f0_0000 | 0x400_0000;

pub const XMODEL_TESS_SAMPLER_IMAGE7_SHIFT: u32 = 26;

#[must_use]
pub fn xmodel_tess_sampler_packed_word(sampler_flags: u8, lut_low5: u32, image_byte7: u8) -> u32 {
    let f = u32::from(sampler_flags);
    let mut packed = (f & 0x80) | 0x54;
    packed = packed * 2 | (f & 0x40);
    packed = packed * 2 | (f & 0x20);
    (packed << 16) | lut_low5 | (u32::from(image_byte7) << XMODEL_TESS_SAMPLER_IMAGE7_SHIFT)
}

#[must_use]
pub fn xmodel_tess_sampler_packed_off(stage: u32) -> u32 {
    XMODEL_TESS_SAMPLER_PACKED_BASE.saturating_add(stage.saturating_mul(4))
}

#[must_use]
pub fn xmodel_tess_sampler_pack_dirty(cached: u32, desired: u32) -> bool {
    cached != desired
}

#[must_use]
pub fn xmodel_tess_sampler_xor_needs_device(current: u32, desired: u32) -> bool {
    let xor = current ^ desired;
    if xor & XMODEL_TESS_SAMPLER_DEVICE_XOR != 0 {
        return true;
    }
    (xor & 0xff) != 0 && (current & 0xff) >= 2
}

#[must_use]
pub fn xmodel_tess_sampler_merge_low_byte(current: u32, desired: u32) -> u32 {
    if (current ^ desired) & 0xff == 0 || current & 0xff >= 2 {
        current
    } else {
        (current & !0xff) | (desired & 0xff)
    }
}

pub const WORLD_STREAM0_STRIDE: u32 = 0x2c;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TrianglesListFlush {
    pub vertex_count: u32,
    pub base_index: u32,
    pub tri_count: u32,

    pub rebind_streams: bool,
    pub first_vertex: u32,

    pub entry_start: u32,
    pub entry_count: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TrianglesListArm {
    Prepass { n: u8 },

    ShadowmapBuild,

    Colour,
}

impl TrianglesListArm {
    pub const fn for_tech_type(tech_type: u8, n: u8) -> Self {
        if tech_type < 2 {
            Self::Prepass { n }
        } else if tech_type < 4 {
            Self::ShadowmapBuild
        } else {
            Self::Colour
        }
    }

    pub const fn flag_mask(self) -> u8 {
        match self {
            Self::Prepass { n } => {
                if n == 0 {
                    0x4
                } else {
                    0x8
                }
            }
            Self::ShadowmapBuild => 0x10,
            Self::Colour => 0,
        }
    }
}

pub fn r_tess_triangles_list_generic(
    entries: &[GfxTrianglesListEntry],
    mut sort_key_run_ends: impl FnMut(u32, u32) -> bool,
) -> Vec<TrianglesListFlush> {
    let mut flushes = Vec::new();
    let mut iter = entries.iter();
    let Some(first) = iter.next() else {
        return flushes;
    };

    let mut base_index = first.base_index;
    let mut tri_count = u32::from(first.tri_count);
    let mut first_vertex = first.first_vertex;
    let mut vertex_count = first.vertex_count;
    let mut sort_key = first.sort_key;
    let mut prev_first_vertex: Option<u32> = None;
    let mut entry_start = 0u32;
    let mut entry_count = 1u32;

    for (entry_i, next) in iter.enumerate() {
        let continues = triangles_list_run_continues(base_index, tri_count, first_vertex, next)
            && !(next.sort_key != sort_key && sort_key_run_ends(sort_key, next.sort_key));
        if continues {
            tri_count += u32::from(next.tri_count);
            vertex_count = vertex_count.max(next.vertex_count);
            sort_key = next.sort_key;
            entry_count += 1;
            continue;
        }
        flushes.push(TrianglesListFlush {
            vertex_count,
            base_index,
            tri_count,
            rebind_streams: prev_first_vertex != Some(first_vertex),
            first_vertex,
            entry_start,
            entry_count,
        });
        prev_first_vertex = Some(first_vertex);
        base_index = next.base_index;
        tri_count = u32::from(next.tri_count);
        first_vertex = next.first_vertex;
        vertex_count = next.vertex_count;
        sort_key = next.sort_key;

        entry_start = entry_i as u32 + 1;
        entry_count = 1;
    }

    flushes.push(TrianglesListFlush {
        vertex_count,
        base_index,
        tri_count,
        rebind_streams: prev_first_vertex != Some(first_vertex),
        first_vertex,
        entry_start,
        entry_count,
    });
    flushes
}
