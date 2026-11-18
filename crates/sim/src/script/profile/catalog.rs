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
    Actor,
    Sentient,
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
    realm: crate::script::FamilyId,
    names: BTreeMap<Namespace, BTreeMap<&'static str, Builtin>>,
}
impl Catalog {
    pub fn iw4() -> Self {
        Self::from_list(crate::script::FamilyId::IW4, super::iw4_catalog::IW4)
    }
    pub fn iw4_campaign() -> Self {
        Self::from_list(
            crate::script::FamilyId::IW4,
            super::iw4_campaign_catalog::IW4_CAMPAIGN,
        )
    }
    pub fn from_list(realm: crate::script::FamilyId, list: &[Builtin]) -> Self {
        let mut catalog = Self {
            realm,
            names: BTreeMap::new(),
        };
        for builtin in list {
            catalog.insert(builtin.clone());
        }
        catalog
    }
    pub fn realm(&self) -> crate::script::FamilyId {
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
