#[inline]
pub fn surface_type_from_flags(surface_flags: u32) -> u8 {
    ((surface_flags >> 20) & 0x1f) as u8
}
