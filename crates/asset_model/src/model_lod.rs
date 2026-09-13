pub use fastfile_t5::xmodel_lod as t5_lod;

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum ModelLodSelector {
    Iw4 {
        lod_start: u8,
        num_lods: u8,
        lod_dist: [f32; 4],
    },

    T5 {
        num_lods: i16,
        lod_dist: [f32; 4],
    },
}

impl ModelLodSelector {
    #[must_use]
    pub fn lod_dist(self) -> [f32; 4] {
        match self {
            Self::Iw4 { lod_dist, .. } | Self::T5 { lod_dist, .. } => lod_dist,
        }
    }

    #[must_use]
    pub fn num_lods(self) -> i16 {
        match self {
            Self::Iw4 { num_lods, .. } => i16::from(num_lods),
            Self::T5 { num_lods, .. } => num_lods,
        }
    }
}
