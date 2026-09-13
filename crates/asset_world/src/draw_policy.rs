#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct WorldDrawPolicy {
    pub resolve_specular_env: bool,
    pub lightmap_requires_image: bool,
    pub decode_color_at_convert: bool,
}

impl Default for WorldDrawPolicy {
    fn default() -> Self {
        Self::iw4()
    }
}

impl WorldDrawPolicy {
    pub const fn iw4() -> Self {
        Self {
            resolve_specular_env: true,
            lightmap_requires_image: false,
            decode_color_at_convert: false,
        }
    }

    pub const fn t5() -> Self {
        Self {
            resolve_specular_env: false,
            lightmap_requires_image: true,
            decode_color_at_convert: true,
        }
    }

    pub const fn iw5() -> Self {
        Self {
            resolve_specular_env: false,
            lightmap_requires_image: false,
            decode_color_at_convert: false,
        }
    }
}
