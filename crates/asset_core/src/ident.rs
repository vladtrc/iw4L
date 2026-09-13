use std::{collections::BTreeSet, fmt, marker::PhantomData, sync::Mutex};

pub trait IndexSpace:
    Copy + Clone + fmt::Debug + PartialEq + Eq + std::hash::Hash + 'static
{
    const NAME: &'static str;
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum MaterialSpace {}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum TechniqueSetSpace {}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum TracerSpace {}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum FxSpace {}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum FxModelSpace {}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum LoadedSoundSpace {}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum SoundAliasSpace {}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum XAnimSpace {}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum MapXModelSpace {}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ProjectileModelSpace {}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum FpvMeshSpace {}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum WorldWeaponSpace {}

impl IndexSpace for MaterialSpace {
    const NAME: &'static str = "material";
}
impl IndexSpace for TechniqueSetSpace {
    const NAME: &'static str = "technique_set";
}
impl IndexSpace for TracerSpace {
    const NAME: &'static str = "tracer";
}
impl IndexSpace for FxSpace {
    const NAME: &'static str = "fx";
}
impl IndexSpace for FxModelSpace {
    const NAME: &'static str = "fx_model";
}
impl IndexSpace for LoadedSoundSpace {
    const NAME: &'static str = "loaded_sound";
}
impl IndexSpace for SoundAliasSpace {
    const NAME: &'static str = "sound_alias";
}
impl IndexSpace for XAnimSpace {
    const NAME: &'static str = "xanim";
}
impl IndexSpace for MapXModelSpace {
    const NAME: &'static str = "map_xmodel";
}
impl IndexSpace for ProjectileModelSpace {
    const NAME: &'static str = "projectile_model";
}
impl IndexSpace for FpvMeshSpace {
    const NAME: &'static str = "fpv_mesh";
}
impl IndexSpace for WorldWeaponSpace {
    const NAME: &'static str = "world_weapon";
}

pub type MaterialIndex = CatalogIndex<MaterialSpace>;

pub type TechniqueSetIndex = CatalogIndex<TechniqueSetSpace>;

pub type TracerIndex = CatalogIndex<TracerSpace>;

pub type FxIndex = CatalogIndex<FxSpace>;

pub type FxModelIndex = CatalogIndex<FxModelSpace>;

pub type LoadedSoundIndex = CatalogIndex<LoadedSoundSpace>;

pub type SoundAliasIndex = CatalogIndex<SoundAliasSpace>;

pub type XAnimIndex = CatalogIndex<XAnimSpace>;

pub type MapXModelIndex = CatalogIndex<MapXModelSpace>;

pub type ProjectileModelIndex = CatalogIndex<ProjectileModelSpace>;

pub type FpvMeshIndex = CatalogIndex<FpvMeshSpace>;

pub type WorldWeaponIndex = CatalogIndex<WorldWeaponSpace>;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct CatalogIndex<S: IndexSpace> {
    order: usize,
    _space: PhantomData<S>,
}

impl<S: IndexSpace> CatalogIndex<S> {
    pub fn from_order(order: usize) -> Self {
        Self {
            order,
            _space: PhantomData,
        }
    }

    pub fn order(self) -> usize {
        self.order
    }

    pub fn space_name() -> &'static str {
        S::NAME
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct WalkLocalMaterialIndex(usize);

impl WalkLocalMaterialIndex {
    pub fn from_walk(order: usize) -> Self {
        Self(order)
    }

    pub fn get(self) -> usize {
        self.0
    }

    pub fn space_name() -> &'static str {
        "walk_local_material"
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum AssetRef {
    Real(String),

    Reference(String),
}

impl AssetRef {
    pub fn decode(raw: &str) -> Self {
        match raw.strip_prefix(',') {
            Some(bare) => Self::Reference(bare.to_owned()),
            None => Self::Real(raw.to_owned()),
        }
    }

    pub fn bare_name(raw: &str) -> &str {
        raw.strip_prefix(',').unwrap_or(raw)
    }

    pub fn as_str(&self) -> &str {
        match self {
            Self::Real(name) | Self::Reference(name) => name.as_str(),
        }
    }

    pub fn is_real(&self) -> bool {
        matches!(self, Self::Real(_))
    }

    pub fn is_reference(&self) -> bool {
        matches!(self, Self::Reference(_))
    }

    pub fn is_empty(&self) -> bool {
        self.as_str().is_empty()
    }

    pub fn same_name(&self, other: &Self) -> bool {
        self.as_str() == other.as_str()
    }

    pub fn incoming_owns_slot(existing: &Self, incoming: &Self) -> bool {
        !matches!((existing, incoming), (Self::Real(_), Self::Reference(_)))
    }
}

impl Default for AssetRef {
    fn default() -> Self {
        Self::Real(String::new())
    }
}

impl From<&str> for AssetRef {
    fn from(raw: &str) -> Self {
        Self::decode(raw)
    }
}

impl From<String> for AssetRef {
    fn from(raw: String) -> Self {
        Self::decode(&raw)
    }
}

impl AsRef<str> for AssetRef {
    fn as_ref(&self) -> &str {
        self.as_str()
    }
}

impl fmt::Display for AssetRef {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl PartialEq<str> for AssetRef {
    fn eq(&self, other: &str) -> bool {
        self.as_str() == other
    }
}

impl PartialEq<&str> for AssetRef {
    fn eq(&self, other: &&str) -> bool {
        self.as_str() == *other
    }
}

impl PartialEq<String> for AssetRef {
    fn eq(&self, other: &String) -> bool {
        self.as_str() == other.as_str()
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct AssetRefCensus {
    pub n: usize,
    pub real: usize,
    pub reference: usize,
}

impl AssetRefCensus {
    pub fn push(&mut self, name: &AssetRef) {
        self.n += 1;
        if name.is_real() {
            self.real += 1;
        } else {
            self.reference += 1;
        }
    }
}

static ZONE_INTERN: Mutex<Vec<&'static str>> = Mutex::new(Vec::new());

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct ZoneOwner(&'static str);

impl ZoneOwner {
    pub const UNKNOWN: Self = Self("");

    pub const TEST: Self = Self("test");
    pub const COMMON_MP: Self = Self("common_mp");

    pub fn intern(name: &str) -> Self {
        match name {
            "" => Self::UNKNOWN,
            "test" => Self::TEST,
            "common_mp" => Self::COMMON_MP,
            other => {
                let mut intern = ZONE_INTERN.lock().expect("zone intern");
                if let Some(existing) = intern.iter().copied().find(|&s| s == other) {
                    return Self(existing);
                }
                let leaked: &'static str = Box::leak(other.to_string().into_boxed_str());
                intern.push(leaked);
                Self(leaked)
            }
        }
    }

    pub fn from_zone_path(path: &std::path::Path) -> Self {
        path.file_stem()
            .map(|stem| Self::intern(&stem.to_string_lossy()))
            .unwrap_or(Self::UNKNOWN)
    }

    pub fn as_str(self) -> &'static str {
        self.0
    }

    pub fn is_known(&self) -> bool {
        !self.0.is_empty()
    }
}

impl Default for ZoneOwner {
    fn default() -> Self {
        Self::UNKNOWN
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AssetEdgeReason {
    TempFieldNotAliasable,

    CatalogMiss,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BoundTarget<S: IndexSpace> {
    index: CatalogIndex<S>,
    zone: ZoneOwner,
}

impl<S: IndexSpace> BoundTarget<S> {
    pub fn index(self) -> CatalogIndex<S> {
        self.index
    }

    pub fn zone(self) -> ZoneOwner {
        self.zone
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AssetEdge<S: IndexSpace> {
    Bound(BoundTarget<S>),
    Unresolved(AssetEdgeReason),
    Absent,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct AssetEdgeCensus {
    pub n: usize,
    pub bound: usize,
    pub unresolved: usize,
    pub absent: usize,
}

impl AssetEdgeCensus {
    pub fn push<S: IndexSpace>(&mut self, edge: AssetEdge<S>) {
        self.n += 1;
        match edge {
            AssetEdge::Bound(_) => self.bound += 1,
            AssetEdge::Unresolved(_) => self.unresolved += 1,
            AssetEdge::Absent => self.absent += 1,
        }
    }

    pub fn add_from(&mut self, other: Self) {
        self.n += other.n;
        self.bound += other.bound;
        self.unresolved += other.unresolved;
        self.absent += other.absent;
    }
}

impl<S: IndexSpace> Default for AssetEdge<S> {
    fn default() -> Self {
        Self::Absent
    }
}

impl<S: IndexSpace> AssetEdge<S> {
    pub fn bind(index: CatalogIndex<S>, zone: ZoneOwner) -> Self {
        Self::Bound(BoundTarget { index, zone })
    }

    pub fn bind_order(order: usize, zone: ZoneOwner) -> Self {
        Self::bind(CatalogIndex::from_order(order), zone)
    }

    pub fn bound(&self) -> Option<CatalogIndex<S>> {
        match self {
            Self::Bound(bound) => Some(bound.index),
            Self::Unresolved(_) | Self::Absent => None,
        }
    }

    pub fn bound_index(&self) -> Option<usize> {
        self.bound().map(CatalogIndex::order)
    }

    pub fn space_name() -> &'static str {
        S::NAME
    }

    pub fn bound_zone(&self) -> Option<ZoneOwner> {
        match self {
            Self::Bound(bound) => Some(bound.zone),
            Self::Unresolved(_) | Self::Absent => None,
        }
    }

    pub fn bound_zone_name(&self) -> Option<&'static str> {
        self.bound_zone()
            .filter(|zone| zone.is_known())
            .map(ZoneOwner::as_str)
    }

    pub fn is_bound(&self) -> bool {
        matches!(self, Self::Bound(_))
    }

    pub fn is_unresolved(&self) -> bool {
        matches!(self, Self::Unresolved(_))
    }

    pub fn is_absent(&self) -> bool {
        matches!(self, Self::Absent)
    }

    pub fn edge_kind(&self) -> &'static str {
        match self {
            Self::Bound(_) => "bound",
            Self::Unresolved(AssetEdgeReason::TempFieldNotAliasable) => "unresolved:temp_field",
            Self::Unresolved(AssetEdgeReason::CatalogMiss) => "unresolved:catalog_miss",
            Self::Absent => "absent",
        }
    }

    pub fn report(&self) -> &'static str {
        self.edge_kind()
    }

    pub fn unresolved_presence(has_slot: bool, has_alias: bool) -> Self {
        if !has_slot && !has_alias {
            Self::Absent
        } else if !has_alias {
            Self::Unresolved(AssetEdgeReason::TempFieldNotAliasable)
        } else {
            Self::Unresolved(AssetEdgeReason::CatalogMiss)
        }
    }
}

pub fn bound_zone_names<'a, S: IndexSpace + 'a, I>(edges: I) -> String
where
    I: IntoIterator<Item = &'a AssetEdge<S>>,
{
    let mut names = BTreeSet::new();
    for edge in edges {
        if let Some(name) = edge.bound_zone_name() {
            names.insert(name);
        }
    }
    names.into_iter().collect::<Vec<_>>().join(",")
}
