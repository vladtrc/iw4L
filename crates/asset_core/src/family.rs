use std::collections::BTreeMap;
use std::sync::OnceLock;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct FamilyId(&'static str);

impl FamilyId {
    pub const IW4: Self = Self("iw4");

    pub const fn new(token: &'static str) -> Self {
        assert!(Self::valid_token(token), "invalid family token");
        Self(token)
    }

    const fn valid_token(token: &str) -> bool {
        let bytes = token.as_bytes();
        if bytes.is_empty() || bytes.len() > 64 {
            return false;
        }
        let mut i = 0;
        while i < bytes.len() {
            let b = bytes[i];
            if !b.is_ascii_lowercase() && !b.is_ascii_digit() && b != b'_' && b != b'-' {
                return false;
            }
            i += 1;
        }
        true
    }

    /// Resolve a token to an installed family. Tokens come from zone keys,
    /// asset keys and peers, so an unknown one is refused rather than
    /// interned: nothing outside the builder can grow the set of families.
    pub fn parse(token: &str) -> Option<Self> {
        FAMILIES
            .get()?
            .families
            .keys()
            .find(|id| id.0 == token)
            .copied()
    }
    pub const fn as_str(self) -> &'static str {
        self.0
    }
}

/// How a family's zones are recognised and which ones it always loads.
/// Asset-level hooks live on `game_family::GameFamily`, which extends this.
pub trait ZoneFormat: Send + Sync + 'static {
    fn id(&self) -> FamilyId;
    fn zone_version(&self) -> u32;
    fn accepts_zone_header(&self, header: &[u8]) -> bool;
    fn anchor_zone(&self) -> &'static str;
    fn localized_zones(&self, language: &str) -> Vec<String>;
}

// Ordered by id, not by registration order: weapon rows and content hashes
// must not depend on the order the builder adds the games in.
#[derive(Default)]
pub struct FamilyRegistry {
    families: BTreeMap<FamilyId, &'static dyn ZoneFormat>,
}

impl FamilyRegistry {
    pub fn register(&mut self, family: &'static dyn ZoneFormat) {
        assert!(
            !self.families.contains_key(&family.id()),
            "duplicate family {}",
            family.id().as_str()
        );
        self.families.insert(family.id(), family);
    }
    pub fn get(&self, id: FamilyId) -> Option<&'static dyn ZoneFormat> {
        self.families.get(&id).copied()
    }
    pub fn iter(&self) -> impl Iterator<Item = &'static dyn ZoneFormat> + '_ {
        self.families.values().copied()
    }
}

/// Filled by `game_family::install` before any discovery or asset load starts.
static FAMILIES: OnceLock<FamilyRegistry> = OnceLock::new();

pub fn install_families(registry: FamilyRegistry) -> Result<(), &'static str> {
    FAMILIES
        .set(registry)
        .map_err(|_| "families already installed")
}

pub fn families() -> &'static FamilyRegistry {
    FAMILIES
        .get()
        .expect("builder must install families before discovery")
}
