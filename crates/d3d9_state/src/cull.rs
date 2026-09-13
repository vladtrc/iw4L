#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum CullMode {
    None,

    Cw,

    Ccw,

    Unknown(u32),
}

pub const D3DCULL_NONE: u32 = 1;

pub const D3DCULL_CW: u32 = 2;

pub const D3DCULL_CCW: u32 = 3;

impl CullMode {
    pub const fn from_raw(raw: u32) -> Self {
        match raw {
            1 => Self::None,
            2 => Self::Cw,
            3 => Self::Ccw,
            value => Self::Unknown(value),
        }
    }

    pub const fn to_raw(self) -> Option<u32> {
        match self {
            Self::None => Some(1),
            Self::Cw => Some(2),
            Self::Ccw => Some(3),
            Self::Unknown(_) => None,
        }
    }
}
