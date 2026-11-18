use std::any::{Any, TypeId};
use std::collections::BTreeMap;
use std::fmt::Debug;

trait StoredValue: Debug + Send + Sync {
    fn clone_value(&self) -> Box<dyn StoredValue>;
    fn as_any(&self) -> &dyn Any;
    fn as_any_mut(&mut self) -> &mut dyn Any;
    fn into_any(self: Box<Self>) -> Box<dyn Any>;
}
impl<T: Clone + Debug + Send + Sync + 'static> StoredValue for T {
    fn clone_value(&self) -> Box<dyn StoredValue> {
        Box::new(self.clone())
    }
    fn as_any(&self) -> &dyn Any {
        self
    }
    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }
    fn into_any(self: Box<Self>) -> Box<dyn Any> {
        self
    }
}
#[derive(Debug, Default)]
pub struct OwnedExtensions(BTreeMap<TypeId, Box<dyn StoredValue>>);
impl Clone for OwnedExtensions {
    fn clone(&self) -> Self {
        Self(
            self.0
                .iter()
                .map(|(key, value)| (*key, value.as_ref().clone_value()))
                .collect(),
        )
    }
}
impl OwnedExtensions {
    pub fn insert<T: Clone + Debug + Send + Sync + 'static>(&mut self, value: T) {
        self.0.insert(TypeId::of::<T>(), Box::new(value));
    }
    pub fn get<T: 'static>(&self) -> Option<&T> {
        self.0
            .get(&TypeId::of::<T>())?
            .as_ref()
            .as_any()
            .downcast_ref()
    }
    pub fn get_mut<T: 'static>(&mut self) -> Option<&mut T> {
        self.0
            .get_mut(&TypeId::of::<T>())?
            .as_mut()
            .as_any_mut()
            .downcast_mut()
    }
    pub fn remove<T: 'static>(&mut self) -> Option<T> {
        self.0
            .remove(&TypeId::of::<T>())?
            .into_any()
            .downcast::<T>()
            .ok()
            .map(|value| *value)
    }
}
