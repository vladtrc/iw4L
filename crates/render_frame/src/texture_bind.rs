use render_material::{PackedCodeSamplerLane, PackedCodeSamplers, PackedLocalSamplers, PortId};

use crate::SurfaceSamplerInputs;

const FNV1A64_OFFSET: u64 = 0xcbf29ce484222325;
const FNV1A64_PRIME: u64 = 0x100_0000_01b3;

fn fnv1a64(bytes: &[u8]) -> u64 {
    fnv1a64_more(FNV1A64_OFFSET, bytes)
}

fn fnv1a64_more(mut hash: u64, bytes: &[u8]) -> u64 {
    for &byte in bytes {
        hash ^= u64::from(byte);
        hash = hash.wrapping_mul(FNV1A64_PRIME);
    }
    hash
}

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

fn hash_packed_code_lanes(lanes: &[PackedCodeSamplerLane]) -> u64 {
    let mut hash = fnv1a64(&(lanes.len() as u64).to_le_bytes());
    for lane in lanes {
        hash = fnv1a64_more(hash, &lane.register.to_le_bytes());
        hash = fnv1a64_more(hash, &lane.index.to_le_bytes());
        hash = fnv1a64_more(hash, &[lane.sampler_state]);
        match lane.image {
            Some(image) => {
                hash = fnv1a64_more(hash, &[1]);
                hash = fnv1a64_more(hash, &image.0.to_le_bytes());
            }
            None => {
                hash = fnv1a64_more(hash, &[0]);
            }
        }
    }
    hash
}
