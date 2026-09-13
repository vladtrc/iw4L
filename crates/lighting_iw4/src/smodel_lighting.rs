use crate::entry::GfxLightGridEntry;

pub const GFX_STATIC_MODEL_INST_SIZE: usize = 0x24;

pub const GFX_STATIC_MODEL_DRAW_INST_SIZE: usize = 0x4c;

pub const GFX_STATIC_MODEL_INST_LIGHTING_ORIGIN: usize = 0x18;

pub const GFX_STATIC_MODEL_DRAW_INST_LIGHTING_HANDLE: usize = 0x3a;

pub const GFX_STATIC_MODEL_DRAW_INST_REFLECTION_PROBE_INDEX: usize = 0x3c;

pub const GFX_STATIC_MODEL_DRAW_INST_PRIMARY_LIGHT_INDEX: usize = 0x3d;

pub const GFX_STATIC_MODEL_DRAW_INST_SAMPLE_SELECTOR: usize =
    GFX_STATIC_MODEL_DRAW_INST_PRIMARY_LIGHT_INDEX;

pub const GFX_STATIC_MODEL_DRAW_INST_FLAGS: usize = 0x3e;

pub const STATIC_MODEL_FLAG_NO_CAST_SHADOW: u8 = 0x10;

pub const GFX_STATIC_MODEL_DRAW_INST_PACKED_LIGHTING: usize = 0x40;

pub const GFX_STATIC_MODEL_DRAW_INST_CACHE_INDEX: usize = 0x44;

pub const SMC_CACHE_INDEX_LODS: usize = 4;

pub const GFX_MODEL_LIGHTING_PATCH_SIZE: usize = 0x34;

pub const GFX_MODEL_LIGHTING_PATCH_LIST_CAP: u32 = 0x1000;

pub const GFX_MODEL_LIGHTING_PATCH_COLORS_SLOTS: usize = 8;

pub const SMODEL_LIGHTING_RESERVED_ENTRY0_AFTER_WALK: u32 = 1;

pub const SMODEL_LIGHTING_FREEABLE_AGE_FRAMES: u32 = 4;

pub const SMODEL_LIGHTING_REUSE_MARKS_DIRTY: bool = false;

pub const SMODEL_LIGHTING_DEFER_FLAG: u8 = 0x20;

#[inline]
pub const fn smodel_lighting_bits_mask(smodel_index: u32) -> (usize, u32) {
    let word = (smodel_index >> 5) as usize;
    let mask = 0x8000_0000u32 >> (smodel_index & 31);
    (word, mask)
}

#[inline]
pub const fn smodel_lighting_msb_local_bit(word: u32) -> Option<u32> {
    if word == 0 {
        return None;
    }
    let high = 31u32 - word.leading_zeros();
    Some(high ^ 0x1f)
}

#[inline]
pub const fn model_lighting_handle_from_entry(entry: u16) -> u16 {
    entry.wrapping_add(1)
}

pub const SMODEL_LIGHTING_WARN_TOO_MUCH: u32 = 0xd;

pub const MODEL_LIGHTING_WARN_CACHE_ALLOC_FAILED: u32 = 4;

pub const GFX_MODELLIGHT_EXTRAPOLATE: u32 = 0;

pub const GFX_MODELLIGHT_SHOW_MISSING: u32 = 1;

#[inline]
pub const fn draw_inst_defers_lighting(flags: u8) -> bool {
    (flags & SMODEL_LIGHTING_DEFER_FLAG) != 0
}

#[inline]
pub const fn draw_inst_lighting_handle_entry(raw: u16) -> Option<u16> {
    if raw == 0 {
        None
    } else {
        Some(raw.wrapping_sub(1))
    }
}

#[inline]
pub fn lighting_origin_from_inst_bytes(inst: &[u8]) -> Option<[f32; 3]> {
    if inst.len() < GFX_STATIC_MODEL_INST_SIZE {
        return None;
    }
    let o = GFX_STATIC_MODEL_INST_LIGHTING_ORIGIN;
    let x = f32::from_le_bytes(inst[o..o + 4].try_into().ok()?);
    let y = f32::from_le_bytes(inst[o + 4..o + 8].try_into().ok()?);
    let z = f32::from_le_bytes(inst[o + 8..o + 12].try_into().ok()?);
    Some([x, y, z])
}

#[inline]
pub const fn light_grid_entry_primary_light(entry: &GfxLightGridEntry) -> u8 {
    entry.primary_light_index
}

#[inline]
pub const fn light_grid_entry_needs_trace_bits(entry: &GfxLightGridEntry) -> u8 {
    entry.needs_trace
}
