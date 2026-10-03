#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ZoneGame {
    #[default]
    Iw4,

    T5,

    Iw5,

    T6,
}

impl ZoneGame {
    pub fn prefix(self) -> &'static str {
        match self {
            Self::Iw4 => "iw4",
            Self::Iw5 => "iw5",
            Self::T5 => "t5",
            Self::T6 => "t6",
        }
    }

    pub fn from_prefix(prefix: &str) -> Option<Self> {
        match prefix {
            "iw4" => Some(Self::Iw4),
            "iw5" => Some(Self::Iw5),
            "t5" => Some(Self::T5),
            "t6" => Some(Self::T6),
            _ => None,
        }
    }
}
