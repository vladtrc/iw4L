use super::Value;
use crate::extensions::OwnedExtensions;
use std::any::TypeId;
use std::collections::BTreeMap;
use std::fmt::Debug;

pub trait RoundState: Default + Clone + Debug + Send + Sync + 'static {
    fn trace_values(&self, pending: &mut Vec<Value>);
}
type Trace = fn(&OwnedExtensions, &mut Vec<Value>);
#[derive(Clone, Debug, Default)]
pub struct RoundExtensions {
    values: OwnedExtensions,
    tracers: BTreeMap<TypeId, Trace>,
}
impl RoundExtensions {
    pub fn state<T: RoundState>(&self) -> &T {
        self.values
            .get::<T>()
            .expect("script family must initialize round state")
    }
    pub fn state_mut<T: RoundState>(&mut self) -> &mut T {
        if self.values.get::<T>().is_none() {
            self.values.insert(T::default());
            self.tracers.insert(TypeId::of::<T>(), |values, pending| {
                values.get::<T>().unwrap().trace_values(pending)
            });
        }
        self.values.get_mut::<T>().unwrap()
    }
    pub fn trace_values(&self, pending: &mut Vec<Value>) {
        for trace in self.tracers.values() {
            trace(&self.values, pending);
        }
    }
}
