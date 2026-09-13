#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum TextureFilter {
    None,

    Point,

    Linear,

    Anisotropic,

    Unknown(u32),
}

impl TextureFilter {
    pub const fn from_raw(raw: u32) -> Self {
        match raw {
            0 => Self::None,
            1 => Self::Point,
            2 => Self::Linear,
            3 => Self::Anisotropic,
            value => Self::Unknown(value),
        }
    }

    pub const fn to_raw(self) -> Option<u32> {
        match self {
            Self::None => Some(0),
            Self::Point => Some(1),
            Self::Linear => Some(2),
            Self::Anisotropic => Some(3),
            Self::Unknown(_) => None,
        }
    }

    pub const fn is_min_mag(self) -> bool {
        matches!(self, Self::Point | Self::Linear | Self::Anisotropic)
    }
}
