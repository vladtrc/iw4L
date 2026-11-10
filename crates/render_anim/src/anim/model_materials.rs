use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use bevy::prelude::*;
use render_material::RuntimeMaterialCatalog;
use render_scene::{SmodelPassMaterial, TessMaterials, WorldModelLightingAtlas};

struct ModelMaterialsOwner {
    catalog: Arc<RuntimeMaterialCatalog>,
    images: Arc<Vec<Option<Handle<Image>>>>,
    atlas: Option<bevy::asset::AssetId<Image>>,
    bodies: Option<Arc<asset_model::BodyMeshCatalog>>,
    world: Option<u64>,
    weapons: Option<u64>,
    scene_models: bool,
    projectiles: bool,
}

#[derive(Resource, Default)]
pub struct PreparedModelMaterials {
    owner: Option<ModelMaterialsOwner>,
    by_key: HashMap<asset_core::MaterialKey, SmodelPassMaterial>,
    by_authored: HashMap<assets::MaterialIndex, SmodelPassMaterial>,
    scene_dobjs: HashMap<String, Arc<xmodel_runtime::DObj>>,
    projectile_materials: HashMap<assets::MaterialIndex, SmodelPassMaterial>,
    projectile_dobjs: HashMap<(asset_core::FamilyId, String), Arc<xmodel_runtime::DObj>>,
}

impl PreparedModelMaterials {
    pub fn settled_for(&self, catalog: &Arc<RuntimeMaterialCatalog>) -> bool {
        self.owner
            .as_ref()
            .is_some_and(|owner| Arc::ptr_eq(&owner.catalog, catalog))
    }

    pub fn material(
        &self,
        catalog: &Arc<RuntimeMaterialCatalog>,
        key: &asset_core::MaterialKey,
    ) -> Option<&SmodelPassMaterial> {
        if !self.settled_for(catalog) {
            return None;
        }
        self.by_key.get(key)
    }

    pub fn authored(
        &self,
        catalog: &Arc<RuntimeMaterialCatalog>,
        authored: assets::MaterialIndex,
    ) -> Option<&SmodelPassMaterial> {
        if !self.settled_for(catalog) {
            return None;
        }
        self.by_authored.get(&authored)
    }

    pub fn scene_dobj(&self, model: &str) -> Option<&Arc<xmodel_runtime::DObj>> {
        self.scene_dobjs.get(model)
    }

    pub fn projectile_material(
        &self,
        catalog: &Arc<RuntimeMaterialCatalog>,
        authored: assets::MaterialIndex,
    ) -> Option<&SmodelPassMaterial> {
        if !self.settled_for(catalog) {
            return None;
        }
        self.projectile_materials.get(&authored)
    }

    pub fn projectile_dobj(
        &self,
        namespace: asset_core::FamilyId,
        model: &str,
    ) -> Option<&Arc<xmodel_runtime::DObj>> {
        self.projectile_dobjs.get(&(namespace, model.to_owned()))
    }

    pub fn clear(&mut self) {
        *self = Self::default();
    }
}

fn admit_keys<'a>(
    keys: impl IntoIterator<Item = &'a asset_core::MaterialKey>,
    atlas: &WorldModelLightingAtlas,
    catalog: &RuntimeMaterialCatalog,
    by_key: &mut HashMap<asset_core::MaterialKey, SmodelPassMaterial>,
    refused: &mut HashSet<asset_core::MaterialKey>,
) {
    for key in keys {
        if by_key.contains_key(key) || refused.contains(key) {
            continue;
        }
        match crate::body_lit_pass_material(atlas, catalog, key) {
            Some(material) => {
                by_key.insert(key.clone(), material);
            }
            None => {
                refused.insert(key.clone());
            }
        }
    }
}

pub fn scene_lit_pass_material(
    atlas: &WorldModelLightingAtlas,
    tess: &TessMaterials,
    authored: assets::MaterialIndex,
) -> Option<SmodelPassMaterial> {
    let world_material = tess.catalog().derived(authored)?;
    if world_material.shadow_only {
        return None;
    }
    let ordinal = tess
        .catalog()
        .parts()
        .sorted_materials
        .ordinal_for_asset_id(authored.order())?;
    let maps = render_scene::runtime_maps(
        Some(authored),
        tess.catalog(),
        tess.material_images.as_ref(),
    );
    let inv_h = lighting_iw4::model_lighting_inv_image_height(atlas.dims.image_height)?;
    let scale = lighting_iw4::model_lighting_lookup_scale(inv_h);
    Some(SmodelPassMaterial {
        model_lighting_required: true,
        color: maps.color,
        specular: maps.specular,
        probe: None,
        atlas: Some(atlas.image.clone()),
        alpha_mode: maps.alpha_mode,
        draw_mode: maps.draw_mode,
        cull_mode: maps.cull_mode,
        env_map_parms: maps.env_map_parms,
        lighting_lookup_scale: [scale.u, scale.v, scale.w, scale.q],
        atlas_lookup: [
            lighting_iw4::MODEL_LIGHTING_INV_ATLAS_WIDTH as f32,
            inv_h,
            lighting_iw4::MODEL_LIGHTING_VOLUME_W,
            0.0,
        ],
        sort_key: world_material.sort_key,
        material_sorted_index: Some(ordinal.get()),
    })
}

