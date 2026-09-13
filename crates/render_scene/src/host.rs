use std::collections::{HashMap, HashSet};

use bevy::prelude::*;
use dpvs_iw4::set_scene_ent_part_bit;
use lighting_iw4::{
    SPOT_SHADOW_ENTITY_ORIGIN_TRACK_ENTS, SPOT_SHADOW_ENTITY_RELINK_THRESH_SQ, SpotShadowSceneSlot,
    spot_shadow_entity_origin_track_index, spot_shadow_entity_should_relink,
};

use crate::{AddBModelArgs, AddBModelPose, AddDObjArgs, AddDObjPose, GfxScene, scene_info_entnum};

#[derive(Resource, Default)]
pub struct HostGfxScene {
    pub scene: GfxScene,

    pub census_valid: bool,
}

#[derive(Resource, Default)]
pub struct SpotShadowSceneOccupancy {
    pub dobjs: Vec<SpotShadowSceneSlot>,
    pub models: Vec<SpotShadowSceneSlot>,
}

#[derive(Resource, Clone, Debug)]
pub struct SpotShadowEntityOriginTrack {
    pub last: Vec<[f32; 3]>,
    pub relink_n: u32,
}

impl Default for SpotShadowEntityOriginTrack {
    fn default() -> Self {
        Self {
            last: vec![[0.0; 3]; SPOT_SHADOW_ENTITY_ORIGIN_TRACK_ENTS as usize],
            relink_n: 0,
        }
    }
}

fn spot_slot(info: u32, bounds: Option<dpvs_iw4::Bounds>) -> SpotShadowSceneSlot {
    SpotShadowSceneSlot {
        info,
        box_mid: bounds.map(|b| b.mid()),
        box_half: bounds.map(|b| b.half()),
    }
}

#[must_use]
pub fn gfx_scene_spot_shadow_dobj_slots(scene: &GfxScene) -> Vec<SpotShadowSceneSlot> {
    scene
        .scene_dobjs
        .iter()
        .map(|d| spot_slot(d.info, d.posed_bounds))
        .collect()
}

#[must_use]
pub fn gfx_scene_spot_shadow_model_slots(scene: &GfxScene) -> Vec<SpotShadowSceneSlot> {
    scene
        .scene_models
        .iter()
        .map(|m| spot_slot(m.info, m.posed_bounds))
        .collect()
}

pub fn snapshot_spot_shadow_occupancy(
    gfx: Res<HostGfxScene>,
    mut occ: ResMut<SpotShadowSceneOccupancy>,
    mut track: ResMut<SpotShadowEntityOriginTrack>,
) {
    occ.dobjs = gfx_scene_spot_shadow_dobj_slots(&gfx.scene);
    occ.models = gfx_scene_spot_shadow_model_slots(&gfx.scene);
    track_spot_shadow_entity_origins(&gfx, &mut track);
}

fn track_spot_shadow_entity_origins(gfx: &HostGfxScene, track: &mut SpotShadowEntityOriginTrack) {
    track.relink_n = 0;
    let ents = gfx
        .scene
        .scene_dobjs
        .iter()
        .map(|d| (d.origin, d.info))
        .chain(gfx.scene.scene_models.iter().map(|m| (m.origin, m.info)));
    for (origin, info) in ents {
        let Some(idx) = spot_shadow_entity_origin_track_index(0, scene_info_entnum(info)) else {
            continue;
        };
        let Some(slot) = track.last.get_mut(idx) else {
            continue;
        };
        if spot_shadow_entity_should_relink(*slot, origin, SPOT_SHADOW_ENTITY_RELINK_THRESH_SQ) {
            *slot = origin;
            track.relink_n = track.relink_n.saturating_add(1);
        }
    }
}

#[derive(SystemSet, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct GfxSceneClear;

#[derive(SystemSet, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct GfxSceneAdd;

pub fn clear_host_gfx_scene(
    mut scene: ResMut<HostGfxScene>,
    mut skin_inputs: ResMut<SceneEntSkinInputs>,
    mut spot: ResMut<SpotShadowSceneOccupancy>,
) {
    scene.scene.clear();
    scene.census_valid = true;
    skin_inputs.by_ent.clear();
    skin_inputs.pending.clear();
    skin_inputs.frame_bytes_used = 0;
    *spot = SpotShadowSceneOccupancy::default();
}

