use asset_core::{AssetEdge, IndexSpace};
use fastfile_iw4::Ptr;

pub(crate) trait AssetEdgeFromPtrs<S: IndexSpace> {
    fn unresolved_for(slot: Option<Ptr>, alias: Option<Ptr>) -> AssetEdge<S>;
    fn from_capture(slot: Option<Ptr>, alias: Option<Ptr>) -> AssetEdge<S>;
}

impl<S: IndexSpace> AssetEdgeFromPtrs<S> for AssetEdge<S> {
    fn unresolved_for(slot: Option<Ptr>, alias: Option<Ptr>) -> AssetEdge<S> {
        AssetEdge::unresolved_presence(slot.is_some(), alias.is_some())
    }

    fn from_capture(slot: Option<Ptr>, alias: Option<Ptr>) -> AssetEdge<S> {
        Self::unresolved_for(slot, alias)
    }
}

/// What the walk found for one asset reference, once the zone it found it in is
/// closed. `unresolved_presence` is the only thing the pointers were ever asked
/// for after the walk, so this is what travels instead of them.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct AuthoredRef {
    pub slot: bool,
    pub alias: bool,
}

impl AuthoredRef {
    pub(crate) fn from_ptrs(slot: Option<Ptr>, alias: Option<Ptr>) -> Self {
        Self {
            slot: slot.is_some(),
            alias: alias.is_some(),
        }
    }

    pub(crate) fn unresolved<S: IndexSpace>(self) -> AssetEdge<S> {
        AssetEdge::unresolved_presence(self.slot, self.alias)
    }
}