fn present_keys<'a>(
    keys: &'a [Option<asset_core::MaterialKey>],
    edges: &'a [assets::AssetEdge<assets::MaterialSpace>],
) -> impl Iterator<Item = &'a asset_core::MaterialKey> {
    keys.iter()
        .zip(edges)
        .filter(|(_, edge)| edge.is_bound())
        .filter_map(|(key, _)| key.as_ref())
}

#[derive(bevy::ecs::system::SystemParam)]
pub struct ModelMaterialAdmission<'w> {
    tess: Res<'w, TessMaterials>,
    atlas: Option<Res<'w, WorldModelLightingAtlas>>,
    prepared: ResMut<'w, PreparedModelMaterials>,
}

impl ModelMaterialAdmission<'_> {
    fn prepare(
        mut self,
        bodies: Option<Res<assets::PreparedBodies>>,
        world_weapons: Option<Res<assets::PreparedWorldWeapons>>,
        weapons: Option<Res<assets::PreparedWeapons>>,
        scene_models: Option<Res<asset_world::MapXModelSceneCatalog>>,
        projectiles: Option<Res<assets::PreparedProjectileMeshes>>,
    ) {
        if self.tess.material_images.is_empty() {
            return;
        }
        let atlas_id = self.atlas.as_ref().map(|atlas| atlas.image.id());
        let world_id = world_weapons.as_ref().map(|world| world.0.identity());
        let weapons_revision = weapons
            .as_ref()
            .map(|weapons| weapons.registry().revision());
        let bodies_catalog = bodies.as_ref().map(|bodies| &bodies.0);
        if self.prepared.owner.as_ref().is_some_and(|owner| {
            Arc::ptr_eq(&owner.catalog, self.tess.catalog())
                && Arc::ptr_eq(&owner.images, &self.tess.material_images)
                && owner.atlas == atlas_id
                && owner.world == world_id
                && owner.weapons == weapons_revision
                && owner.scene_models == scene_models.is_some()
                && owner.projectiles == projectiles.is_some()
                && match (&owner.bodies, bodies_catalog) {
                    (Some(previous), Some(current)) => Arc::ptr_eq(previous, current),
                    (None, None) => true,
                    _ => false,
                }
        }) && !scene_models
            .as_ref()
            .is_some_and(|models| models.is_changed())
            && !projectiles
                .as_ref()
                .is_some_and(|meshes| meshes.is_changed())
        {
            return;
        }
        let owner = ModelMaterialsOwner {
            catalog: Arc::clone(self.tess.catalog()),
            images: Arc::clone(&self.tess.material_images),
            atlas: atlas_id,
            bodies: bodies_catalog.cloned(),
            world: world_id,
            weapons: weapons_revision,
            scene_models: scene_models.is_some(),
            projectiles: projectiles.is_some(),
        };
        let (Some(atlas), Some(bodies), Some(world)) = (self.atlas, bodies, world_weapons) else {
            diag::info!(
                World,
                "model materials: nothing to admit (lighting atlas or model catalogs missing)"
            );
            *self.prepared = PreparedModelMaterials {
                owner: Some(owner),
                ..Default::default()
            };
            return;
        };
        let started = std::time::Instant::now();
        let mut by_key = HashMap::new();
        let mut refused = HashSet::new();
        for name in bodies.0.names() {
            if let Some(entry) = bodies.0.get(name) {
                admit_keys(
                    present_keys(&entry.material_keys, &entry.material_edges),
                    &atlas,
                    self.tess.catalog(),
                    &mut by_key,
                    &mut refused,
                );
            }
        }
        for index in 0..world.0.len() {
            if let Some(entry) = world.0.get_at(index) {
                admit_keys(
                    present_keys(&entry.material_keys, &entry.material_edges),
                    &atlas,
                    self.tess.catalog(),
                    &mut by_key,
                    &mut refused,
                );
            }
        }
        if let Some(weapons) = weapons.as_deref() {
            for id in 1..=weapons.registry().len() as u32 {
                for appearance in weapons.registry().appearances_of(id) {
                    let Some(camo) = appearance.material_camouflage() else {
                        continue;
                    };
                    for (_, key) in &camo.materials {
                        if by_key.contains_key(key) {
                            continue;
                        }
                        let Some(authored) = self.tess.catalog().material_for_key(key) else {
                            continue;
                        };
                        if let Some(material) = scene_lit_pass_material(
                            &atlas,
                            &self.tess,
                            assets::MaterialIndex::from_order(usize::from(authored.asset_id.0)),
                        ) {
                            by_key.insert(key.clone(), material);
                        }
                    }
                }
            }
        }
        let mut by_authored = HashMap::new();
        let mut scene_dobjs = HashMap::new();
        if let Some(scene_models) = scene_models.as_deref() {
            for (key, asset) in scene_models.iter() {
                let skel = match asset {
                    asset_world::MapXModelSceneAsset::Captured { skel, .. } => skel,
                    asset_world::MapXModelSceneAsset::Unavailable { .. } => continue,
                };
                for surface in 0..skel.surface_materials.len() {
                    let Some(authored) = scene_models.surface_material(key, surface) else {
                        continue;
                    };
                    if by_authored.contains_key(&authored) {
                        continue;
                    }
                    if let Some(material) = scene_lit_pass_material(&atlas, &self.tess, authored) {
                        by_authored.insert(authored, material);
                    }
                }
                if let Some(pose) = skel.pose.as_ref()
                    && let Ok(dobj) = xmodel_runtime::DObj::build(&[(pose, None)])
                {
                    scene_dobjs.insert(key.0.clone(), Arc::new(dobj));
                }
            }
        }
        let mut projectile_materials = HashMap::new();
        let mut projectile_dobjs = HashMap::new();
        if let Some(projectiles) = projectiles.as_deref() {
            for index in 0..projectiles.0.len() {
                let Some(entry) = projectiles.0.get_at(index) else {
                    continue;
                };
                let key = projectiles.0.key_at(index).cloned().unwrap_or(
                    asset_model::ProjectileMeshKey::new(entry.namespace, &entry.skel.name),
                );
                for surface in 0..entry.material_edges.len() {
                    let Some(authored) = entry.material_index(surface) else {
                        continue;
                    };
                    if projectile_materials.contains_key(&authored) {
                        continue;
                    }
                    if let Some(material) = crate::authored_lit_xmodel_pass_material(
                        &atlas,
                        self.tess.catalog(),
                        self.tess.material_images.as_ref(),
                        authored,
                    ) {
                        projectile_materials.insert(authored, material);
                    }
                }
                if let Some(pose) = entry.skel.pose.as_ref()
                    && let Ok(dobj) = xmodel_runtime::DObj::build(&[(pose, None)])
                {
                    projectile_dobjs.insert((key.namespace, key.name), Arc::new(dobj));
                }
            }
        }
        let mut refused: Vec<_> = refused.into_iter().collect();
        refused.sort_by(|left, right| {
            (left.namespace.as_str(), &left.name).cmp(&(right.namespace.as_str(), &right.name))
        });
        diag::info!(
            World,
            "model materials prepared before Ready: admitted={} map_model_materials={} map_model_dobjs={} projectile_materials={} projectile_dobjs={} refused={} elapsed={:.1}ms{}",
            by_key.len(),
            by_authored.len(),
            scene_dobjs.len(),
            projectile_materials.len(),
            projectile_dobjs.len(),
            refused.len(),
            started.elapsed().as_secs_f64() * 1000.0,
            if refused.is_empty() {
                String::new()
            } else {
                format!(
                    " first_refused=[{}]",
                    refused
                        .iter()
                        .take(8)
                        .map(|key| format!("{}:{}", key.namespace.as_str(), key.name))
                        .collect::<Vec<_>>()
                        .join(",")
                )
            }
        );
        *self.prepared = PreparedModelMaterials {
            owner: Some(owner),
            by_key,
            by_authored,
            scene_dobjs,
            projectile_materials,
            projectile_dobjs,
        };
    }
}

pub fn prepare_model_materials(
    bodies: Option<Res<assets::PreparedBodies>>,
    world_weapons: Option<Res<assets::PreparedWorldWeapons>>,
    weapons: Option<Res<assets::PreparedWeapons>>,
    scene_models: Option<Res<asset_world::MapXModelSceneCatalog>>,
    projectiles: Option<Res<assets::PreparedProjectileMeshes>>,
    admission: ModelMaterialAdmission,
) {
    admission.prepare(bodies, world_weapons, weapons, scene_models, projectiles);
}
