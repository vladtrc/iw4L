use render_material::{
    PackedCodeSamplerLane, PackedCodeSamplers, PackedLocalSamplers, PortId, hash_packed_code_lanes,
};

use crate::SurfaceSamplerInputs;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct TextureBindIdentity {
    pub port: PortId,
    pub local: u64,
    pub code: u64,
    pub surface: SurfaceSamplerInputs,
}

impl TextureBindIdentity {
    pub fn from_packed(
        port: PortId,
        local: Option<&PackedLocalSamplers>,
        code: &PackedCodeSamplers,
        surface: SurfaceSamplerInputs,
    ) -> Self {
        Self {
            port,
            local: local.map(|packed| packed.id).unwrap_or(0),
            code: code.id,
            surface,
        }
    }

    pub fn from_lanes(
        port: PortId,
        local: Option<&PackedLocalSamplers>,
        code: &[PackedCodeSamplerLane],
        surface: SurfaceSamplerInputs,
    ) -> Self {
        Self {
            port,
            local: local.map(|packed| packed.id).unwrap_or(0),
            code: hash_packed_code_lanes(code),
            surface,
        }
    }
}
