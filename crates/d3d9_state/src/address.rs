#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum AddressMode {
    Wrap,

    Mirror,

    Clamp,

    Border,

    MirrorOnce,

    Unknown(u32),
}

impl AddressMode {
    pub const fn from_raw(raw: u32) -> Self {
        match raw {
            1 => Self::Wrap,
            2 => Self::Mirror,
            3 => Self::Clamp,
            4 => Self::Border,
            5 => Self::MirrorOnce,
            value => Self::Unknown(value),
        }
    }

    pub const fn from_clamp_flag(clamp: bool) -> Self {
        if clamp { Self::Clamp } else { Self::Wrap }
    }

    pub const fn to_raw(self) -> Option<u32> {
        match self {
            Self::Wrap => Some(1),
            Self::Mirror => Some(2),
            Self::Clamp => Some(3),
            Self::Border => Some(4),
            Self::MirrorOnce => Some(5),
            Self::Unknown(_) => None,
        }
    }
}
