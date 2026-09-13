use crate::{
    MaterialCatalog, body_catalog::BodyMeshCatalog, fpv_catalog::FpvMeshCatalog,
    fx_catalog::FxCatalog, model_mesh::MapXModelSceneCatalog,
    projectile_mesh_catalog::ProjectileMeshCatalog, sound_catalog::SoundCatalog,
    tracer_catalog::TracerCatalog, weapon_catalog::WeaponCatalog,
    world_weapon_catalog::WorldWeaponCatalog, xanim_catalog::XAnimCatalog,
};
pub use asset_core::{
    AssetEdge, AssetEdgeCensus, AssetEdgeReason, CatalogIndex, FpvMeshIndex, FpvMeshSpace, FxIndex,
    FxModelIndex, FxModelSpace, FxSpace, IndexSpace, LoadedSoundIndex, LoadedSoundSpace,
    MapXModelIndex, MapXModelSpace, MaterialIndex, MaterialSpace, ProjectileModelIndex,
    ProjectileModelSpace, SoundAliasIndex, SoundAliasSpace, TechniqueSetIndex, TechniqueSetSpace,
    TracerIndex, TracerSpace, WorldWeaponIndex, WorldWeaponSpace, XAnimIndex, XAnimSpace,
    ZoneOwner,
};

pub type DeathClipEdge = AssetEdge<XAnimSpace>;

pub type DeathHuskEdge = AssetEdge<MapXModelSpace>;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DestructibleDeathHint {
    pub kind: &'static str,
    pub clip: &'static str,
    pub husk: &'static str,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DestructibleDeathRow {
    pub kind: &'static str,
    pub clip_hint: &'static str,
    pub husk_hint: &'static str,
    pub clip: DeathClipEdge,
    pub husk: DeathHuskEdge,
}

fn edge_from_index<S: IndexSpace>(
    authored: &str,
    index: Option<CatalogIndex<S>>,
    zone: ZoneOwner,
) -> AssetEdge<S> {
    if authored.is_empty() {
        AssetEdge::Absent
    } else if let Some(index) = index {
        AssetEdge::bind(index, zone)
    } else {
        AssetEdge::Unresolved(AssetEdgeReason::CatalogMiss)
    }
}

pub fn stamp_destructible_death_edges(
    xanims: &XAnimCatalog,
    models: &MapXModelSceneCatalog,
    hints: &[DestructibleDeathHint],
) -> Vec<DestructibleDeathRow> {
    hints
        .iter()
        .map(|hint| DestructibleDeathRow {
            kind: hint.kind,
            clip_hint: hint.clip,
            husk_hint: hint.husk,
            clip: {
                let index = xanims
                    .index_by_name(crate::AssetNamespace::Iw4, hint.clip)
                    .map(CatalogIndex::<XAnimSpace>::from_order);
                edge_from_index(
                    hint.clip,
                    index,
                    index.map(|i| xanims.zone_of(i.order())).unwrap_or_default(),
                )
            },
            husk: {
                let index = models
                    .index_by_name(hint.husk)
                    .map(CatalogIndex::<MapXModelSpace>::from_order);
                edge_from_index(
                    hint.husk,
                    index,
                    index.map(|i| models.zone_of(i.order())).unwrap_or_default(),
                )
            },
        })
        .collect()
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct AssetGraphCensus {
    pub tracer_materials: AssetEdgeCensus,
    pub fx_elem_materials: AssetEdgeCensus,
    pub fx_nested_children: AssetEdgeCensus,
    pub fx_runner_children: AssetEdgeCensus,
    pub weapon_tracer_type: AssetEdgeCensus,
    pub xmodel_body_materials: AssetEdgeCensus,
    pub xmodel_gun_materials: AssetEdgeCensus,
    pub xmodel_fpv_materials: AssetEdgeCensus,
    pub weapon_combat_fx: AssetEdgeCensus,
    pub weapon_projectile_fx: AssetEdgeCensus,
    pub weapon_hud_materials: AssetEdgeCensus,
    pub loaded_sounds: AssetEdgeCensus,
}

impl AssetGraphCensus {
    pub fn totals(&self) -> AssetEdgeCensus {
        let mut totals = AssetEdgeCensus::default();
        totals.add_from(self.tracer_materials);
        totals.add_from(self.fx_elem_materials);
        totals.add_from(self.fx_nested_children);
        totals.add_from(self.fx_runner_children);
        totals.add_from(self.weapon_tracer_type);
        totals.add_from(self.xmodel_body_materials);
        totals.add_from(self.xmodel_gun_materials);
        totals.add_from(self.xmodel_fpv_materials);
        totals.add_from(self.weapon_combat_fx);
        totals.add_from(self.weapon_projectile_fx);
        totals.add_from(self.weapon_hud_materials);
        totals.add_from(self.loaded_sounds);
        totals
    }
}

pub fn resolve_after_absorb(
    materials: &MaterialCatalog,
    tracers: &mut TracerCatalog,
    fx: &mut FxCatalog,
    weapons: Option<&mut WeaponCatalog>,
    bodies: Option<&mut BodyMeshCatalog>,
    world_weapons: Option<&mut WorldWeaponCatalog>,
    sounds: Option<&mut SoundCatalog>,
    fpv: Option<&mut FpvMeshCatalog>,
    projectiles: Option<&mut ProjectileMeshCatalog>,
) -> AssetGraphCensus {
    tracers.resolve_materials(materials);
    fx.resolve_materials(materials);
    fx.resolve_nested_edges();
    let mut census = AssetGraphCensus {
        tracer_materials: tracers.material_edge_census(),
        fx_elem_materials: fx.material_edge_census(),
        fx_nested_children: fx.nested_child_edge_census(),
        fx_runner_children: fx.runner_child_edge_census(),
        weapon_tracer_type: AssetEdgeCensus::default(),
        xmodel_body_materials: AssetEdgeCensus::default(),
        xmodel_gun_materials: AssetEdgeCensus::default(),
        xmodel_fpv_materials: AssetEdgeCensus::default(),
        weapon_combat_fx: AssetEdgeCensus::default(),
        weapon_projectile_fx: AssetEdgeCensus::default(),
        weapon_hud_materials: AssetEdgeCensus::default(),
        loaded_sounds: AssetEdgeCensus::default(),
    };
    if let Some(weapons) = weapons {
        weapons.resolve_combat_fx(fx, tracers);
        weapons.resolve_projectile_fx_edges(fx);
        weapons.resolve_reticles(materials);
        census.weapon_tracer_type = weapons.tracer_type_census();
        census.weapon_combat_fx = weapons.combat_fx_census();
        census.weapon_projectile_fx = weapons.projectile_fx_edge_census();
        census.weapon_hud_materials = weapons.hud_material_edge_census();
    }
    if let Some(bodies) = bodies {
        bodies.resolve_materials(materials);
        census.xmodel_body_materials = bodies.material_edge_census();
    }
    if let Some(world_weapons) = world_weapons {
        world_weapons.resolve_materials(materials);
        census.xmodel_gun_materials = world_weapons.material_edge_census();
    }
    if let Some(sounds) = sounds {
        sounds.resolve_loaded_edges();
        census.loaded_sounds = sounds.loaded_edge_census();
    }
    if let Some(fpv) = fpv {
        fpv.resolve_materials(materials);
        census.xmodel_fpv_materials = fpv.material_edge_census();
    }
    if let Some(projectiles) = projectiles {
        projectiles.resolve_materials(materials);
    }
    census
}
