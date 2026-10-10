use std::collections::BTreeMap;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Namespace {
    Function,
    Method,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Owner {
    Script,
    Player,
    Entity,
    HudElem,
    ScriptMover,
    PlayerCommand,
    Helicopter,
    Vehicle,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Builtin {
    pub namespace: Namespace,
    pub name: &'static str,
    pub owner: Owner,
    pub developer: bool,
}
impl Builtin {
    pub const fn new(
        namespace: Namespace,
        name: &'static str,
        owner: Owner,
        developer: bool,
    ) -> Self {
        Self {
            namespace,
            name,
            owner,
            developer,
        }
    }
}

#[derive(Clone, Debug)]
pub struct Catalog {
    realm: asset_core::FamilyId,
    names: BTreeMap<Namespace, BTreeMap<&'static str, Builtin>>,
}
impl Catalog {
    pub fn from_list(realm: asset_core::FamilyId, list: &[Builtin]) -> Self {
        let mut catalog = Self {
            realm,
            names: BTreeMap::new(),
        };
        for builtin in list {
            catalog.insert(builtin.clone());
        }
        catalog
    }
    pub fn extended(mut self, extra: impl IntoIterator<Item = Builtin>) -> Self {
        for builtin in extra {
            self.insert(builtin);
        }
        self
    }
    pub fn realm(&self) -> asset_core::FamilyId {
        self.realm
    }
    fn insert(&mut self, builtin: Builtin) {
        self.names
            .entry(builtin.namespace)
            .or_default()
            .insert(builtin.name, builtin);
    }
    pub fn get(&self, namespace: Namespace, name: &str) -> Option<&Builtin> {
        self.names.get(&namespace)?.get(name)
    }
    pub fn iter(&self) -> impl Iterator<Item = &Builtin> {
        self.names.values().flat_map(BTreeMap::values)
    }
}
