#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum BlendFactor {
    Zero,

    One,

    SrcColor,

    InvSrcColor,

    SrcAlpha,

    InvSrcAlpha,

    DestAlpha,

    InvDestAlpha,

    DestColor,

    InvDestColor,

    SrcAlphaSat,

    Unknown(u32),
}

impl BlendFactor {
    pub const fn from_raw(raw: u32) -> Self {
        match raw {
            1 => Self::Zero,
            2 => Self::One,
            3 => Self::SrcColor,
            4 => Self::InvSrcColor,
            5 => Self::SrcAlpha,
            6 => Self::InvSrcAlpha,
            7 => Self::DestAlpha,
            8 => Self::InvDestAlpha,
            9 => Self::DestColor,
            10 => Self::InvDestColor,
            11 => Self::SrcAlphaSat,
            value => Self::Unknown(value),
        }
    }

    pub const fn to_raw(self) -> Option<u32> {
        match self {
            Self::Zero => Some(1),
            Self::One => Some(2),
            Self::SrcColor => Some(3),
            Self::InvSrcColor => Some(4),
            Self::SrcAlpha => Some(5),
            Self::InvSrcAlpha => Some(6),
            Self::DestAlpha => Some(7),
            Self::InvDestAlpha => Some(8),
            Self::DestColor => Some(9),
            Self::InvDestColor => Some(10),
            Self::SrcAlphaSat => Some(11),
            Self::Unknown(_) => None,
        }
    }

    pub const fn is_known(self) -> bool {
        !matches!(self, Self::Unknown(_))
    }
}
