use std::collections::{BTreeMap, BTreeSet, btree_map};
use std::ops::{Deref, DerefMut};
use std::sync::Arc;

pub struct ScriptTable<V> {
    rows: BTreeMap<u64, V>,
    changed: BTreeSet<u64>,
    all_changed: bool,
    identity: Arc<()>,
}

impl<V> ScriptTable<V> {
    pub fn get_mut(&mut self, id: &u64) -> Option<&mut V> {
        self.mark(*id);
        self.rows.get_mut(id)
    }
    pub fn entry(&mut self, id: u64) -> btree_map::Entry<'_, u64, V> {
        self.mark(id);
        self.rows.entry(id)
    }
    pub fn insert(&mut self, id: u64, value: V) -> Option<V> {
        self.mark(id);
        self.rows.insert(id, value)
    }
    pub fn remove(&mut self, id: &u64) -> Option<V> {
        self.mark(*id);
        self.rows.remove(id)
    }
    fn mark(&mut self, id: u64) {
        if !self.all_changed {
            self.changed.insert(id);
        }
    }
    pub(crate) fn changed(&self) -> Option<&BTreeSet<u64>> {
        (!self.all_changed).then_some(&self.changed)
    }
    pub(crate) fn identity(&self) -> &Arc<()> {
        &self.identity
    }
    pub(crate) fn acknowledge(&mut self) {
        self.changed.clear();
        self.all_changed = false;
    }
}
impl<V> Default for ScriptTable<V> {
    fn default() -> Self {
        BTreeMap::new().into()
    }
}
impl<V: Clone> Clone for ScriptTable<V> {
    fn clone(&self) -> Self {
        self.rows.clone().into()
    }
}
impl<V> Deref for ScriptTable<V> {
    type Target = BTreeMap<u64, V>;
    fn deref(&self) -> &Self::Target {
        &self.rows
    }
}
impl<V> DerefMut for ScriptTable<V> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        self.all_changed = true;
        self.changed.clear();
        &mut self.rows
    }
}
impl<'a, V> IntoIterator for &'a ScriptTable<V> {
    type Item = (&'a u64, &'a V);
    type IntoIter = btree_map::Iter<'a, u64, V>;
    fn into_iter(self) -> Self::IntoIter {
        self.rows.iter()
    }
}
impl<'a, V> IntoIterator for &'a mut ScriptTable<V> {
    type Item = (&'a u64, &'a mut V);
    type IntoIter = btree_map::IterMut<'a, u64, V>;
    fn into_iter(self) -> Self::IntoIter {
        self.deref_mut().iter_mut()
    }
}
impl<V> IntoIterator for ScriptTable<V> {
    type Item = (u64, V);
    type IntoIter = btree_map::IntoIter<u64, V>;
    fn into_iter(self) -> Self::IntoIter {
        self.rows.into_iter()
    }
}
impl<V> From<BTreeMap<u64, V>> for ScriptTable<V> {
    fn from(rows: BTreeMap<u64, V>) -> Self {
        Self {
            rows,
            changed: BTreeSet::new(),
            all_changed: true,
            identity: Arc::new(()),
        }
    }
}
impl<V> FromIterator<(u64, V)> for ScriptTable<V> {
    fn from_iter<T: IntoIterator<Item = (u64, V)>>(iter: T) -> Self {
        BTreeMap::from_iter(iter).into()
    }
}
impl<V: std::fmt::Debug> std::fmt::Debug for ScriptTable<V> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.rows.fmt(f)
    }
}
