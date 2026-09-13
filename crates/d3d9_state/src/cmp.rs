#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum CompareFunc {
    Never,

    Less,

    Equal,

    LessEqual,

    Greater,

    NotEqual,

    GreaterEqual,

    Always,

    Unknown(u32),
}

pub const D3DCMP_ALWAYS: u32 = 8;

pub const D3DCMP_LESS: u32 = 2;

pub const D3DCMP_EQUAL: u32 = 3;

pub const D3DCMP_LESSEQUAL: u32 = 4;

pub const D3DCMP_GREATER: u32 = 5;

pub const D3DCMP_GREATEREQUAL: u32 = 7;

impl CompareFunc {
    pub const fn from_raw(raw: u32) -> Self {
        match raw {
            1 => Self::Never,
            2 => Self::Less,
            3 => Self::Equal,
            4 => Self::LessEqual,
            5 => Self::Greater,
            6 => Self::NotEqual,
            7 => Self::GreaterEqual,
            8 => Self::Always,
            value => Self::Unknown(value),
        }
    }

    pub const fn to_raw(self) -> Option<u32> {
        match self {
            Self::Never => Some(1),
            Self::Less => Some(2),
            Self::Equal => Some(3),
            Self::LessEqual => Some(4),
            Self::Greater => Some(5),
            Self::NotEqual => Some(6),
            Self::GreaterEqual => Some(7),
            Self::Always => Some(8),
            Self::Unknown(_) => None,
        }
    }
}
