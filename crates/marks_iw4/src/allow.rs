pub const R_ALLOW_MARKS_GAME_FLAGS_MASK: u8 = 0x5;

pub const R_ALLOW_MARKS_STATE_FLAGS_MASK: u8 = 0x4;

pub const GFX_SURFACE_MATERIAL_OFF: usize = 0x10;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FxMarkAllow {
    Keep,
    Reject,

    Unknown,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct MarkWorldAllowCensus {
    pub decal_list_n: u32,

    pub sphere_hit: u32,
    pub keep: u32,
    pub reject: u32,
    pub unknown: u32,
}

#[inline]
pub fn fx_mark_material_allows_marks(
    receiver_game_flags: u8,
    receiver_surface_type_bits: u32,
    mark_surface_type_bits: u32,
) -> bool {
    if receiver_game_flags & R_ALLOW_MARKS_GAME_FLAGS_MASK != 0 {
        return false;
    }
    (receiver_surface_type_bits & mark_surface_type_bits) == mark_surface_type_bits
}

pub fn fx_mark_allow(
    receiver_game_flags: Option<u8>,
    receiver_surface_type_bits: Option<u32>,
    mark_surface_type_bits: Option<u32>,
) -> FxMarkAllow {
    let Some(flags) = receiver_game_flags else {
        return FxMarkAllow::Unknown;
    };
    if flags & R_ALLOW_MARKS_GAME_FLAGS_MASK != 0 {
        return FxMarkAllow::Reject;
    }
    let Some(recv) = receiver_surface_type_bits else {
        return FxMarkAllow::Unknown;
    };
    let Some(mark) = mark_surface_type_bits else {
        return FxMarkAllow::Unknown;
    };
    if fx_mark_material_allows_marks(flags, recv, mark) {
        FxMarkAllow::Keep
    } else {
        FxMarkAllow::Reject
    }
}

#[inline]
pub fn fx_mark_include_in_world_clip(allow: FxMarkAllow) -> bool {
    matches!(allow, FxMarkAllow::Keep)
}
