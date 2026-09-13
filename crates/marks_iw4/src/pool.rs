pub const FX_MARK_STRIDE: usize = 0x40;

pub const FX_MARKS_LIMIT: u32 = 512;

pub const FX_TRI_GROUP_STRIDE: usize = 0x18;

pub const FX_TRI_GROUP_LIMIT: u32 = 2048;

pub const FX_POINT_GROUP_STRIDE: usize = 0x44;

pub const FX_POINT_GROUP_LIMIT: u32 = 3072;

pub const FX_MARKS_CLIENT_STRIDE: usize = 0x4_7000;

pub const FX_MARK_ENT_LIMIT: u8 = 0x10;

pub const FX_MARK_HANDLE_NONE: u16 = 0xffff;

pub const FX_TRI_GROUP_NEXT_NONE: u32 = 0;
pub const FX_POINT_GROUP_NEXT_NONE: u32 = 0;

#[inline]
pub const fn fx_init_mark_next_handle(slot: u32) -> u16 {
    if slot + 1 >= FX_MARKS_LIMIT {
        FX_MARK_HANDLE_NONE
    } else {
        fx_mark_handle_for_slot(slot + 1)
    }
}

#[inline]
pub const fn fx_mark_handle_from_byte_offset(byte_offset: u32) -> u16 {
    (byte_offset >> 6) as u16
}

#[inline]
pub const fn fx_mark_handle_for_slot(slot: u32) -> u16 {
    fx_mark_handle_from_byte_offset(slot.wrapping_mul(FX_MARK_STRIDE as u32))
}

#[inline]
pub const fn fx_init_tri_next_slot(slot: u32) -> u32 {
    if slot + 1 >= FX_TRI_GROUP_LIMIT {
        FX_TRI_GROUP_NEXT_NONE
    } else {
        slot + 1
    }
}

#[inline]
pub const fn fx_init_point_next_slot(slot: u32) -> u32 {
    if slot + 1 >= FX_POINT_GROUP_LIMIT {
        FX_POINT_GROUP_NEXT_NONE
    } else {
        slot + 1
    }
}
