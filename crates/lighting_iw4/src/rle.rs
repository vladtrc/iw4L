pub const LIGHT_GRID_RLE_BASE_STRIDE: usize = 3;

#[inline]
pub const fn light_grid_rle_run_stride(row_count: u16) -> usize {
    LIGHT_GRID_RLE_BASE_STRIDE + (row_count > 0xff) as usize
}

#[inline]
pub const fn model_lighting_patch_swizzle(packed: [u8; 4]) -> u32 {
    ((packed[3] as u32) << 24)
        | ((packed[0] as u32) << 16)
        | ((packed[1] as u32) << 8)
        | (packed[2] as u32)
}
