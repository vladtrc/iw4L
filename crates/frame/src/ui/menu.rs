use bevy::prelude::*;
use master_protocol::ContentFlags;

#[derive(Resource, Default, Debug)]
pub struct UiPartyState {
    pub active: bool,
    pub in_lobby: bool,
    pub is_host: bool,
}

#[derive(Resource, Default, Debug)]
pub struct UiMenuDvars {
    values: std::collections::HashMap<String, String>,
    choices: std::collections::HashMap<String, Vec<(String, String)>>,
}

impl UiMenuDvars {
    pub fn get(&self, name: &str) -> Option<&str> {
        self.values
            .get(&name.to_ascii_lowercase())
            .map(String::as_str)
    }

    pub fn set(&mut self, name: &str, value: impl Into<String>) {
        self.values.insert(name.to_ascii_lowercase(), value.into());
    }

    pub fn choices(&self, name: &str) -> Option<&[(String, String)]> {
        self.choices
            .get(&name.to_ascii_lowercase())
            .map(Vec::as_slice)
    }

    pub fn set_choices(&mut self, name: &str, choices: Vec<(String, String)>) {
        self.choices.insert(name.to_ascii_lowercase(), choices);
    }

    pub fn iter(&self) -> impl Iterator<Item = (&str, &str)> {
        self.values
            .iter()
            .map(|(name, value)| (name.as_str(), value.as_str()))
    }
}

#[derive(Resource, Default, Debug)]
pub struct FrontendContentLoading;

#[derive(Resource, Clone, Default, Debug)]
pub struct HostMatchRules(pub Vec<(String, String)>);

#[derive(Message, Clone, Debug)]
pub struct UiExecCommand {
    pub text: String,
}

#[derive(Message, Clone, Debug, PartialEq, Eq)]
pub enum UiMenuRequest {
    Toggle,
    RestoreLobby,
    Open(String),
    Close(String),
    Focus { menu: String, item: String },
    Key(UiMenuKey),
    Text(String),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UiMenuKey {
    Escape,
    Enter,
    Up,
    Down,
    Left,
    Right,
    Home,
    End,
    Backspace,
    Delete,
}

#[derive(Resource, Clone, Copy, Debug, PartialEq, Eq)]
pub struct AssetFamilies(pub u8);

impl AssetFamilies {
    pub const IW4: Self = Self(ContentFlags::IW4.0);
    pub const ALL: Self = Self(ContentFlags::KNOWN.0);

    pub fn iw4() -> Self {
        Self::IW4
    }
    pub const SETTINGS: [(&'static str, &'static str, u8); 3] = [
        ("iw5", "sv_assets_iw5", ContentFlags::IW5.0),
        ("t5", "sv_assets_t5", ContentFlags::T5.0),
        ("t6", "sv_assets_t6", ContentFlags::T6.0),
    ];

    pub fn from_families<'a>(families: impl IntoIterator<Item = &'a str>) -> Self {
        Self(
            families
                .into_iter()
                .filter_map(ContentFlags::family)
                .fold(0, |mask, flag| mask | flag.0),
        )
    }

    pub fn enabled(self, family: &str) -> bool {
        ContentFlags::family(family).is_some_and(|flag| ContentFlags(self.0).contains(flag))
    }

    pub fn missing_notice(self, have: u8) -> Option<String> {
        let missing = self.0 & !have;
        if missing == 0 {
            return None;
        }
        let names = [
            (ContentFlags::IW4, "Modern Warfare 2 (IW4)"),
            (ContentFlags::IW5, "Modern Warfare 3 (IW5)"),
            (ContentFlags::T5, "Black Ops (T5)"),
            (ContentFlags::T6, "Black Ops II (T6)"),
        ]
        .into_iter()
        .filter_map(|(flag, name)| ContentFlags(missing).contains(flag).then_some(name))
        .collect::<Vec<_>>()
        .join(", ");
        let names = if missing & !ContentFlags::KNOWN.0 != 0 {
            if names.is_empty() {
                "unknown game content (update IW4L)".to_owned()
            } else {
                format!("{names}, unknown game content (update IW4L)")
            }
        } else {
            names
        };
        Some(format!(
            "Missing game files:\n{names}.\n\nInstall these games and select their game folders,\nor ask the host to disable their optional asset families."
        ))
    }

    pub fn from_inventory(have: u8, dvars: &UiMenuDvars) -> Result<Self, String> {
        let mut enabled = have & ContentFlags::IW4.0;
        for (family, setting, bit) in Self::SETTINGS {
            let env = std::env::var(format!("IW4L_ASSETS_{}", family.to_ascii_uppercase())).ok();
            let value = dvars.get(setting).or(env.as_deref());
            let allow = match value {
                None => have & bit != 0,
                Some("0") => false,
                Some("1") => true,
                Some(_) => return Err(format!("{setting} must be 0 or 1")),
            };
            if allow {
                if have & bit == 0 {
                    return Err(format!(
                        "Cannot enable {family}: game content is not installed"
                    ));
                }
                enabled |= bit;
            }
        }
        Ok(Self(enabled))
    }

    pub fn require_map(self, map: &str) -> Result<(), String> {
        let family = ContentFlags::map_family(map);
        if self.enabled(family) {
            Ok(())
        } else {
            Err(format!(
                "Map `{map}` requires disabled or missing {family} content"
            ))
        }
    }
}
