use crate::{AddressMode, TextureFilter};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SamplerDecodeError {
    UnsupportedMinMagFilter { packed_word: u32, raw: u32 },

    UnsupportedMipFilter { packed_word: u32, raw: u32 },

    AnisotropyOverflow { packed_word: u32, anisotropy: u32 },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DecodedSamplerState {
    pub packed_word: u32,
    pub address_u: AddressMode,
    pub address_v: AddressMode,
    pub address_w: AddressMode,
    pub mag_filter: TextureFilter,
    pub min_filter: TextureFilter,
    pub mip_filter: TextureFilter,

    pub uses_mipmaps: bool,
    pub anisotropy_clamp: u16,
}

impl DecodedSamplerState {
    pub fn from_packed_word(
        packed_word: u32,
        address_u: AddressMode,
        address_v: AddressMode,
        address_w: AddressMode,
    ) -> Result<Self, SamplerDecodeError> {
        let nibble = |shift: u32| (packed_word >> shift) & 0xf;
        let mag_filter = decode_min_mag(nibble(12), packed_word)?;
        let min_filter = decode_min_mag(nibble(8), packed_word)?;
        let mip_raw = nibble(16);
        let (mip_filter, uses_mipmaps) = match TextureFilter::from_raw(mip_raw) {
            TextureFilter::None => (TextureFilter::None, false),
            TextureFilter::Point => (TextureFilter::Point, true),
            TextureFilter::Linear => (TextureFilter::Linear, true),
            other => {
                return Err(SamplerDecodeError::UnsupportedMipFilter {
                    packed_word,
                    raw: other.to_raw().unwrap_or(mip_raw),
                });
            }
        };
        let anisotropy = packed_word & 0xff;

        let linearish =
            |f: TextureFilter| matches!(f, TextureFilter::Linear | TextureFilter::Anisotropic);
        let anisotropy_clamp = if linearish(mag_filter) && linearish(min_filter) && anisotropy > 1 {
            match u16::try_from(anisotropy) {
                Ok(v) => v,
                Err(_) => {
                    return Err(SamplerDecodeError::AnisotropyOverflow {
                        packed_word,
                        anisotropy,
                    });
                }
            }
        } else {
            1
        };
        Ok(Self {
            packed_word,
            address_u,
            address_v,
            address_w,
            mag_filter,
            min_filter,
            mip_filter,
            uses_mipmaps,
            anisotropy_clamp,
        })
    }

    pub fn from_packed_word_and_clamp_flags(
        packed_word: u32,
        clamp_u: bool,
        clamp_v: bool,
        clamp_w: bool,
    ) -> Result<Self, SamplerDecodeError> {
        Self::from_packed_word(
            packed_word,
            AddressMode::from_clamp_flag(clamp_u),
            AddressMode::from_clamp_flag(clamp_v),
            AddressMode::from_clamp_flag(clamp_w),
        )
    }
}

fn decode_min_mag(raw: u32, packed_word: u32) -> Result<TextureFilter, SamplerDecodeError> {
    match TextureFilter::from_raw(raw) {
        TextureFilter::Point | TextureFilter::Linear | TextureFilter::Anisotropic => {
            Ok(TextureFilter::from_raw(raw))
        }
        _ => Err(SamplerDecodeError::UnsupportedMinMagFilter { packed_word, raw }),
    }
}
