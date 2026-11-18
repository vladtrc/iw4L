#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum RuntimeProfile {
    #[default]
    Multiplayer,
    Campaign,
}

impl RuntimeProfile {
    pub const fn token(self) -> &'static str {
        match self {
            Self::Multiplayer => "multiplayer",
            Self::Campaign => "campaign",
        }
    }

    pub const fn common_zone(self) -> &'static str {
        match self {
            Self::Multiplayer => "common_mp",
            Self::Campaign => "common",
        }
    }

    pub const fn startup_zones(self) -> &'static [&'static str] {
        match self {
            Self::Multiplayer => &["code_post_gfx_mp", "localized_code_post_gfx_mp", "patch_mp"],
            Self::Campaign => &["code_post_gfx", "localized_code_post_gfx", "patch"],
        }
    }
}
