use bevy::prelude::Resource;

use crate::{
    asset_graph::DestructibleDeathRow, body_catalog::BodyMeshCatalog, fpv_catalog::FpvMeshCatalog,
    weapon_catalog::WeaponRegistry, world_weapon_catalog::WorldWeaponCatalog,
    xanim_catalog::XAnimCatalog,
};

/// The one material population a prepared match owns, and the map-zone-local
/// index space that resolves into it.
///
/// Geometry keeps references — local material indices and, after the merge,
/// nothing else. The pool itself is never nested inside an optional product.
#[derive(Default)]
pub struct MatchMaterials {
    pub population: crate::MaterialDefinitions,

    /// map-zone-local material index -> row in `population`
    pub map_ids: Vec<Option<usize>>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct PreparedGaps {
    pub lines: Vec<String>,
}

/// What the map itself declares about the match, captured once by the lane and
/// moved from there to its single owner in [`PreparedMap`].
#[derive(Clone, Debug, Default, PartialEq)]
pub struct MapFacts {
    pub minimap_corners: Option<crate::MinimapCorners>,

    pub north_yaw: Option<f32>,

    pub compass: crate::MapCompassDeclaration,

    pub script_sound: crate::MapScriptSoundFacts,

    pub team_icons: crate::TeamIcons,

    pub t5_teamset: Option<String>,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct PreparedMap {
    pub zone: String,

    pub namespace: Option<crate::AssetNamespace>,
    pub spawns: Vec<crate::SpawnPoint>,

    pub facts: MapFacts,
    pub gaps: PreparedGaps,
}

#[derive(Clone, Debug, Default, Resource)]
pub struct PreparedWeapons(pub WeaponRegistry);

#[derive(Clone, Debug, Default, Resource)]
pub struct MatchType10SoundHints(pub Vec<String>);

#[derive(Clone, Debug, Default, Resource)]
pub struct PreparedFpvMeshes(pub FpvMeshCatalog);

#[derive(Clone, Debug, Default, Resource)]
pub struct PreparedBodies(pub BodyMeshCatalog);

#[derive(Clone, Debug, Default, Resource)]
pub struct PreparedWorldWeapons(pub WorldWeaponCatalog);

#[derive(Clone, Debug, Default, Resource)]
pub struct PreparedProjectileMeshes(pub crate::ProjectileMeshCatalog);

#[derive(Clone, Debug, Default, Resource)]
pub struct PreparedXModelWalkCensus {
    pub walked_n: usize,

    pub unclassified_n: usize,

    pub projectile_names: Vec<String>,
}

impl PreparedXModelWalkCensus {
    pub fn projectile_walked_n(&self) -> usize {
        self.projectile_names.len()
    }

    pub fn projectile_names_csv(&self) -> String {
        self.projectile_names.join(",")
    }

    pub fn report_line(&self, source: &str) -> String {
        format!(
            "{source} XModels: {} unique names, {} unclassified (model_kind None); projectile_* walked: {} ({})",
            self.walked_n,
            self.unclassified_n,
            self.projectile_walked_n(),
            if self.projectile_names.is_empty() {
                "none".to_owned()
            } else {
                self.projectile_names_csv()
            }
        )
    }
}

#[derive(Clone, Debug, Default, Resource)]
pub struct PreparedXAnims(pub XAnimCatalog);

#[derive(Clone, Debug, Default, Resource)]
pub struct PreparedDestructibleDeath(pub Vec<DestructibleDeathRow>);

#[derive(Clone, Debug, Default, Resource)]
pub struct PreparedLocalizedStrings(pub crate::LocalizeCatalog);

#[derive(Clone, Debug, Default, Resource)]
pub struct SessionCompass {
    pub corners: Option<crate::MinimapCorners>,

    pub north_yaw: Option<f32>,

    pub declaration: crate::MapCompassDeclaration,
}

#[derive(Clone, Copy, Debug, Default, Resource, PartialEq, Eq)]
pub struct PreparedBodyClips(pub bool);
