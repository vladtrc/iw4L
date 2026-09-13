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
