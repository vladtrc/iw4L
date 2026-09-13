use crate::ZoneGame;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum AssetNamespace {
    #[default]
    Iw4,
    T5,
    Iw5,
}

impl AssetNamespace {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Iw4 => "iw4",
            Self::T5 => "t5",
            Self::Iw5 => "iw5",
        }
    }

    pub fn from_zone_game(game: ZoneGame) -> Self {
        match game {
            ZoneGame::Iw4 => Self::Iw4,
            ZoneGame::T5 => Self::T5,
            ZoneGame::Iw5 => Self::Iw5,
        }
    }

    pub fn parse(token: &str) -> Option<Self> {
        match token {
            "iw4" => Some(Self::Iw4),
            "t5" => Some(Self::T5),
            "iw5" => Some(Self::Iw5),
            _ => None,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum AssetKind {
    Weapon,
    Map,
    XModel,
    Anim,
    Material,
}

impl AssetKind {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Weapon => "weapon",
            Self::Map => "map",
            Self::XModel => "xmodel",
            Self::Anim => "anim",
            Self::Material => "material",
        }
    }

    pub fn parse(token: &str) -> Option<Self> {
        match token {
            "weapon" => Some(Self::Weapon),
            "map" => Some(Self::Map),
            "xmodel" => Some(Self::XModel),
            "anim" => Some(Self::Anim),
            "material" => Some(Self::Material),
            _ => None,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct AssetKey {
    pub namespace: AssetNamespace,
    pub kind: AssetKind,
    pub name: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AssetKeyError {
    MissingNamespace,

    UnknownNamespace,

    UnknownKind,

    EmptyName,
}

impl core::fmt::Display for AssetKeyError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::MissingNamespace => {
                write!(f, "asset key missing namespace (want iw4:/t5:/iw5:)")
            }
            Self::UnknownNamespace => write!(f, "asset key unknown namespace"),
            Self::UnknownKind => write!(f, "asset key missing or unknown kind"),
            Self::EmptyName => write!(f, "asset key empty logical name"),
        }
    }
}

impl std::error::Error for AssetKeyError {}

impl AssetKey {
    pub fn new(
        namespace: AssetNamespace,
        kind: AssetKind,
        name: impl Into<String>,
    ) -> Result<Self, AssetKeyError> {
        let name = name.into();
        if name.is_empty() || name.contains(':') || name.contains('/') {
            return Err(AssetKeyError::EmptyName);
        }
        Ok(Self {
            namespace,
            kind,
            name,
        })
    }

    pub fn parse(raw: &str) -> Result<Self, AssetKeyError> {
        let Some((ns, rest)) = raw.split_once(':') else {
            return Err(AssetKeyError::MissingNamespace);
        };
        let namespace = AssetNamespace::parse(ns).ok_or(AssetKeyError::UnknownNamespace)?;
        let Some((kind_tok, name)) = rest.split_once('/') else {
            return Err(AssetKeyError::UnknownKind);
        };
        let kind = AssetKind::parse(kind_tok).ok_or(AssetKeyError::UnknownKind)?;
        if name.is_empty() {
            return Err(AssetKeyError::EmptyName);
        }
        Ok(Self {
            namespace,
            kind,
            name: name.to_owned(),
        })
    }

    pub fn display(&self) -> String {
        format!(
            "{}:{}/{}",
            self.namespace.as_str(),
            self.kind.as_str(),
            self.name
        )
    }

    pub fn logical_name(&self) -> &str {
        &self.name
    }
}

impl core::fmt::Display for AssetKey {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(
            f,
            "{}:{}/{}",
            self.namespace.as_str(),
            self.kind.as_str(),
            self.name
        )
    }
}
