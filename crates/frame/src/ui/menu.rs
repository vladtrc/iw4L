use bevy::prelude::*;
use master_protocol::ContentFamilies;

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

#[derive(Resource, Clone, Debug, PartialEq, Eq)]
pub struct AssetFamilies(pub ContentFamilies);

impl AssetFamilies {
    pub fn all_installed() -> Self {
        Self::from_families(
            asset_core::families()
                .iter()
                .map(|family| family.id().as_str()),
        )
    }
    pub fn settings() -> Vec<(asset_core::FamilyId, String)> {
        asset_core::families()
            .iter()
            .map(|family| family.id())
            .filter(|id| *id != asset_core::FamilyId::IW4)
            .map(|id| (id, format!("sv_assets_{}", id.as_str())))
            .collect()
    }
    pub fn from_families<'a>(families: impl IntoIterator<Item = &'a str>) -> Self {
        Self(ContentFamilies::from_families(families).expect("valid bounded family selection"))
    }
    pub fn enabled(&self, family: &str) -> bool {
        self.0.contains_family(family)
    }
    pub fn missing_notice(&self, have: &ContentFamilies) -> Option<String> {
        let missing = self.0.missing_from(have);
        if missing.is_empty() {
            return None;
        }
        let names = missing
            .iter()
            .map(|token| {
                asset_core::FamilyId::parse(token)
                    .and_then(asset_core::game_folder_family)
                    .map(|policy| format!("{} ({token})", policy.title))
                    .unwrap_or_else(|| token.to_owned())
            })
            .collect::<Vec<_>>()
            .join(", ");
        Some(format!(
            "Missing game files:\n{names}.\n\nInstall these games and select their game folders,\nor ask the host to disable their optional asset families."
        ))
    }
    pub fn from_inventory(have: &ContentFamilies, dvars: &UiMenuDvars) -> Result<Self, String> {
        let mut enabled = Vec::new();
        if have.contains_family("iw4") {
            enabled.push("iw4".to_owned());
        }
        for (id, setting) in Self::settings() {
            let family = id.as_str();
            let env = std::env::var(format!("IW4L_ASSETS_{}", family.to_ascii_uppercase())).ok();
            let value = dvars.get(&setting).or(env.as_deref());
            let allow = match value {
                None => have.contains_family(family),
                Some("0") => false,
                Some("1") => true,
                Some(_) => return Err(format!("{setting} must be 0 or 1")),
            };
            if allow {
                if !have.contains_family(family) {
                    return Err(format!(
                        "Cannot enable {family}: game content is not installed"
                    ));
                }
                enabled.push(family.to_owned());
            }
        }
        Ok(Self(
            ContentFamilies::from_families(enabled).map_err(|error| error.to_string())?,
        ))
    }
    pub fn require_map(&self, map: &str) -> Result<(), String> {
        let family = ContentFamilies::map_family(map);
        if self.enabled(family) {
            Ok(())
        } else {
            Err(format!(
                "Map `{map}` requires disabled or missing {family} content"
            ))
        }
    }
}