pub fn scene_quat_from_angles(angles: [f32; 3]) -> [f32; 4] {
    fx_iw4::fx_axis_to_quat(math_iw4::angles_to_axis(angles))
}

pub fn scene_quat_from_viewmodel_axes(gun_angles: [f32; 3], view_angles: [f32; 3]) -> [f32; 4] {
    fx_iw4::fx_axis_to_quat(fx_iw4::fx_mat3_mul(
        math_iw4::angles_to_axis(gun_angles),
        math_iw4::angles_to_axis(view_angles),
    ))
}

pub fn occupy_add_dobj(scene: &mut GfxScene, model_n: usize, has_tree: bool, pose: AddDObjPose) {
    occupy_add_dobj_fx(scene, model_n, has_tree, 0, pose);
}

pub fn occupy_add_dobj_fx(
    scene: &mut GfxScene,
    model_n: usize,
    has_tree: bool,
    render_fx_flags: u32,
    pose: AddDObjPose,
) {
    let num_models = u8::try_from(model_n).unwrap_or(u8::MAX);
    let _ = scene.add_dobj(AddDObjArgs {
        render_fx_flags,
        material_time: 0.0,
        has_tree,
        num_models,
        pose: Some(pose),
    });
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ScriptMoverBmodelClaim {
    pub model_index: u32,
    pub origin: [f32; 3],
    pub angles: [f32; 3],
    pub entnum: u32,
}

pub fn occupy_add_bmodel(scene: &mut GfxScene, surf_id: i16, pose: AddBModelPose) {
    let _ = scene.add_bmodel(AddBModelArgs { surf_id }, pose);
}

fn surf_id_for_model(models: &[assets::GfxBrushModelSurfs], model_index: u32) -> i16 {
    models
        .get(model_index as usize)
        .and_then(|model| i16::try_from(model.surface_count).ok())
        .unwrap_or(0)
}

pub fn occupy_script_brushes(
    scene: &mut GfxScene,
    models: &[assets::GfxBrushModelSurfs],
    authored: &[assets::ScriptBrushModelPlacement],
    live: &[ScriptMoverBmodelClaim],
) {
    let mut claims: HashMap<u32, ScriptMoverBmodelClaim> = HashMap::new();
    for claim in live {
        if claim.model_index == 0 {
            continue;
        }
        claims.entry(claim.model_index).or_insert(*claim);
    }
    let mut used = HashSet::new();
    for brush in authored {
        if brush.cmodel_handle == 0 {
            continue;
        }
        let surf_id = surf_id_for_model(models, brush.cmodel_handle);
        if let Some(claim) = claims.get(&brush.cmodel_handle) {
            used.insert(claim.model_index);
            occupy_add_bmodel(
                scene,
                surf_id,
                AddBModelPose {
                    model_index: claim.model_index,
                    origin: claim.origin,
                    quat: Some(scene_quat_from_angles(claim.angles)),
                    param_4: u16::try_from(claim.entnum).ok(),
                },
            );
        } else {
            occupy_add_bmodel(
                scene,
                surf_id,
                AddBModelPose {
                    model_index: brush.cmodel_handle,
                    origin: brush.origin,
                    quat: Some(scene_quat_from_angles(brush.angles)),
                    param_4: None,
                },
            );
        }
    }
    for claim in claims.values() {
        if !used.insert(claim.model_index) {
            continue;
        }
        occupy_add_bmodel(
            scene,
            surf_id_for_model(models, claim.model_index),
            AddBModelPose {
                model_index: claim.model_index,
                origin: claim.origin,
                quat: Some(scene_quat_from_angles(claim.angles)),
                param_4: u16::try_from(claim.entnum).ok(),
            },
        );
    }
}

#[derive(Resource, Default)]
pub struct SceneEntSkinInputs {
    pub by_ent: HashMap<u32, SceneEntSkinInput>,

    pub pending: HashMap<u32, SceneEntSkinPending>,

    pub frame_bytes_used: u32,
}

#[derive(Clone, Debug, Default)]
pub struct SceneEntSkinInput {
    pub models: Vec<SceneEntSkinModel>,

    pub hide_part_bits: [u32; 6],
}

#[derive(Clone, Debug)]
pub struct SceneEntSkinModel {
    pub lod: i8,
    pub bone_count: u8,

    pub surfaces: std::sync::Arc<[dpvs_iw4::PreSkinSurface]>,
}

#[derive(Clone, Debug)]
pub struct SceneEntSkinPendingModel {
    pub lod: i8,
    pub bone_count: u8,
    pub skel: std::sync::Arc<assets::ModelSkel>,
}

#[derive(Clone, Debug, Default)]
pub struct SceneEntSkinPending {
    pub models: Vec<SceneEntSkinPendingModel>,
    pub hide_part_bits: [u32; 6],
}

#[derive(Resource, Default)]
pub struct SceneEntSurfaceCache {
    by_name: HashMap<String, HashMap<u8, std::sync::Arc<[dpvs_iw4::PreSkinSurface]>>>,
}

impl SceneEntSurfaceCache {
    pub fn surfaces(
        &mut self,
        name: &str,
        skel: &assets::ModelSkel,
        lod: u8,
    ) -> std::sync::Arc<[dpvs_iw4::PreSkinSurface]> {
        if let Some(hit) = self.by_name.get(name).and_then(|lods| lods.get(&lod)) {
            return std::sync::Arc::clone(hit);
        }
        let built: Vec<dpvs_iw4::PreSkinSurface> = skel
            .surfaces_for_lod(lod)
            .map(|surface| pre_skin_xsurface(skel, surface))
            .collect();
        let arc: std::sync::Arc<[dpvs_iw4::PreSkinSurface]> = built.into();
        self.by_name
            .entry(name.to_owned())
            .or_default()
            .insert(lod, std::sync::Arc::clone(&arc));
        arc
    }
}

fn pre_skin_xsurface(skel: &assets::ModelSkel, surface: usize) -> dpvs_iw4::PreSkinSurface {
    dpvs_iw4::PreSkinSurface {
        part_bits: skel
            .surface_part_bits
            .get(surface)
            .copied()
            .unwrap_or([0; 6]),
        vert_count: skel
            .surface_vertex_ranges
            .get(surface)
            .map(|(_, count)| u16::try_from(*count).unwrap_or(u16::MAX))
            .unwrap_or(0),
        deformed: skel
            .surface_deformed
            .get(surface)
            .copied()
            .flatten()
            .unwrap_or(true),
        vert_list_count: skel
            .surface_vert_list_count
            .get(surface)
            .copied()
            .flatten()
            .unwrap_or(0),
    }
}

pub fn store_scene_ent_pending(
    scene: &mut GfxScene,
    skin_inputs: &mut SceneEntSkinInputs,
    entnum: u32,
    models: Vec<SceneEntSkinPendingModel>,
    hide_part_bits: [u32; 6],
) {
    let lod_bytes: Vec<i8> = models.iter().map(|model| model.lod).collect();
    scene.store_scene_ent_lods(entnum, &lod_bytes);
    skin_inputs.pending.insert(
        entnum,
        SceneEntSkinPending {
            models,
            hide_part_bits,
        },
    );
}

pub fn expand_scene_ent_pending(
    pending: &SceneEntSkinPending,
    surface_cache: &mut SceneEntSurfaceCache,
) -> SceneEntSkinInput {
    let models = pending
        .models
        .iter()
        .map(|model| SceneEntSkinModel {
            lod: model.lod,
            bone_count: model.bone_count,
            surfaces: if model.lod < 0 {
                std::sync::Arc::from([])
            } else {
                surface_cache.surfaces(
                    &model.skel.name,
                    &model.skel,
                    u8::try_from(model.lod).unwrap_or(0),
                )
            },
        })
        .collect();
    SceneEntSkinInput {
        models,
        hide_part_bits: pending.hide_part_bits,
    }
}

pub fn hide_part_bits_from_tags(
    bits: &mut [u32; 6],
    skel: &assets::ModelSkel,
    base: usize,
    hide_tags: &[String],
) {
    if hide_tags.is_empty() {
        return;
    }
    for bone in 0..skel.bone_names.len() {
        if assets::bone_has_hidden_ancestor(
            &skel.bone_names,
            |b| skel.parent_of(b),
            bone,
            hide_tags,
        ) {
            set_scene_ent_part_bit(bits, base + bone);
        }
    }
}
