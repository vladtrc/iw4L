//! Portal levels occupy the upper two stencil bits. Mark and unmark invert
//! exactly the bits that differ between adjacent levels: incrementing an
//! 8-bit stencil value under a 0xc0 write mask cannot advance these levels.

pub const PORTAL_LEVEL_MASK: u32 = 0xc0;
pub const MATERIAL_STENCIL_MASK: u32 = 0x3f;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PortalStencilTransition {
    /// Compare EQUAL against this reference with PORTAL_LEVEL_MASK.
    pub reference: u32,
    /// Use INVERT on stencil/depth pass with this write mask.
    pub invert_mask: u32,
}

impl PortalStencilTransition {
    pub fn between(from: u8, to: u8) -> Option<Self> {
        if from > 3 || to > 3 || from.abs_diff(to) != 1 {
            return None;
        }
        Some(Self {
            reference: u32::from(from) << 6,
            invert_mask: u32::from(from ^ to) << 6,
        })
    }
}
