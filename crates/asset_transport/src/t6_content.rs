use std::path::Path;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum T6ContentMode {
    Multiplayer,
    Zombies,
}

impl T6ContentMode {
    pub fn for_path(path: &Path) -> Self {
        let stem = path.file_stem().and_then(|s| s.to_str()).unwrap_or("");
        let stem = stem.to_ascii_lowercase();
        if stem.starts_with("zm_") || stem.ends_with("_zm") || stem.contains("_zm_") {
            Self::Zombies
        } else {
            Self::Multiplayer
        }
    }

    pub fn common(self) -> &'static str {
        match self {
            Self::Multiplayer => "common_mp",
            Self::Zombies => "common_zm",
        }
    }

    pub fn patch(self) -> &'static str {
        match self {
            Self::Multiplayer => "patch_mp",
            Self::Zombies => "patch_zm",
        }
    }

    pub fn startup(self) -> [&'static str; 3] {
        match self {
            Self::Multiplayer => ["code_post_gfx_mp", "localized_code_post_gfx_mp", "patch_mp"],
            Self::Zombies => ["code_post_gfx_zm", "localized_code_post_gfx_zm", "patch_zm"],
        }
    }

    pub fn localized(self) -> [&'static str; 3] {
        match self {
            Self::Multiplayer => ["patch_mp", "ui_mp", "code_post_gfx_mp"],
            Self::Zombies => ["patch_zm", "ui_zm", "code_post_gfx_zm"],
        }
    }

    pub fn icons(self) -> [&'static str; 3] {
        match self {
            Self::Multiplayer => ["code_post_gfx_mp.ff", "patch_ui_mp.ff", "ui_mp.ff"],
            Self::Zombies => ["code_post_gfx_zm.ff", "patch_ui_zm.ff", "ui_zm.ff"],
        }
    }

    pub fn supplements(self, path: &Path) -> Vec<String> {
        let mut zones = vec![self.patch().to_owned()];
        if self == Self::Zombies {
            let map = path.file_stem().and_then(|s| s.to_str()).unwrap_or("");
            if map.starts_with("zm_") {
                zones.push(format!("so_zclassic_{map}"));
                zones.push(format!("patch_{map}"));
            }
        }
        zones
    }
}
