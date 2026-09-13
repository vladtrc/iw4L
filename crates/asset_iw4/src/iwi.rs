pub const IWI_V8_HEADER_LEN: usize = 32;

pub const IWI_MAGIC: [u8; 3] = *b"IWi";

pub const IWI_VERSION_V8: u8 = 8;

pub const IWI_FLAG_NO_MIPMAPS: u8 = 0x2;

pub const IWI_USAGE_COLOR: u8 = 0x0;
pub const IWI_USAGE_NORMAL: u8 = 0x4;
pub const IWI_USAGE_SKYBOX_A: u8 = 0x1;
pub const IWI_USAGE_SKYBOX_B: u8 = 0x9;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IwiHeaderError {
    ShortHeader,
    BadMagic,

    UnsupportedVersion(u8),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct IwiHeader {
    pub flags: u8,
    pub usage: u8,
    pub format: u8,
    pub width: u16,
    pub height: u16,
    pub depth: u16,
    pub file_size_for_picmip: [u32; 4],
}

impl IwiHeader {
    pub fn parse(bytes: &[u8]) -> Result<Self, IwiHeaderError> {
        if bytes.len() < IWI_V8_HEADER_LEN {
            return Err(IwiHeaderError::ShortHeader);
        }
        if bytes[..3] != IWI_MAGIC {
            return Err(IwiHeaderError::BadMagic);
        }
        if bytes[3] != IWI_VERSION_V8 {
            return Err(IwiHeaderError::UnsupportedVersion(bytes[3]));
        }
        let u16_at = |o: usize| u16::from_le_bytes([bytes[o], bytes[o + 1]]);
        let u32_at = |o: usize| u32::from_le_bytes(bytes[o..o + 4].try_into().unwrap());
        Ok(Self {
            flags: bytes[4],
            usage: bytes[6],
            format: bytes[8],
            width: u16_at(10),
            height: u16_at(12),
            depth: u16_at(14),
            file_size_for_picmip: [u32_at(16), u32_at(20), u32_at(24), u32_at(28)],
        })
    }

    #[inline]
    pub fn no_mipmaps(self) -> bool {
        self.flags & IWI_FLAG_NO_MIPMAPS != 0
    }

    #[inline]
    pub fn is_skybox(self) -> bool {
        self.usage == IWI_USAGE_SKYBOX_A || self.usage == IWI_USAGE_SKYBOX_B
    }
}
