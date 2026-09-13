#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct TechType(pub u8);

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum SetupArm {
    LitModelLighting,
    DepthPrepass,
    DebugBumpmapRemap,
    Generic,
}

impl SetupArm {
    pub fn classify(tech_type: TechType, packed: u64) -> Self {
        let tech = tech_type.0;
        let scene = (packed >> 24) as u8;

        if lighting_iw4::is_lit_remap_slot(tech) {
            Self::LitModelLighting
        } else if tech < 2 {
            Self::DepthPrepass
        } else if tech == 0x2e && (scene == 3 || scene == 4) {
            Self::DebugBumpmapRemap
        } else {
            Self::Generic
        }
    }
}
