#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum Team {
    #[default]
    Free = 0,
    Axis = 1,
    Allies = 2,
}

impl Team {
    pub fn from_retail_u8(raw: u8) -> Option<Self> {
        match raw {
            0 => Some(Self::Free),
            1 => Some(Self::Axis),
            2 => Some(Self::Allies),
            _ => None,
        }
    }

    pub fn opposing(self) -> Self {
        match self {
            Self::Allies => Self::Axis,
            Self::Axis => Self::Allies,
            Self::Free => Self::Free,
        }
    }
}
