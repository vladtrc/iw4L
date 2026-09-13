use crate::cmp::CompareFunc;

pub const D3DRS_ALPHATESTENABLE: u32 = 0xf;

pub const D3DRS_ALPHAREF: u32 = 0x18;

pub const D3DRS_ALPHAFUNC: u32 = 0x19;

pub const ALPHA_REF_SCALE: f32 = 255.0;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct AlphaTest {
    pub func: CompareFunc,

    pub reference: u8,
}

impl AlphaTest {
    pub const fn from_raw(func: u32, reference: u8) -> Self {
        Self {
            func: CompareFunc::from_raw(func),
            reference,
        }
    }

    pub fn normalised_reference(self) -> f32 {
        f32::from(self.reference) / ALPHA_REF_SCALE
    }

    pub const fn passes(self, alpha8: u8) -> Option<bool> {
        let (a, b) = (alpha8, self.reference);
        Some(match self.func {
            CompareFunc::Never => false,
            CompareFunc::Less => a < b,
            CompareFunc::Equal => a == b,
            CompareFunc::LessEqual => a <= b,
            CompareFunc::Greater => a > b,
            CompareFunc::NotEqual => a != b,
            CompareFunc::GreaterEqual => a >= b,
            CompareFunc::Always => true,
            CompareFunc::Unknown(_) => return None,
        })
    }
}
