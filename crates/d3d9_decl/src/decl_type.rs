#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum DeclType {
    Float2,

    Float3,

    Float4,

    D3dColor,

    UByte4,

    UByte4N,

    Unknown(u8),
}

impl DeclType {
    pub const fn from_raw(raw: u8) -> Self {
        match raw {
            1 => Self::Float2,
            2 => Self::Float3,
            3 => Self::Float4,
            4 => Self::D3dColor,
            5 => Self::UByte4,
            8 => Self::UByte4N,
            value => Self::Unknown(value),
        }
    }

    pub const fn byte_len(self) -> Option<u8> {
        match self {
            Self::Float2 => Some(8),
            Self::Float3 => Some(12),
            Self::Float4 => Some(16),
            Self::D3dColor | Self::UByte4 | Self::UByte4N => Some(4),
            Self::Unknown(_) => None,
        }
    }

    pub const fn to_raw(self) -> Option<u8> {
        match self {
            Self::Float2 => Some(1),
            Self::Float3 => Some(2),
            Self::Float4 => Some(3),
            Self::D3dColor => Some(4),
            Self::UByte4 => Some(5),
            Self::UByte4N => Some(8),
            Self::Unknown(_) => None,
        }
    }
}
