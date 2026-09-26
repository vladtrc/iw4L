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
    realm: super::Realm,
    names: BTreeMap<Namespace, BTreeMap<&'static str, Builtin>>,
}
impl Catalog {
    pub fn iw4() -> Self {
        Self::from_list(super::Realm::Iw4, super::iw4_builtins::IW4)
    }
    pub fn t5() -> Self {
        Self::from_list(super::Realm::T5, super::t5_builtins::T5)
    }
    fn from_list(realm: super::Realm, list: &[Builtin]) -> Self {
        let mut catalog = Self {
            realm,
            names: BTreeMap::new(),
        };
        for builtin in list {
            catalog.insert(builtin.clone());
        }
        catalog
    }
    pub fn realm(&self) -> super::Realm {
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
