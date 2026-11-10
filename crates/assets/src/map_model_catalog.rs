use asset_material::AssetPointerIdentity;
use asset_world::{
    MapXModelAssetKey, MapXModelSceneAsset, MapXModelSceneCatalog, ModelMesh, PreparedMapModels,
    ScriptModelMetadata, ScriptModelPlacement, ScriptModelSceneInstance, StaticModelDraw,
    StaticModelDrawError, StaticModelInstance, StaticModelPlacement,
};
use bevy::prelude::{Mat3, Quat, Transform, Vec3};
use std::collections::HashMap;

#[derive(Default)]
pub struct MapXModelCatalog {
    pub shared_surfaces: asset_model::SharedXModelSurfaces,
    meshes: Vec<ModelMesh>,
    direct: HashMap<AssetPointerIdentity, usize>,
    aliases: HashMap<AssetPointerIdentity, AssetPointerIdentity>,
    failed: usize,
    scene_assets: MapXModelSceneCatalog,

    t5_destructibles: HashMap<String, Result<T5DestructibleInitial, &'static str>>,
}
pub struct T5DestructibleInitial {
    pub definition: std::sync::Arc<xmodel_runtime::T5DestructibleDef>,
    pub model: MapXModelAssetKey,
    pub hide_parts: xmodel_runtime::HidePartBits,
}
impl MapXModelCatalog {
    pub fn take_named_mesh(&mut self, name: &str) -> Option<ModelMesh> {
        let index = self.meshes.iter().position(|mesh| mesh.name == name)?;
        self.direct.retain(|_, slot| *slot != index);
        for slot in self.direct.values_mut() {
            if *slot > index {
                *slot -= 1;
            }
        }
        Some(self.meshes.remove(index))
    }
    pub fn set_capture_zone(&mut self, zone: asset_core::ZoneOwner) {
        self.scene_assets.set_capture_zone(zone);
    }
    pub fn alias(
        &mut self,
        slot: impl Into<AssetPointerIdentity>,
        target: impl Into<AssetPointerIdentity>,
    ) {
        self.aliases.insert(slot.into(), target.into());
    }
    pub fn mesh_index(&self, slot: impl Into<AssetPointerIdentity>) -> Option<usize> {
        let mut current = slot.into();
        for _ in 0..self.aliases.len().saturating_add(1) {
            if let Some(&index) = self.direct.get(&current) {
                return Some(index);
            }
            current = *self.aliases.get(&current)?;
        }
        None
    }
    pub fn name_at_slot(&self, slot: impl Into<AssetPointerIdentity>) -> Option<&str> {
        let index = self.mesh_index(slot)?;
        Some(self.meshes[index].name.as_str())
    }
    pub fn phys_preset_slot_n(&self) -> usize {
        self.scene_assets.phys_preset_slot_n()
    }
    pub fn phys_preset_name_hint_n(&self) -> usize {
        self.scene_assets.phys_preset_name_hint_n()
    }
    pub fn captured_model_n(&self) -> usize {
        self.scene_assets.len()
    }
    pub fn set_dynent_phys_preset_n(&mut self, n: usize) {
        self.scene_assets.set_dynent_phys_preset_n(n);
    }
    pub fn note_xmodel_phys_preset(&mut self, live: bool, named: bool) {
        self.scene_assets.note_xmodel_phys_preset(live, named);
    }
    pub fn note_failed_mesh(&mut self) {
        self.failed += 1;
    }
    pub fn insert_scene_asset(&mut self, key: MapXModelAssetKey, asset: MapXModelSceneAsset) {
        self.scene_assets.insert(key, asset);
    }
    pub fn scene_asset(&self, name: &str) -> Option<&MapXModelSceneAsset> {
        self.scene_assets.get_name(name)
    }
    pub fn insert_mesh(
        &mut self,
        mesh: ModelMesh,
        slot: impl Into<AssetPointerIdentity>,
        insert_slot: Option<AssetPointerIdentity>,
    ) {
        let index = self.meshes.len();
        self.meshes.push(mesh);
        self.direct.insert(slot.into(), index);
        if let Some(slot) = insert_slot {
            self.direct.insert(slot, index);
        }
    }
    pub fn insert_destructible_initial(
        &mut self,
        name: String,
        initial: Result<T5DestructibleInitial, &'static str>,
    ) {
        self.t5_destructibles.insert(name, initial);
    }
}
pub fn link_model_placements(
    placements: Result<Vec<StaticModelInstance>, StaticModelDrawError>,
    scripts: &[ScriptModelPlacement],
    script_brush_models: Vec<asset_world::ScriptBrushModelPlacement>,
    flag_descriptors: Vec<asset_world::FlagDescriptor>,
    script_structs: Vec<asset_world::MapScriptStruct>,
    mut catalog: MapXModelCatalog,
    smodel_count: usize,
) -> PreparedMapModels {
    let (placements, mut static_error) = match placements {
        Ok(placements) => (placements, None),
        Err(error) => (Vec::new(), Some(error)),
    };
    if static_error.is_none() && placements.len() != smodel_count {
        static_error = Some(StaticModelDrawError::PlacementCountMismatch {
            decoded: placements.len(),
            smodel_count,
        });
    }
    let mut remap = vec![None; catalog.meshes.len()];
    let mut used = Vec::new();
    let mut gaps = catalog.failed;
    let mut instances: Vec<Option<StaticModelPlacement>> = vec![None; smodel_count];
    let mut script_instances = Vec::with_capacity(scripts.len());

    let mut take_mesh = |source_mesh: usize| -> usize {
        match remap[source_mesh] {
            Some(mesh) => mesh,
            None => {
                let mesh = used.len();
                remap[source_mesh] = Some(mesh);
                used.push(source_mesh);
                mesh
            }
        }
    };

    if static_error.is_none() {
        for (slot, placement) in placements.into_iter().enumerate() {
            let Some(source_mesh) = catalog.mesh_index(placement.model_slot) else {
                gaps += 1;
                continue;
            };
            let mesh = take_mesh(source_mesh);
            let basis = Mat3::from_cols(
                Vec3::from_array(placement.axis[0]),
                Vec3::from_array(placement.axis[1]),
                Vec3::from_array(placement.axis[2]),
            );
            instances[slot] = Some(StaticModelPlacement {
                mesh,
                transform: Transform {
                    translation: Vec3::from_array(placement.origin),
                    rotation: Quat::from_mat3(&basis),
                    scale: Vec3::splat(placement.scale),
                },
                origin: placement.origin,
                axis: placement.axis,
                scale: placement.scale,
                cull_dist: placement.cull_dist,
                reflection_probe_index: placement.reflection_probe_index,
                primary_light_index: placement.primary_light_index,
                flags: placement.flags,
            });
        }
    } else {
        gaps = gaps.saturating_add(smodel_count);
    }

    let mut script_gaps = 0usize;
    for placement in scripts {
        let initial = catalog.t5_destructibles.get(&placement.destructible_def);
        let current_model = match initial {
            Some(Ok(initial)) => initial.model.clone(),
            _ if !placement.destructible_def.is_empty() => {
                MapXModelAssetKey(placement.destructible_def.clone())
            }
            _ => MapXModelAssetKey(placement.model.clone()),
        };
        if catalog.scene_assets.get(&current_model).is_none() {
            script_gaps += 1;
            catalog.scene_assets.insert(
                current_model.clone(),
                MapXModelSceneAsset::Unavailable {
                    reason: "map XModel was not captured as a scene asset",
                },
            );
        }
        let mut dobj_state =
            xmodel_runtime::DObjSemanticState::bind_pose(current_model.0.clone(), 1, 1);
        match initial {
            Some(Ok(initial)) => dobj_state.hide_part_bits = initial.hide_parts,
            Some(Err(reason)) => {
                catalog.scene_assets.insert(
                    current_model.clone(),
                    MapXModelSceneAsset::Unavailable { reason },
                );
            }
            None if !placement.destructible_def.is_empty() => {
                catalog.scene_assets.insert(
                    current_model.clone(),
                    MapXModelSceneAsset::Unavailable {
                        reason: "T5 MapEnts DestructibleDef was not captured",
                    },
                );
            }
            None => {}
        }
        let [pitch, yaw, roll] = placement.angles.map(f32::to_radians);
        script_instances.push(ScriptModelSceneInstance {
            id: placement.id,
            dobj_state,
            current_model,
            transform: Transform {
                translation: Vec3::from_array(placement.origin),
                rotation: Quat::from_euler(bevy::math::EulerRot::ZYX, yaw, pitch, roll),
                scale: Vec3::ONE,
            },
            lighting_origin: placement.lighting_origin.unwrap_or(placement.origin),
            metadata: ScriptModelMetadata {
                gameobject: placement.gameobject.clone(),
                targetname: placement.targetname.clone(),
                script_noteworthy: placement.script_noteworthy.clone(),
                destructible_type: placement.destructible_type.clone(),
                destructible_def: placement.destructible_def.clone(),
                t5_destructible: initial
                    .and_then(|row| row.as_ref().ok())
                    .map(|row| row.definition.clone()),
                target: placement.target.clone(),
                script_exploder: placement.script_exploder.clone(),
                brush_link: placement.brush_link.clone(),
                script_accumulate: placement.script_accumulate,
                script_threshold: placement.script_threshold,
                script_destructable_area: placement.script_destructable_area.clone(),
                script_fxid: placement.script_fxid.clone(),
            },
        });
    }

    let mut meshes = catalog.meshes.into_iter().map(Some).collect::<Vec<_>>();
    let meshes = used
        .into_iter()
        .filter_map(|index| meshes.get_mut(index).and_then(Option::take))
        .collect();
    PreparedMapModels {
        static_draw: StaticModelDraw {
            meshes,
            placements: instances,
            gaps,
        },
        static_error,
        scene_assets: catalog.scene_assets,
        script_instances,
        script_brush_models,
        flag_descriptors,
        script_structs,
        script_gaps,
    }
}

pub fn normalize_bomb_sites(
    scripts: &mut [asset_world::ScriptModelPlacement],
    brushes: &mut [asset_world::ScriptBrushModelPlacement],
    triggers: &[asset_world::MapUseTrigger],
    demolition_tag: &str,
) {
    let has_dedicated_sites = ["a", "b"].into_iter().all(|label| {
        triggers.iter().any(|trigger| {
            trigger.targetname == demolition_tag
                && trigger.script_label.trim_start_matches('_') == label
        })
    });
    let normalize = |value: &mut String| {
        *value = value
            .split_ascii_whitespace()
            .map(|token| {
                if token == demolition_tag {
                    "bombzone"
                } else if token == "bombzone" && has_dedicated_sites {
                    "sd_bombzone"
                } else {
                    token
                }
            })
            .collect::<Vec<_>>()
            .join(" ");
    };
    for script in scripts {
        normalize(&mut script.gameobject);
    }
    for brush in brushes {
        normalize(&mut brush.gameobject);
    }
}
