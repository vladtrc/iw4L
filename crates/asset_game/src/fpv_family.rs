use crate::FpvMeshIndex;
use asset_core::FamilyId;
use asset_model::{FpvMeshCatalog, FpvMeshHandle};
#[derive(Clone, Copy, Debug)]
pub struct FpvFamilyConnection {
    gun: FpvMeshHandle,
    hands: FpvMeshHandle,
}
impl FpvFamilyConnection {
    pub fn connect(gun: FpvMeshHandle, hands: FpvMeshHandle) -> Option<Self> {
        (gun.owner() != 0 && gun.owner() == hands.owner()).then_some(Self { gun, hands })
    }
    pub(crate) fn bind(
        catalog: &FpvMeshCatalog,
        gun: FpvMeshIndex,
        hands: FpvMeshIndex,
    ) -> Option<Self> {
        let gun_family = catalog.get_at(gun.order())?.namespace;
        let hands_family = catalog.get_at(hands.order())?.namespace;
        Self::connect(
            catalog.family_mesh(gun, gun_family)?,
            catalog.family_mesh(hands, hands_family)?,
        )
    }
    pub fn family(self) -> FamilyId {
        self.gun.family()
    }
    pub fn gun(self) -> FpvMeshIndex {
        self.gun.index()
    }
    pub fn hands(self) -> FpvMeshIndex {
        self.hands.index()
    }
}
