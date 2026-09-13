pub use d3d9_state::{
    D3DCMP_ALWAYS, D3DCMP_EQUAL, D3DCMP_LESS, D3DCMP_LESSEQUAL, D3DRS_ZENABLE, D3DRS_ZFUNC,
    D3DRS_ZWRITEENABLE,
};

pub const GFXS1_DEPTHWRITE: u32 = 0x1;

pub const GFXS1_DEPTHTEST_DISABLE: u32 = 0x2;

pub const GFXS1_DEPTHTEST_SHIFT: u32 = 2;

pub const GFXS1_DEPTHTEST_MASK: u32 = 0xc;

pub const GFXS1_DEPTHTEST_ALWAYS: u32 = 0x0;

pub const GFXS1_DEPTHTEST_LESS: u32 = 0x4;

pub const GFXS1_DEPTHTEST_EQUAL: u32 = 0x8;

pub const GFXS1_DEPTHTEST_LESSEQUAL: u32 = 0xc;

pub const S_DEPTH_TEST_TABLE: [u32; 4] =
    [D3DCMP_ALWAYS, D3DCMP_LESS, D3DCMP_EQUAL, D3DCMP_LESSEQUAL];

#[inline]
pub const fn depth_test_field(word1: u32) -> u32 {
    (word1 & GFXS1_DEPTHTEST_MASK) >> GFXS1_DEPTHTEST_SHIFT
}

#[inline]
pub const fn depth_write_enable(word1: u32) -> bool {
    (word1 & GFXS1_DEPTHWRITE) != 0
}

#[inline]
pub const fn depth_test_enable(word1: u32) -> bool {
    (word1 & GFXS1_DEPTHTEST_DISABLE) == 0
}

#[inline]
pub const fn d3d_zfunc_from_word1(word1: u32) -> u32 {
    S_DEPTH_TEST_TABLE[depth_test_field(word1) as usize]
}

#[inline]
pub const fn depth_state_from_state_bits(load_bits: [u32; 2]) -> (bool, bool, u32) {
    let w1 = load_bits[1];
    (
        depth_write_enable(w1),
        depth_test_enable(w1),
        d3d_zfunc_from_word1(w1),
    )
}
