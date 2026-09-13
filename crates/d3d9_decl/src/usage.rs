pub const D3DDECLUSAGE_POSITION: u8 = 0;

pub const D3DDECLUSAGE_NORMAL: u8 = 3;

pub const D3DDECLUSAGE_TEXCOORD: u8 = 5;

pub const D3DDECLUSAGE_COLOR: u8 = 10;

pub const D3DDECLUSAGE_DEPTH: u8 = 12;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Semantic {
    pub usage: u8,
    pub usage_index: u8,
}

impl Semantic {
    pub const fn new(usage: u8, usage_index: u8) -> Self {
        Self { usage, usage_index }
    }

    pub const fn position(usage_index: u8) -> Self {
        Self::new(D3DDECLUSAGE_POSITION, usage_index)
    }

    pub const fn texcoord(usage_index: u8) -> Self {
        Self::new(D3DDECLUSAGE_TEXCOORD, usage_index)
    }
}
