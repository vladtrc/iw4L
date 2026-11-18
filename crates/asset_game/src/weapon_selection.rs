use crate::{AssetKey, AssetKind, CacAuthoredCategory, FamilyId};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LoadoutRules {
    pub max_attachments: usize,
}

impl Default for LoadoutRules {
    fn default() -> Self {
        Self { max_attachments: 2 }
    }
}

impl LoadoutRules {
    pub fn for_class(perk1: &str) -> Self {
        Self {
            max_attachments: if perk1 == "specialty_bling" { 2 } else { 1 },
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct FamilyKey {
    pub namespace: FamilyId,
    pub base: String,
}

impl FamilyKey {
    pub fn new(namespace: FamilyId, base: &str) -> Self {
        let base = base.to_ascii_lowercase();
        let base = base.strip_suffix("_mp").unwrap_or(&base).to_owned();
        Self { namespace, base }
    }

    pub fn parse(raw: &str) -> Option<Self> {
        if let Ok(key) = AssetKey::parse(raw) {
            return (key.kind == AssetKind::Weapon)
                .then(|| Self::new(key.namespace, key.logical_name()));
        }
        let (game, name) = raw.split_once(':')?;
        let namespace = FamilyId::parse(game)?;
        (!name.is_empty() && !name.contains('/')).then(|| Self::new(namespace, name))
    }

    pub fn short(&self) -> String {
        let game = self.namespace.as_str();
        let name = self
            .base
            .strip_prefix(game)
            .and_then(|rest| rest.strip_prefix('_'))
            .unwrap_or(&self.base);
        format!("{game}:{name}")
    }

    pub fn asset_key(&self) -> String {
        format!("{}:weapon/{}_mp", self.namespace.as_str(), self.base)
    }
}

impl core::fmt::Display for FamilyKey {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str(&self.asset_key())
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FamilySlot {
    Primary,
    Secondary,
    Lethal,
    Tactical,
    Other,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AttachmentChoice {
    pub name: String,
    pub point: String,
    pub caption_key: String,
    pub icon: String,
    pub desc_key: String,
    pub order: u32,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WeaponFamily {
    pub key: FamilyKey,
    pub item_group: String,
    pub category: Option<CacAuthoredCategory>,
    pub slot: FamilySlot,
    pub display_key: String,
    pub image: String,
    pub base: Option<u32>,
    pub attachments: Vec<AttachmentChoice>,
}

impl WeaponFamily {
    pub fn name_key(&self) -> String {
        let key = self.display_key.trim_start_matches('@');
        match self.key.namespace {
            FamilyId::IW4 => key.to_owned(),
            namespace => format!("{}:localize/{key}", namespace.as_str()),
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Hash)]
pub struct WeaponSelection {
    pub family: Option<FamilyKey>,
    pub attachments: Vec<String>,
}

impl WeaponSelection {
    pub fn bare(family: FamilyKey) -> Self {
        Self {
            family: Some(family),
            attachments: Vec::new(),
        }
    }

    pub fn with(family: FamilyKey, attachments: &[String]) -> Self {
        Self {
            family: Some(family),
            attachments: attachments.to_vec(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ConfigurationRefusal {
    UnknownFamily(String),
    UnknownAttachment(String),
    NotOffered(String),
    Incompatible {
        a: String,
        b: String,
    },
    RuleRestricted(String),
    MissingContent(String),
    MissingDependency {
        weapon: String,
        kind: &'static str,
        name: String,
    },
    Unsupported(String),
}

impl ConfigurationRefusal {
    pub fn code(&self) -> &'static str {
        match self {
            Self::UnknownFamily(_) => "weapon.unknown_family",
            Self::UnknownAttachment(_) => "weapon.unknown_attachment",
            Self::NotOffered(_) => "weapon.not_offered",
            Self::Incompatible { .. } => "weapon.incompatible",
            Self::RuleRestricted(_) => "weapon.rule_restricted",
            Self::MissingContent(_) => "weapon.missing_content",
            Self::MissingDependency { .. } => "weapon.missing_dependency",
            Self::Unsupported(_) => "weapon.unsupported",
        }
    }
}

impl core::fmt::Display for ConfigurationRefusal {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::UnknownFamily(name) => write!(f, "no weapon family `{name}`"),
            Self::UnknownAttachment(name) => write!(f, "no attachment `{name}` in this game"),
            Self::NotOffered(name) => write!(f, "`{name}` is not offered for this weapon"),
            Self::Incompatible { a, b } => write!(f, "`{a}` and `{b}` cannot be combined"),
            Self::RuleRestricted(why) => write!(f, "not allowed here: {why}"),
            Self::MissingContent(name) => write!(f, "`{name}` is not in the loaded content"),
            Self::MissingDependency { weapon, kind, name } => {
                write!(f, "`{weapon}` names {kind} `{name}`, which is not loaded")
            }
            Self::Unsupported(why) => write!(f, "not supported yet: {why}"),
        }
    }
}

impl std::error::Error for ConfigurationRefusal {}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CacOffhandBucket {
    Lethal,
    Tactical,
}

pub fn cac_offhand_bucket(offhand_class: i32) -> Option<CacOffhandBucket> {
    match offhand_class {
        1 | 4 | 5 => Some(CacOffhandBucket::Lethal),
        2 | 3 => Some(CacOffhandBucket::Tactical),
        _ => None,
    }
}
