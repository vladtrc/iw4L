pub use d3d9_state::{
    AlphaTest, D3DCMP_GREATER, D3DCMP_GREATEREQUAL, D3DCMP_LESS, D3DCULL_CCW, D3DCULL_CW,
    D3DCULL_NONE, D3DRS_ALPHAFUNC, D3DRS_ALPHAREF, D3DRS_ALPHATESTENABLE, D3DRS_CULLMODE,
};

pub const GFXS0_CULL_SHIFT: u32 = 0xe;

pub const GFXS0_CULL_MASK: u32 = 0xc000;

pub const GFXS0_CULL_NONE: u32 = 0x4000;

pub const GFXS0_CULL_BACK: u32 = 0x8000;

pub const GFXS0_CULL_FRONT: u32 = 0xc000;

pub const S_CULL_TABLE: [u32; 4] = [0, D3DCULL_NONE, D3DCULL_CCW, D3DCULL_CW];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Gfxs0CullFace {
    None,
    Back,
    Front,
}

impl Gfxs0CullFace {
    pub const fn from_field(field: u32) -> Self {
        match field & 0b11 {
            2 => Self::Back,
            3 => Self::Front,

            _ => Self::None,
        }
    }

    pub const fn table_index(self) -> usize {
        match self {
            Self::None => 1,
            Self::Back => 2,
            Self::Front => 3,
        }
    }
}

pub const GFXS0_ATEST_DISABLE: u32 = 0x800;

pub const GFXS0_ATEST_SHIFT: u32 = 0xc;

pub const GFXS0_ATEST_MASK: u32 = 0x3000;

pub const GFXS0_ATEST_GT_0: u32 = 0x1000;

pub const GFXS0_ATEST_LT_128: u32 = 0x2000;

pub const GFXS0_ATEST_GE_128: u32 = 0x3000;

pub const S_ALPHA_TEST_REF_128: u8 = 0x80;

pub const S_ALPHA_TEST_TABLE: [(u32, u8); 3] = [
    (D3DCMP_GREATER, 0),
    (D3DCMP_LESS, S_ALPHA_TEST_REF_128),
    (D3DCMP_GREATEREQUAL, S_ALPHA_TEST_REF_128),
];

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Gfxs0AlphaTest {
    GreaterThanZero,

    LessThan128,

    GreaterEqual128,
}

impl Gfxs0AlphaTest {
    pub const fn from_field(field: u32) -> Option<Self> {
        match field & 0b11 {
            1 => Some(Self::GreaterThanZero),
            2 => Some(Self::LessThan128),
            3 => Some(Self::GreaterEqual128),
            _ => None,
        }
    }

    pub const fn table_index(self) -> usize {
        match self {
            Self::GreaterThanZero => 0,
            Self::LessThan128 => 1,
            Self::GreaterEqual128 => 2,
        }
    }

    pub const fn d3d(self) -> AlphaTest {
        let (func, reference) = S_ALPHA_TEST_TABLE[self.table_index()];
        AlphaTest::from_raw(func, reference)
    }
}

pub const fn alpha_test_from_state_bits(load_bits: [u32; 2]) -> Option<Gfxs0AlphaTest> {
    if load_bits[0] & GFXS0_ATEST_DISABLE != 0 {
        return None;
    }
    Gfxs0AlphaTest::from_field(load_bits[0] >> GFXS0_ATEST_SHIFT)
}

pub const fn cull_face_from_state_bits(load_bits: [u32; 2]) -> Gfxs0CullFace {
    Gfxs0CullFace::from_field(load_bits[0] >> GFXS0_CULL_SHIFT)
}

pub const fn d3d_cull_mode_from_state_bits(load_bits: [u32; 2]) -> u32 {
    S_CULL_TABLE[((load_bits[0] >> GFXS0_CULL_SHIFT) & 3) as usize]
}
