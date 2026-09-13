use asset_core::AssetNamespace;
use lighting_iw4::is_lit_remap_slot;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum PassColorSpace {
    #[default]
    Linear,
    GammaEncoded,
    Unknown,
}

impl PassColorSpace {
    pub const fn port_mix(self) -> u64 {
        match self {
            Self::Linear => 0,
            Self::GammaEncoded => 0xC01C_5ACE,
            Self::Unknown => 0xA11C_0001,
        }
    }
}

pub fn pass_color_space(namespace: AssetNamespace, tech_slot: u8) -> PassColorSpace {
    match namespace {
        AssetNamespace::Iw4 | AssetNamespace::Iw5 => PassColorSpace::Linear,
        AssetNamespace::T5 => {
            if is_lit_remap_slot(tech_slot) {
                PassColorSpace::GammaEncoded
            } else {
                PassColorSpace::Unknown
            }
        }
    }
}
