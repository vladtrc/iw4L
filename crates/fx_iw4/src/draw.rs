#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum FxElemType {
    Billboard = 0,
    Oriented = 1,
    Tail = 2,
    Trail = 3,
    Cloud = 4,
    SparkCloud = 5,
    SparkFountain = 6,
    Model = 7,
    OmniLight = 8,
    SpotLight = 9,
    Sound = 10,
    Decal = 11,
    Runner = 12,
}

impl FxElemType {
    pub const DRAW_TABLE_COUNT: usize = 11;

    pub const COUNT: usize = 13;

    #[inline]
    pub const fn from_u8(v: u8) -> Option<Self> {
        match v {
            0 => Some(Self::Billboard),
            1 => Some(Self::Oriented),
            2 => Some(Self::Tail),
            3 => Some(Self::Trail),
            4 => Some(Self::Cloud),
            5 => Some(Self::SparkCloud),
            6 => Some(Self::SparkFountain),
            7 => Some(Self::Model),
            8 => Some(Self::OmniLight),
            9 => Some(Self::SpotLight),
            10 => Some(Self::Sound),
            11 => Some(Self::Decal),
            12 => Some(Self::Runner),
            _ => None,
        }
    }

    #[inline]
    pub const fn draw_elem_handler_is_null(self) -> bool {
        matches!(self, Self::Trail | Self::Sound | Self::Decal | Self::Runner)
    }

    #[inline]
    pub const fn spawn_elem_early_out(self) -> bool {
        matches!(self, Self::Sound | Self::Decal | Self::Runner)
    }
}

pub const FX_DRAW_ELEM_HANDLER_PRESENT: [bool; FxElemType::DRAW_TABLE_COUNT] = [
    true, true, true, false, true, true, true, true, true, true, false,
];

#[inline]
pub const fn fx_draw_elem_handler_present(elem_type: u8) -> bool {
    if (elem_type as usize) >= FX_DRAW_ELEM_HANDLER_PRESENT.len() {
        return false;
    }
    FX_DRAW_ELEM_HANDLER_PRESENT[elem_type as usize]
}
