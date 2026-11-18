use std::collections::BTreeMap;

use asset_anim::PlayerAnimSources;
use asset_core::FamilyId;
use asset_model::SharedXModelSurfaces;

/// Common products consumed by a map must belong to its captured family.
#[derive(Clone, Default)]
pub(crate) struct MapCommonPayloads {
    surfaces: BTreeMap<FamilyId, SharedXModelSurfaces>,
    animations: BTreeMap<FamilyId, PlayerAnimSources>,
}

impl MapCommonPayloads {
    pub(crate) fn insert_surfaces(&mut self, family: FamilyId, surfaces: SharedXModelSurfaces) {
        self.surfaces.insert(family, surfaces);
    }

    pub(crate) fn insert_animations(&mut self, sources: PlayerAnimSources) {
        if let Some(family) = sources.family() {
            self.animations.insert(family, sources);
        }
    }

    pub(crate) fn take_surfaces(&mut self, family: FamilyId) -> SharedXModelSurfaces {
        self.surfaces
            .remove(&family)
            .unwrap_or_else(SharedXModelSurfaces::default)
    }

    pub(crate) fn take_animations(&mut self, family: FamilyId) -> PlayerAnimSources {
        self.animations
            .remove(&family)
            .unwrap_or_else(PlayerAnimSources::default)
    }
}
