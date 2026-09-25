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
    Probe,
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

/// The builtin names a program may link against. Linking is decided by the catalog;
/// binding an implementation happens at install.
#[derive(Clone, Debug)]
pub struct Catalog(BTreeMap<Namespace, BTreeMap<&'static str, Builtin>>);
impl Catalog {
    pub fn iw4() -> Self {
        let mut catalog = Self(BTreeMap::new());
        for builtin in super::iw4_builtins::IW4 {
            catalog.insert(builtin.clone());
        }
        catalog
    }
    fn insert(&mut self, builtin: Builtin) {
        self.0
            .entry(builtin.namespace)
            .or_default()
            .insert(builtin.name, builtin);
    }
    /// Adds a name outside the engine ABI; for probes only.
    pub fn with(mut self, namespace: Namespace, name: &'static str) -> Self {
        self.insert(Builtin::new(namespace, name, Owner::Probe, false));
        self
    }
    pub fn get(&self, namespace: Namespace, name: &str) -> Option<&Builtin> {
        self.0.get(&namespace)?.get(name)
    }
    pub fn iter(&self) -> impl Iterator<Item = &Builtin> {
        self.0.values().flat_map(BTreeMap::values)
    }
}
