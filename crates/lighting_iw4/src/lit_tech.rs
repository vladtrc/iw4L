pub const GFX_DRAW_METHOD_LIT_BEGIN: u8 = 9;

pub const TECHNIQUE_NONE: u8 = 0x32;

pub const TECHNIQUE_LIGHT_SPOT: u8 = 37;
pub const TECHNIQUE_LIGHT_OMNI: u8 = 38;
pub const TECHNIQUE_LIGHT_SPOT_SHADOW: u8 = 39;

pub const fn additional_light_tech_type(gfx_light_type: u8, has_shadow_map: bool) -> u8 {
    match gfx_light_type {
        crate::GFX_LIGHT_TYPE_SPOT if has_shadow_map => TECHNIQUE_LIGHT_SPOT_SHADOW,
        crate::GFX_LIGHT_TYPE_SPOT => TECHNIQUE_LIGHT_SPOT,
        crate::GFX_LIGHT_TYPE_OMNI => TECHNIQUE_LIGHT_OMNI,
        _ => TECHNIQUE_NONE,
    }
}

pub const LIT_TECH_COL_COUNT: u8 = 7;

pub const LIT_TECH_SURF_ROWS: u8 = 16;

pub const LIT_TECH_SHADOW_COLUMN_BIAS: u8 = 3;

pub const LIT_TECH_INSTANCED_SURF_TYPES: [u8; 2] = [3, 4];

pub const LIT_TECH_STANDARD_ROW: [u8; 7] = [9, 0x0b, 0x0f, 0x13, 0x0d, 0x11, 0x15];

pub const LIT_TECH_STANDARD_ROW_DFOG: [u8; 7] = [10, 0x0c, 0x10, 0x14, 0x0e, 0x12, 0x16];

pub const LIT_TECH_INSTANCED_ROW: [u8; 7] = [0x17, 0x19, 0x1d, 0x21, 0x1b, 0x1f, 0x23];

pub const LIT_TECH_INSTANCED_ROW_DFOG: [u8; 7] = [0x18, 0x1a, 0x1e, 0x22, 0x1c, 0x20, 0x24];

pub const fn lit_tech_column(gfx_light_type: u8, has_shadow_map: bool) -> Option<u8> {
    let biased = if has_shadow_map {
        match gfx_light_type.checked_add(LIT_TECH_SHADOW_COLUMN_BIAS) {
            Some(v) => v,
            None => return None,
        }
    } else {
        gfx_light_type
    };
    if biased < LIT_TECH_COL_COUNT {
        Some(biased)
    } else {
        None
    }
}

const fn standard_surfs_contain(surf_type: u8) -> bool {
    matches!(surf_type, 0 | 1 | 2 | 5 | 6 | 7 | 8 | 9 | 10 | 11 | 12)
}

pub fn lit_tech_type(
    base: u8,
    surf_type: u8,
    gfx_light_type: u8,
    dfog: bool,
    has_shadow_map: bool,
) -> u8 {
    if base != GFX_DRAW_METHOD_LIT_BEGIN {
        return base;
    }
    let Some(col) = lit_tech_column(gfx_light_type, has_shadow_map) else {
        return TECHNIQUE_NONE;
    };
    let row: [u8; 7] = if LIT_TECH_INSTANCED_SURF_TYPES.contains(&surf_type) {
        if dfog {
            LIT_TECH_INSTANCED_ROW_DFOG
        } else {
            LIT_TECH_INSTANCED_ROW
        }
    } else if standard_surfs_contain(surf_type) {
        if dfog {
            LIT_TECH_STANDARD_ROW_DFOG
        } else {
            LIT_TECH_STANDARD_ROW
        }
    } else {
        return TECHNIQUE_NONE;
    };
    row[col as usize]
}

pub const LIT_TECH_NO_SHADOW_DIR_SLOTS: [u8; 8] = [9, 10, 0x0b, 0x0c, 0x17, 0x18, 0x19, 0x1a];

pub const LIT_TECH_NO_SHADOW_LOCAL_SLOTS: [u8; 8] =
    [0x0f, 0x10, 0x13, 0x14, 0x1d, 0x1e, 0x21, 0x22];

pub const LIT_TECH_SHADOW_DIR_SLOTS: [u8; 4] = [0x0d, 0x0e, 0x1b, 0x1c];

pub const LIT_TECH_SHADOW_SPOT_SLOTS: [u8; 4] = [0x11, 0x12, 0x1f, 0x20];

pub const fn is_lit_remap_slot(tech: u8) -> bool {
    contains_u8(&LIT_TECH_STANDARD_ROW, tech)
        || contains_u8(&LIT_TECH_STANDARD_ROW_DFOG, tech)
        || contains_u8(&LIT_TECH_INSTANCED_ROW, tech)
        || contains_u8(&LIT_TECH_INSTANCED_ROW_DFOG, tech)
}

const fn contains_u8(row: &[u8; 7], tech: u8) -> bool {
    let mut i = 0;
    while i < 7 {
        if row[i] == tech {
            return true;
        }
        i += 1;
    }
    false
}
