pub const FX_STATUS_REF_COUNT_MASK_IW4: u32 = 0x1fff;

pub const FX_STATUS_REF_COUNT_MASK: u32 = 0xffff;

pub const FX_STATUS_HAS_PENDING_LOOP_ELEMS: u32 = 0x8000;

pub const FX_STATUS_OWNED_EFFECTS_MASK: u32 = 0x03ff_0000;

pub const FX_STATUS_DEFER_UPDATE: u32 = 0x0800_0000;

pub const FX_STATUS_IS_LOCKED: u32 = 0x2000_0000;

pub const FX_STATUS_IS_LOCKED_MASK: u32 = 0x6000_0000;

#[inline]
pub const fn fx_status_is_unique_done(status: u32) -> bool {
    (status & FX_STATUS_REF_COUNT_MASK_IW4) == 1
}
