use std::collections::BTreeMap;
use std::ops::{Deref, DerefMut};
use std::sync::Arc;

use crate::script::{ArrayKey, Value};

#[derive(Clone, Default)]
pub(crate) struct ArrayValues(Arc<BTreeMap<ArrayKey, Value>>);

impl Deref for ArrayValues {
    type Target = BTreeMap<ArrayKey, Value>;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl DerefMut for ArrayValues {
    fn deref_mut(&mut self) -> &mut Self::Target {
        Arc::make_mut(&mut self.0)
    }
}

impl From<BTreeMap<ArrayKey, Value>> for ArrayValues {
    fn from(values: BTreeMap<ArrayKey, Value>) -> Self {
        Self(Arc::new(values))
    }
}

impl FromIterator<(ArrayKey, Value)> for ArrayValues {
    fn from_iter<T: IntoIterator<Item = (ArrayKey, Value)>>(iter: T) -> Self {
        BTreeMap::from_iter(iter).into()
    }
}

impl std::fmt::Debug for ArrayValues {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(formatter)
    }
}
