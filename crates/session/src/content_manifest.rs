use std::collections::HashSet;

use assets::{AssetKey, AssetKind, WeaponRegistry};
use bevy::prelude::Resource;

pub const SESSION_MANIFEST_SCHEME: u32 = 1;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RuntimeRuleset {
    Iw4,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct SessionWeaponId(pub u32);

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ManifestFact<T> {
    Known(T),
    Unavailable(ManifestGap),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ManifestGap {
    MapSourceNotOpened,
    InvalidMapKey,
    ValidatedIw4WeaponProfileMissing,
    AtomicWeaponPresentationBundleNotInstalled,
    CharacterPackageNotInstalled,
    NamespacedMaterialCorpusNotInstalled,
    ProgramCorpusNotFrozen,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AuthorityWeaponProfile {
    Combat,
    Equipment,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SessionWeaponManifestRow {
    pub id: SessionWeaponId,
    pub key: AssetKey,
    pub authority: ManifestFact<AuthorityWeaponProfile>,

    pub presentation: ManifestFact<u64>,
}

#[derive(Clone, Debug, PartialEq, Eq, Resource)]
pub struct SessionContentManifest {
    pub scheme: u32,
    pub ruleset: RuntimeRuleset,
    pub map: ManifestFact<AssetKey>,
    pub gameplay_digest: u64,
    pub weapons: Vec<SessionWeaponManifestRow>,
    pub character_package: ManifestFact<AssetKey>,
    pub material_corpus: ManifestFact<u64>,
    pub program_corpus: ManifestFact<u64>,
    pub digest: u64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SessionManifestError {
    MissingWeaponKey(SessionWeaponId),
    DuplicateWeaponKey(AssetKey),
}

impl core::fmt::Display for SessionManifestError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::MissingWeaponKey(id) => write!(f, "session weapon {} has no durable key", id.0),
            Self::DuplicateWeaponKey(key) => {
                write!(f, "duplicate durable session weapon key `{key}`")
            }
        }
    }
}

impl std::error::Error for SessionManifestError {}

impl SessionContentManifest {
    pub fn build(
        map: &assets::PreparedMap,
        registry: &WeaponRegistry,
        combat: &[sim::WeaponCombatFacts],
        equipment: &[sim::EquipmentRuntimeFacts],
        gameplay_digest: u64,
    ) -> Result<Self, SessionManifestError> {
        let map = match map.namespace {
            Some(namespace) => match AssetKey::new(namespace, AssetKind::Map, map.zone.clone()) {
                Ok(key) => ManifestFact::Known(key),
                Err(_) => ManifestFact::Unavailable(ManifestGap::InvalidMapKey),
            },
            None => ManifestFact::Unavailable(ManifestGap::MapSourceNotOpened),
        };

        let mut keys = HashSet::with_capacity(registry.len());
        let mut weapons = Vec::with_capacity(registry.len());
        for raw_id in 1..=registry.len() as u32 {
            let id = SessionWeaponId(raw_id);
            let key = registry
                .key_of(raw_id)
                .ok_or(SessionManifestError::MissingWeaponKey(id))?;
            if !keys.insert(key.clone()) {
                return Err(SessionManifestError::DuplicateWeaponKey(key));
            }
            let authority = if combat
                .get(raw_id as usize)
                .copied()
                .is_some_and(sim::WeaponCombatFacts::is_usable)
            {
                ManifestFact::Known(AuthorityWeaponProfile::Combat)
            } else if equipment
                .get(raw_id as usize)
                .copied()
                .is_some_and(sim::EquipmentRuntimeFacts::is_offhand)
            {
                ManifestFact::Known(AuthorityWeaponProfile::Equipment)
            } else {
                ManifestFact::Unavailable(ManifestGap::ValidatedIw4WeaponProfileMissing)
            };
            weapons.push(SessionWeaponManifestRow {
                id,
                key,
                authority,
                presentation: ManifestFact::Unavailable(
                    ManifestGap::AtomicWeaponPresentationBundleNotInstalled,
                ),
            });
        }

        let mut manifest = Self {
            scheme: SESSION_MANIFEST_SCHEME,
            ruleset: RuntimeRuleset::Iw4,
            map,
            gameplay_digest,
            weapons,
            character_package: ManifestFact::Unavailable(ManifestGap::CharacterPackageNotInstalled),
            material_corpus: ManifestFact::Unavailable(
                ManifestGap::NamespacedMaterialCorpusNotInstalled,
            ),
            program_corpus: ManifestFact::Unavailable(ManifestGap::ProgramCorpusNotFrozen),
            digest: 0,
        };
        manifest.digest = manifest.compute_digest();
        Ok(manifest)
    }

    pub fn match_descriptor(
        &self,
        components: sim::ContentComponents,
    ) -> Option<net::MatchDescriptor> {
        match self.map {
            ManifestFact::Known(_) => net::MatchDescriptor::from_components(components),
            ManifestFact::Unavailable(_) => None,
        }
    }

    fn compute_digest(&self) -> u64 {
        let mut digest = Digest::new();
        digest.u32(self.scheme);
        digest.byte(match self.ruleset {
            RuntimeRuleset::Iw4 => 1,
        });
        digest.asset_fact(&self.map);
        digest.u64(self.gameplay_digest);
        digest.u64(self.weapons.len() as u64);
        for row in &self.weapons {
            digest.u32(row.id.0);
            digest.asset_key(&row.key);
            digest.authority_fact(&row.authority);
            digest.u64_fact(&row.presentation);
        }
        digest.asset_fact(&self.character_package);
        digest.u64_fact(&self.material_corpus);
        digest.u64_fact(&self.program_corpus);
        digest.finish()
    }
}

struct Digest(u64);

impl Digest {
    fn new() -> Self {
        Self(0xcbf29ce484222325)
    }

    fn byte(&mut self, value: u8) {
        self.0 ^= u64::from(value);
        self.0 = self.0.wrapping_mul(0x100000001b3);
    }

    fn bytes(&mut self, value: &[u8]) {
        self.u64(value.len() as u64);
        for byte in value {
            self.byte(*byte);
        }
    }

    fn u32(&mut self, value: u32) {
        for byte in value.to_le_bytes() {
            self.byte(byte);
        }
    }

    fn u64(&mut self, value: u64) {
        for byte in value.to_le_bytes() {
            self.byte(byte);
        }
    }

    fn asset_key(&mut self, key: &AssetKey) {
        self.byte(namespace_tag(key.namespace));
        self.byte(kind_tag(key.kind));
        self.bytes(key.name.as_bytes());
    }

    fn asset_fact(&mut self, fact: &ManifestFact<AssetKey>) {
        match fact {
            ManifestFact::Known(key) => {
                self.byte(1);
                self.asset_key(key);
            }
            ManifestFact::Unavailable(gap) => {
                self.byte(0);
                self.byte(gap_tag(*gap));
            }
        }
    }

    fn authority_fact(&mut self, fact: &ManifestFact<AuthorityWeaponProfile>) {
        match fact {
            ManifestFact::Known(AuthorityWeaponProfile::Combat) => self.byte(1),
            ManifestFact::Known(AuthorityWeaponProfile::Equipment) => self.byte(2),
            ManifestFact::Unavailable(gap) => {
                self.byte(0);
                self.byte(gap_tag(*gap));
            }
        }
    }

    fn u64_fact(&mut self, fact: &ManifestFact<u64>) {
        match fact {
            ManifestFact::Known(value) => {
                self.byte(1);
                self.u64(*value);
            }
            ManifestFact::Unavailable(gap) => {
                self.byte(0);
                self.byte(gap_tag(*gap));
            }
        }
    }

    fn finish(self) -> u64 {
        self.0
    }
}

fn namespace_tag(namespace: assets::AssetNamespace) -> u8 {
    match namespace {
        assets::AssetNamespace::Iw4 => 1,
        assets::AssetNamespace::Iw5 => 2,
        assets::AssetNamespace::T5 => 3,
    }
}

fn kind_tag(kind: AssetKind) -> u8 {
    match kind {
        AssetKind::Weapon => 1,
        AssetKind::Map => 2,
        AssetKind::XModel => 3,
        AssetKind::Anim => 4,
        AssetKind::Material => 5,
    }
}

fn gap_tag(gap: ManifestGap) -> u8 {
    match gap {
        ManifestGap::MapSourceNotOpened => 1,
        ManifestGap::InvalidMapKey => 2,
        ManifestGap::ValidatedIw4WeaponProfileMissing => 3,
        ManifestGap::AtomicWeaponPresentationBundleNotInstalled => 4,
        ManifestGap::CharacterPackageNotInstalled => 5,
        ManifestGap::NamespacedMaterialCorpusNotInstalled => 6,
        ManifestGap::ProgramCorpusNotFrozen => 7,
    }
}
