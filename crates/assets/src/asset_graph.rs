use crate::{
    MaterialDefinitions, body_catalog::BodyMeshBuild, fpv_catalog::FpvMeshBuild,
    fx_catalog::FxCatalog, model_mesh::MapXModelSceneCatalog,
    projectile_mesh_catalog::ProjectileMeshBuild, sound_catalog::SoundCatalog,
    tracer_catalog::TracerCatalog, weapon_catalog::WeaponCatalog,
    world_weapon_catalog::WorldWeaponBuild, xanim_catalog::XAnimCatalog,
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

/// GSC death clip/husk names the match graph binds at Ready.
pub const DESTRUCTIBLE_DEATH_HINTS: &[DestructibleDeathHint] = &[
    DestructibleDeathHint {
        kind: "vehicle_pickup",
        clip: "vehicle_80s_sedan1_destroy",
        husk: "vehicle_pickup_destroyed",
    },
    DestructibleDeathHint {
        kind: "vehicle_moving_truck",
        clip: "vehicle_80s_sedan1_destroy",
        husk: "vehicle_moving_truck_dst",
    },
    DestructibleDeathHint {
        kind: "vehicle_policecar",
        clip: "vehicle_80s_sedan1_destroy",
        husk: "vehicle_policecar_lapd_destroy",
    },
    DestructibleDeathHint {
        kind: "explodable_barrel",
        clip: "",
        husk: "com_barrel_piece",
    },
    DestructibleDeathHint {
        kind: "flammable_crate",
        clip: "",
        husk: "global_flammable_crate_jap_piece01_d",
    },
    DestructibleDeathHint {
        kind: "toy_oxygen_tank_01",
        clip: "",
        husk: "machinery_oxygen_tank01_des",
    },
    DestructibleDeathHint {
        kind: "toy_oxygen_tank_02",
        clip: "",
        husk: "machinery_oxygen_tank02_des",
    },
    DestructibleDeathHint {
        kind: "toy_propane_tank02",
        clip: "",
        husk: "com_propane_tank02_DES",
    },
    DestructibleDeathHint {
        kind: "toy_propane_tank02_small",
        clip: "",
        husk: "com_propane_tank02_small_DES",
    },
    DestructibleDeathHint {
        kind: "toy_tubetv_tv1",
        clip: "",
        husk: "com_tv1_d",
    },
    DestructibleDeathHint {
        kind: "toy_tubetv_tv2",
        clip: "",
        husk: "com_tv2_d",
    },
    DestructibleDeathHint {
        kind: "toy_tv_flatscreen_01",
        clip: "",
        husk: "ma_flatscreen_tv_broken_01",
    },
    DestructibleDeathHint {
        kind: "toy_tv_flatscreen_02",
        clip: "",
        husk: "ma_flatscreen_tv_broken_02",
    },
    DestructibleDeathHint {
        kind: "toy_tv_flatscreen_wallmount_01",
        clip: "",
        husk: "ma_flatscreen_tv_wallmount_broken_01",
    },
    DestructibleDeathHint {
        kind: "toy_tv_flatscreen_wallmount_02",
        clip: "",
        husk: "ma_flatscreen_tv_wallmount_broken_02",
    },
    DestructibleDeathHint {
        kind: "toy_light_ceiling_fluorescent",
        clip: "",
        husk: "me_lightfluohang_double_destroyed",
    },
    DestructibleDeathHint {
        kind: "toy_light_ceiling_fluorescent_single",
        clip: "",
        husk: "me_lightfluohang_single_destroyed",
    },
    DestructibleDeathHint {
        kind: "toy_electricbox2",
        clip: "",
        husk: "me_electricbox2_dest",
    },
    DestructibleDeathHint {
        kind: "toy_electricbox4",
        clip: "",
        husk: "me_electricbox4_dest",
    },
    DestructibleDeathHint {
        kind: "toy_airconditioner",
        clip: "",
        husk: "com_ex_airconditioner_dam",
    },
    DestructibleDeathHint {
        kind: "toy_wall_fan",
        clip: "",
        husk: "cs_wallfan1_dmg",
    },
    DestructibleDeathHint {
        kind: "toy_locker_double",
        clip: "",
        husk: "com_locker_double_destroyed",
    },
    DestructibleDeathHint {
        kind: "toy_filecabinet",
        clip: "",
        husk: "com_filecabinetblackclosed_dam",
    },
    DestructibleDeathHint {
        kind: "toy_usa_gas_station_trash_bin_01",
        clip: "",
        husk: "usa_gas_station_trash_bin_01_base",
    },
    DestructibleDeathHint {
        kind: "toy_ceiling_fan",
        clip: "",
        husk: "me_fanceil1_des",
    },
    DestructibleDeathHint {
        kind: "toy_trashbin_01",
        clip: "",
        husk: "com_trashbin01_dmg",
    },
    DestructibleDeathHint {
        kind: "toy_trashbin_02",
        clip: "",
        husk: "com_trashbin02_dmg",
    },
    DestructibleDeathHint {
        kind: "toy_transformer_small01",
        clip: "",
        husk: "utility_transformer_small01_dest",
    },
    DestructibleDeathHint {
        kind: "toy_water_collector",
        clip: "",
        husk: "utility_water_collector_base_dest",
    },
    DestructibleDeathHint {
        kind: "toy_newspaper_stand_red",
        clip: "",
        husk: "com_newspaperbox_red_dam",
    },
    DestructibleDeathHint {
        kind: "toy_newspaper_stand_blue",
        clip: "",
        husk: "com_newspaperbox_blue_dam",
    },
    DestructibleDeathHint {
        kind: "toy_chicken_black_white",
        clip: "",
        husk: "chicken_black_white",
    },
    DestructibleDeathHint {
        kind: "toy_chicken_white",
        clip: "",
        husk: "chicken_white",
    },
    DestructibleDeathHint {
        kind: "toy_firehydrant",
        clip: "",
        husk: "com_firehydrant_dest",
    },
    DestructibleDeathHint {
        kind: "toy_usa_gas_station_trash_bin_02",
        clip: "",
        husk: "usa_gas_station_trash_bin_02_base",
    },
    DestructibleDeathHint {
        kind: "toy_transformer_ratnest01",
        clip: "",
        husk: "utility_transformer_ratnest01_dest",
    },
    DestructibleDeathHint {
        kind: "toy_copier",
        clip: "",
        husk: "prop_photocopier_destroyed",
    },
    DestructibleDeathHint {
        kind: "toy_generator",
        clip: "",
        husk: "machinery_generator_des",
    },
    DestructibleDeathHint {
        kind: "toy_generator_on",
        clip: "",
        husk: "machinery_generator_des",
    },
    DestructibleDeathHint {
        kind: "toy_dt_mirror_large",
        clip: "",
        husk: "dt_mirror_large_des",
    },
    DestructibleDeathHint {
        kind: "toy_dt_mirror",
        clip: "",
        husk: "dt_mirror_des",
    },
];

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

pub(crate) fn stamp_destructible_death_edges(
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
    materials: &MaterialDefinitions,
    tracers: &mut TracerCatalog,
    fx: &mut FxCatalog,
    weapons: Option<&mut WeaponCatalog>,
    bodies: Option<&mut BodyMeshBuild>,
    world_weapons: Option<&mut WorldWeaponBuild>,
    sounds: Option<&mut SoundCatalog>,
    fpv: Option<&mut FpvMeshBuild>,
    projectiles: Option<&mut ProjectileMeshBuild>,
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
        weapons.resolve_reticle_images(materials);
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

pub(crate) fn stamp_match_destructible_death(
    xanims: &XAnimCatalog,
    models: &MapXModelSceneCatalog,
) -> Vec<DestructibleDeathRow> {
    stamp_destructible_death_edges(xanims, models, &DESTRUCTIBLE_DEATH_HINTS)
}
