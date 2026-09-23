use std::collections::HashMap;

use bevy::ecs::system::SystemParam;
use bevy::prelude::*;

use crate::anim::scene_submission::{AnimDObjSceneModel, AnimDObjSceneSubmission, AnimSceneSubmit};
use crate::anim::xmodel_pose::{
    PosedModelSurface, lod_local_surface, skin_model_filtered, stream_lod_surface_rigid,
};
use crate::occupancy::dyn_ent::{fpv_frustum_planes, sphere_behind_frustum};
use crate::{
    ScriptModelDrawPlan, ScriptModelOwnerDraw, append_script_model_asset,
    overwrite_script_model_asset, retain_script_model_assets,
};
use render_frame::RenderFocusFrame;
use render_scene::FpvLens;
use render_scene::WorldModelLightingAtlas;
use render_scene::{HostGfxScene, scene_quat_from_angles};
use render_scene::{
    LodRampArgs, SmodelPassMaterial, XModelColourRefusal, XModelSurfaceDraw, smodel_camera_lod,
};
use render_scene::{ModelLightingOwner, ModelLightingRequest, ModelLightingRequests};
use render_scene::{TessMaterials, WorldPresentFacts, WorldScriptModelInstance};

#[derive(Resource, Default)]
struct ScriptModelDobjs {
    by_id: HashMap<u32, PersistentScriptDobj>,
    mover_pose: HashMap<u32, ScriptMoverCentitySample>,
}

struct PersistentScriptDobj {
    dobj: assets::DObj,
    reuse_key: assets::dobj::DObjReuseKey,
}

#[derive(Clone, Copy, Debug)]
struct ScriptMoverCentitySample {
    angles: [f32; 3],
    skip: Option<&'static str>,
}

impl ScriptMoverCentitySample {
    fn posed(angles: [f32; 3]) -> Self {
        Self { angles, skip: None }
    }

    fn skip(why: &'static str) -> Self {
        Self {
            angles: [0.0; 3],
            skip: Some(why),
        }
    }
}

fn scene_quat_from_mover_sample(sample: Option<&ScriptMoverCentitySample>) -> Option<[f32; 4]> {
    let sample = sample?;
    if sample.skip.is_some() {
        return None;
    }
    Some(scene_quat_from_angles(sample.angles))
}

#[derive(SystemParam)]
struct ScriptModelPoseLocals<'w, 's> {
    live_assets: Local<'s, Vec<usize>>,
    live_ids: Local<'s, std::collections::HashSet<u32>>,
    keep: Local<'s, Vec<bool>>,
    remap: Local<'s, Vec<Option<usize>>>,
    lod_skinned: Res<'w, render_scene::LodRampSkinnedDvar>,
    gfx_scene: Res<'w, HostGfxScene>,
}

#[derive(SystemParam)]
struct ScriptModelTessLocals<'w, 's> {
    last_material_generation: Local<'s, Option<render_material::MaterialGenerationId>>,
    last_unbound_materials: Local<'s, usize>,
    model_materials: Res<'w, crate::anim::model_materials::PreparedModelMaterials>,
    live_plan: Local<'s, Vec<usize>>,
    product_to_plan: Local<'s, Vec<usize>>,
    keep: Local<'s, Vec<bool>>,
    remap: Local<'s, Vec<Option<usize>>>,
}

struct ScriptOwnerRow {
    entity: Entity,
    asset_index: usize,
    focused_owner_id: Option<u32>,
    model: assets::MapXModelAssetKey,
    world_from_local: Mat4,
    entnum: Option<u32>,
    camera_hidden: bool,
    lighting_origin: [f32; 3],
    lookup_fallback: u8,
    caster_bound: render_scene::XModelCasterBound,
}

#[derive(Resource, Default)]
struct ScriptModelPoseProduct {
    assets: Vec<ScriptPosedAsset>,
    rows: Vec<ScriptOwnerRow>,
    producer_unavailable: bool,
    surface_material_unbound: usize,
    surface_material_first: Option<String>,
}

struct ScriptPosedAsset {
    key: assets::MapXModelAssetKey,
    dobj_state: assets::dobj::DObjSemanticState,
    camera_lods: Vec<Option<u8>>,
    surfaces: Vec<PosedModelSurface>,
    authored: Vec<Option<assets::MaterialIndex>>,
}

impl ScriptModelPoseProduct {
    fn asset_index(
        &self,
        key: &assets::MapXModelAssetKey,
        dobj_state: &assets::dobj::DObjSemanticState,
        camera_lods: &[Option<u8>],
    ) -> Option<usize> {
        self.assets.iter().position(|asset| {
            &asset.key == key && &asset.dobj_state == dobj_state && asset.camera_lods == camera_lods
        })
    }

    fn clear_frame(&mut self) {
        self.rows.clear();
        self.producer_unavailable = false;
        self.surface_material_unbound = 0;
        self.surface_material_first = None;
    }
}

struct ScriptSceneSlot<'a> {
    hidden: bool,
    skin_entries: &'a [dpvs_iw4::SceneEntSkinEntry],
}

fn script_scene_slot(scene: &render_scene::GfxScene, entnum: u32) -> ScriptSceneSlot<'_> {
    ScriptSceneSlot {
        hidden: scene.scene_ent_skips_draw(entnum),
        skin_entries: scene
            .scene_ent_skinned_surfs(entnum)
            .map(|surfs| surfs.entries.as_slice())
            .unwrap_or(&[]),
    }
}

#[derive(SystemSet, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ScriptModelDrawSet;

#[derive(SystemSet, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ScriptModelSkinSet;

#[derive(Resource, Debug, Default)]
pub struct RenderFocus {
    pub frame: Option<RenderFocusFrame>,
}

impl RenderFocus {
    pub fn begin_frame(&mut self, owner_id: u32) {
        self.frame = Some(RenderFocusFrame::unresolved(owner_id, "owner_not_found"));
    }

    pub fn refuse(&mut self, owner_id: u32, model: &str, outcome: &'static str) {
        self.frame = Some(RenderFocusFrame {
            model: Some(model.to_owned()),
            ..RenderFocusFrame::unresolved(owner_id, outcome)
        });
    }
}

const PERF_FOCUS_ENV: &str = "IW4L_PERF_FOCUS";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum RenderFocusSelector {
    ScriptModel(u32),
}

#[derive(Resource, Debug, Default)]
struct RenderFocusSelect {
    selector: Option<RenderFocusSelector>,
}

impl RenderFocusSelect {
    fn from_env() -> Result<Self, String> {
        let raw = match std::env::var(PERF_FOCUS_ENV) {
            Ok(raw) => raw,
            Err(std::env::VarError::NotPresent) => return Ok(Self::default()),
            Err(std::env::VarError::NotUnicode(_)) => {
                return Err(format!("{PERF_FOCUS_ENV} is not valid UTF-8"));
            }
        };
        if !perf::recording() {
            return Err(format!(
                "{PERF_FOCUS_ENV} requires IW4L_PERF=1; focused facts exist only in .pftrace"
            ));
        }
        let (kind, id) = raw.split_once(':').ok_or_else(|| {
            format!("invalid {PERF_FOCUS_ENV}={raw:?}; expected script_model:<u32>")
        })?;
        let id = id
            .parse::<u32>()
            .map_err(|_| format!("invalid {PERF_FOCUS_ENV}={raw:?}; owner id must be u32"))?;
        let selector = match kind {
            "script_model" => RenderFocusSelector::ScriptModel(id),
            _ => {
                return Err(format!(
                    "unsupported {PERF_FOCUS_ENV} kind {kind:?}; supported: script_model"
                ));
            }
        };
        Ok(Self {
            selector: Some(selector),
        })
    }

    fn script_model_id(&self) -> Option<u32> {
        match self.selector {
            Some(RenderFocusSelector::ScriptModel(id)) => Some(id),
            None => None,
        }
    }
}

pub fn register_script_model_systems(app: &mut App) {
    let select = RenderFocusSelect::from_env()
        .unwrap_or_else(|error| panic!("focused render observation refused: {error}"));
    app.insert_resource(select)
        .init_resource::<RenderFocus>()
        .init_resource::<ScriptModelDrawPlan>()
        .init_resource::<ScriptModelDobjs>()
        .init_resource::<ScriptModelPoseProduct>()
        .add_systems(
            Update,
            (
                apply_presented_script_model_dobjs,
                apply_script_mover_centity_pose,
                occupy_script_model_scene_ents,
            )
                .chain()
                .in_set(frame::RenderSet::Anim)
                .before(frame::WorkerCmdSet::CellDynModel)
                .in_set(ScriptModelDrawSet)
                .in_set(render_scene::GfxSceneAdd)
                .in_set(AnimSceneSubmit),
        )
        .add_systems(
            Update,
            (pose_script_models, commit_script_model_draw_plan)
                .chain()
                .after(occupy_script_model_scene_ents)
                .after(frame::WorkerCmdSet::CellSceneEnt)
                .after(frame::WorkerCmdSet::DpvsEnt)
                .in_set(ScriptModelSkinSet)
                .in_set(frame::WorkerCmdSet::SkinModel),
        );
}

fn apply_presented_script_model_dobjs(
    presented: Option<Res<net::PresentedSnapshot>>,
    mut owners: Query<(&mut WorldScriptModelInstance, &mut Visibility)>,
) {
    let Some(snapshot) = presented.as_ref().and_then(|value| value.snapshot()) else {
        return;
    };
    let mut by_owner: std::collections::BTreeMap<
        sim::AuthorityModelOwner,
        &assets::dobj::DObjSemanticState,
    > = std::collections::BTreeMap::new();
    for (owner, state) in &snapshot.meta.entity_dobjs {
        by_owner.entry(*owner).or_insert(state);
    }
    for (mut owner, mut visibility) in &mut owners {
        let source = owner.id.source_ordinal();
        if let Some(flag) = snapshot
            .meta
            .objectives
            .flags
            .iter()
            .find(|f| f.model_source == source)
        {
            let model = &snapshot.meta.objectives.flag_models[flag.owner as usize];
            if !model.is_empty() {
                if owner.current_model.0 != *model {
                    owner.current_model = assets::MapXModelAssetKey(model.clone());
                    owner.dobj_state = assets::dobj::DObjSemanticState::bind_pose(
                        model.clone(),
                        flag.owner as u32 + 1,
                        1,
                    );
                }
                *visibility = Visibility::Inherited;
            } else {
                *visibility = Visibility::Hidden;
            }
            continue;
        }
        if let Some(bomb) = snapshot
            .meta
            .objectives
            .bombs
            .iter()
            .find(|b| b.view.model_source == source)
        {
            *visibility = if bomb.planted_at_ms.is_some() {
                Visibility::Inherited
            } else {
                Visibility::Hidden
            };
            continue;
        }
        let objective_visible = snapshot.meta.objectives.model_visible(source);
        if objective_visible == Some(false) {
            *visibility = Visibility::Hidden;
            continue;
        }
        let Some(authority_owner) = owner.authority_owner else {
            *visibility = Visibility::Hidden;
            continue;
        };
        let Some(state) = by_owner.get(&authority_owner).copied() else {
            *visibility = Visibility::Hidden;
            continue;
        };
        let Some(base) = state.composition.models.first() else {
            *visibility = Visibility::Hidden;
            continue;
        };
        if owner.current_model.0 != base.model {
            owner.current_model = assets::MapXModelAssetKey(base.model.clone());
        }
        if owner.dobj_state != *state {
            owner.dobj_state = state.clone();
        }

        *visibility = if objective_visible != Some(true)
            && gamemode_iw4::setup_exploders_hides(
                &owner.current_model.0,
                &owner.metadata.targetname,
                &owner.metadata.script_exploder,
            ) {
            Visibility::Hidden
        } else {
            Visibility::Inherited
        };
    }
}

fn sample_script_mover_pose(
    runtime: &net::CEntityRuntime,
    at_time: net::ServerTime,
) -> ([f32; 3], [f32; 3]) {
    (
        entity_iw4::bg_evaluate_trajectory(&runtime.current.pos, at_time.ms()),
        entity_iw4::bg_evaluate_trajectory(&runtime.current.apos, at_time.ms()),
    )
}

fn apply_script_mover_centity_pose(
    slots: Option<Res<net::CEntitySlots>>,
    presented: Option<Res<net::PresentedSnapshot>>,
    runtimes: Query<&net::CEntityRuntime>,
    mut owners: Query<(&WorldScriptModelInstance, &mut Transform)>,
    mut persist: ResMut<ScriptModelDobjs>,
) {
    persist.mover_pose.clear();
    let Some(slots) = slots else {
        return;
    };
    let Some(snapshot) = presented.as_ref().and_then(|value| value.snapshot()) else {
        return;
    };
    let at_time = net::ServerTime::from_tick(snapshot.tick);
    for (owner, mut transform) in &mut owners {
        let mapent = owner.id.source_ordinal();
        if let Some(bomb) = snapshot
            .meta
            .objectives
            .bombs
            .iter()
            .find(|b| b.view.model_source == mapent)
        {
            transform.translation = Vec3::from_array(bomb.bomb_origin);
            let [pitch, yaw, roll] = bomb.bomb_angles.map(f32::to_radians);
            transform.rotation = Quat::from_euler(EulerRot::ZYX, yaw, pitch, roll);
            persist
                .mover_pose
                .insert(mapent, ScriptMoverCentitySample::posed(bomb.bomb_angles));
            continue;
        }
        let is_fan = owner.metadata.targetname == sim::FAN_BLADE_ROTATE_TARGETNAME
            || owner.metadata.targetname == sim::FAN_BLADE_ROTATE_FAST_TARGETNAME;
        let Some(number) = owner.gentity_number else {
            if is_fan {
                persist
                    .mover_pose
                    .insert(mapent, ScriptMoverCentitySample::skip("no_number"));
            }
            continue;
        };
        let Some(entity) = slots.entity_for_number(number) else {
            persist
                .mover_pose
                .insert(mapent, ScriptMoverCentitySample::skip("no_slot"));
            continue;
        };
        let Ok(runtime) = runtimes.get(entity) else {
            persist
                .mover_pose
                .insert(mapent, ScriptMoverCentitySample::skip("no_runtime"));
            continue;
        };
        let (origin, angles) = sample_script_mover_pose(runtime, at_time);
        let [pitch, yaw, roll] = angles;
        transform.rotation = Quat::from_euler(
            EulerRot::ZYX,
            yaw.to_radians(),
            pitch.to_radians(),
            roll.to_radians(),
        );
        transform.translation = Vec3::from_array(origin);
        persist
            .mover_pose
            .insert(mapent, ScriptMoverCentitySample::posed(angles));
    }
}

fn occupy_script_model_scene_ents(
    assets: Option<Res<assets::MapXModelSceneCatalog>>,
    cameras: Query<(&GlobalTransform, &Projection, &Camera), With<FpvLens>>,
    owners: Query<(&WorldScriptModelInstance, &Transform, &Visibility)>,
    persist: Res<ScriptModelDobjs>,
    mut scene_submissions: MessageWriter<AnimDObjSceneSubmission>,
    lod_skinned: Res<render_scene::LodRampSkinnedDvar>,
) {
    let Some(assets) = assets else {
        return;
    };
    let eye = cameras
        .single()
        .ok()
        .map(|(xf, _, _)| xf.translation().to_array());
    let skinned_ramp = lod_skinned.args();
    for (owner, transform, visibility) in &owners {
        if *visibility == Visibility::Hidden {
            continue;
        }
        let Some(skels) = presented_skel_arcs(&assets, &owner.dobj_state) else {
            continue;
        };
        let skel_refs: Vec<&assets::ModelSkel> = skels.iter().map(|skel| skel.as_ref()).collect();
        if dobj_lod_culled(
            &skel_refs,
            transform.translation.to_array(),
            eye,
            skinned_ramp,
        ) {
            continue;
        }
        let origin = transform.translation.to_array();
        let quat = scene_quat_from_mover_sample(persist.mover_pose.get(&owner.id.source_ordinal()));
        let Some(entnum) = owner.gentity_number.map(u32::from) else {
            continue;
        };
        let lod_view = Some(DObjLodView {
            origin,
            eye: eye.map(Vec3::from_array),
            ramp: skinned_ramp,
        });
        let camera_lods: Vec<Option<u8>> = skels
            .iter()
            .map(|skel| submodel_camera_lod(skel, lod_view))
            .collect();
        let hide = *owner.dobj_state.hide_part_bits.words();
        let models: Vec<AnimDObjSceneModel> = skels
            .iter()
            .zip(camera_lods.iter())
            .map(|(skel, lod)| AnimDObjSceneModel {
                lod: lod.map(|lod| i8::try_from(lod).unwrap_or(-1)).unwrap_or(-1),
                bone_count: u8::try_from(skel.bones.len()).unwrap_or(u8::MAX),
                skel: std::sync::Arc::clone(skel),
            })
            .collect();
        scene_submissions.write(AnimDObjSceneSubmission {
            render_fx_flags: 0,
            has_tree: owner.dobj_state.tree.is_some(),
            origin,
            lighting_origin: owner.lighting_origin,
            radius: skels.iter().find_map(|skel| skel.radius),
            entnum,
            quat,
            occupy_model_n: u8::try_from(models.len()).unwrap_or(u8::MAX),
            models,
            hide_part_bits: hide,
            store_skin: true,
        });
    }
}

fn pose_script_models(
    assets: Option<Res<assets::MapXModelSceneCatalog>>,
    atlas: Option<Res<WorldModelLightingAtlas>>,
    atpoint: Res<render_scene::DynAtPointLookup>,
    facts: Res<WorldPresentFacts>,
    xanims: Option<Res<assets::PreparedXAnims>>,
    cameras: Query<(&GlobalTransform, &Projection, &Camera), With<FpvLens>>,
    owners: Query<(Entity, &WorldScriptModelInstance, &Transform, &Visibility)>,
    mut persist: ResMut<ScriptModelDobjs>,
    mut product: ResMut<ScriptModelPoseProduct>,
    select: Res<RenderFocusSelect>,
    mut focus: ResMut<RenderFocus>,
    locals: ScriptModelPoseLocals,
) {
    let focus_id = select.script_model_id();
    if let Some(id) = focus_id {
        focus.begin_frame(id);
    }
    product.clear_frame();
    let ScriptModelPoseLocals {
        mut live_assets,
        mut live_ids,
        mut keep,
        mut remap,
        lod_skinned,
        gfx_scene,
    } = locals;
    live_assets.clear();
    live_ids.clear();
    let producer_ready = assets.is_some() && atlas.is_some() && facts.spawned;
    if !producer_ready {
        if let Some(id) = focus_id {
            focus.frame = Some(RenderFocusFrame::unresolved(id, "producer_unavailable"));
        }
        product.producer_unavailable = true;
        product.assets.clear();
        persist.by_id.clear();
        return;
    }
    let Some(assets) = assets else {
        return;
    };
    if assets.is_changed() {
        product.assets.clear();
    }
    let planes = fpv_frustum_planes(&cameras);
    let eye = cameras
        .single()
        .ok()
        .map(|(xf, _, _)| xf.translation().to_array());
    let skinned_ramp = lod_skinned.args();
    for (entity, owner, transform, visibility) in &owners {
        let owner_id = owner
            .authority_owner
            .and_then(|owner| owner.script_model())
            .map(|id| id.to_wire());
        let focused_owner_id = focus_id.filter(|wanted| owner_id == Some(*wanted));
        if let Some(id) = focused_owner_id {
            focus.refuse(id, &owner.current_model.0, "unclassified_branch");
        }
        if *visibility == Visibility::Hidden {
            if let Some(id) = focused_owner_id {
                focus.refuse(id, &owner.current_model.0, "hidden");
            }
            continue;
        }
        let entnum = owner.gentity_number.map(u32::from);

        let slot = entnum.map(|entnum| script_scene_slot(&gfx_scene.scene, entnum));
        if slot.as_ref().is_some_and(|slot| slot.hidden) {
            if let Some(id) = focused_owner_id {
                focus.refuse(id, &owner.current_model.0, "scene_ent_skip");
            }
            continue;
        }
        let Some(skels) = presented_skels(&assets, &owner.dobj_state) else {
            if let Some(id) = focused_owner_id {
                focus.refuse(id, &owner.current_model.0, "skeleton_unavailable");
            }
            continue;
        };
        let origin = transform.translation.to_array();

        if dobj_lod_culled(&skels, origin, eye, skinned_ramp) {
            if let Some(id) = focused_owner_id {
                focus.refuse(id, &owner.current_model.0, "lod_culled");
            }
            continue;
        }
        let lod_view = Some(DObjLodView {
            origin,
            eye: eye.map(Vec3::from_array),
            ramp: skinned_ramp,
        });
        let camera_lods: Vec<Option<u8>> = skels
            .iter()
            .map(|skel| submodel_camera_lod(skel, lod_view))
            .collect();

        let asset_index = if let Some(id) = owner_id {
            if compose_or_reuse_script_dobj(&mut persist, id, &assets, &owner.dobj_state).is_none()
            {
                persist.by_id.remove(&id);
                continue;
            }
            live_ids.insert(id);
            let request = owner
                .dobj_state
                .resolve_request(|name| xanims.as_ref()?.0.clip(assets::AssetNamespace::Iw4, name));
            let index = if let Some(index) =
                product.asset_index(&owner.current_model, &owner.dobj_state, &camera_lods)
            {
                index
            } else {
                let Some(slot_dobj) = persist.by_id.get(&id) else {
                    continue;
                };
                let Ok(request) = request else {
                    continue;
                };
                let skin_entries = slot.as_ref().map(|slot| slot.skin_entries).unwrap_or(&[]);
                let Some((surfaces, surface_materials)) = pose_script_dobj_with_materials(
                    Some(&assets),
                    &skels,
                    &slot_dobj.dobj,
                    &request,
                    lod_view,
                    skin_entries,
                ) else {
                    continue;
                };
                for (surface, authored) in surfaces.iter().zip(surface_materials.iter()) {
                    if authored.is_none() {
                        product.surface_material_unbound += 1;
                        if product.surface_material_first.is_none() {
                            product.surface_material_first = Some(format!(
                                "{} surface={}",
                                owner.current_model.0, surface.surface_index
                            ));
                        }
                    }
                }
                append_or_overwrite_script_pose(
                    &mut product,
                    &live_assets,
                    owner.current_model.clone(),
                    owner.dobj_state.clone(),
                    camera_lods.clone(),
                    surfaces,
                    surface_materials,
                )
            };
            live_assets.push(index);
            index
        } else {
            let Some(index) =
                product.asset_index(&owner.current_model, &owner.dobj_state, &camera_lods)
            else {
                if let Some(id) = focused_owner_id {
                    focus.refuse(id, &owner.current_model.0, "posed_asset_unavailable");
                }
                continue;
            };
            index
        };
        let box_half =
            crate::script_model_lighting_box_half(&assets, &owner.dobj_state.composition.models);
        let radius = skels
            .first()
            .and_then(|skel| skel.radius)
            .unwrap_or(1.0)
            .max(1.0);
        product.rows.push(ScriptOwnerRow {
            entity,
            asset_index,
            focused_owner_id,
            model: owner.current_model.clone(),
            world_from_local: transform.to_matrix(),
            entnum,
            camera_hidden: sphere_behind_frustum(origin, radius, &planes),
            lighting_origin: owner.lighting_origin,
            lookup_fallback: atpoint.fallback(owner.lighting_origin, box_half),
            caster_bound: render_scene::XModelCasterBound { origin, radius },
        });
    }
    persist.by_id.retain(|id, _| live_ids.contains(id));
    keep.clear();
    keep.resize(product.assets.len(), false);
    for &index in &live_assets {
        if let Some(slot) = keep.get_mut(index) {
            *slot = true;
        }
    }
    remap.clear();
    remap.resize(keep.len(), None);
    let mut next_index = 0usize;
    for (old_index, &is_live) in keep.iter().enumerate() {
        if is_live {
            remap[old_index] = Some(next_index);
            next_index += 1;
        }
    }
    let old = std::mem::take(&mut product.assets);
    let mut next = Vec::with_capacity(next_index);
    for (old_index, asset) in old.into_iter().enumerate() {
        if keep.get(old_index).copied().unwrap_or(false) {
            next.push(asset);
        }
    }
    product.assets = next;
    for row in &mut product.rows {
        if let Some(index) = remap.get(row.asset_index).copied().flatten() {
            row.asset_index = index;
        }
    }
}

fn commit_script_model_draw_plan(
    assets: Option<Res<assets::MapXModelSceneCatalog>>,
    atlas: Option<Res<WorldModelLightingAtlas>>,
    tess: Option<Res<TessMaterials>>,
    facts: Res<WorldPresentFacts>,
    mut plan: ResMut<ScriptModelDrawPlan>,
    product: Res<ScriptModelPoseProduct>,
    mut focus: ResMut<RenderFocus>,
    mut lighting_requests: ResMut<ModelLightingRequests>,
    locals: ScriptModelTessLocals,
    mut draws: Local<Vec<XModelSurfaceDraw>>,
    mut owners: Local<Vec<ScriptModelOwnerDraw>>,
) {
    let ScriptModelTessLocals {
        mut last_material_generation,
        mut last_unbound_materials,
        model_materials,
        mut live_plan,
        mut product_to_plan,
        mut keep,
        mut remap,
    } = locals;
    if product.producer_unavailable {
        plan.publish_no_rows();
        return;
    }
    if !facts.spawned {
        plan.publish_no_rows();
        return;
    }
    let (Some(assets), Some(atlas), Some(tess)) = (assets, atlas, tess) else {
        plan.publish_no_rows();
        return;
    };

    let material_generation = tess.catalog.generation_id;

    let catalog_reset = assets.is_changed()
        || atlas.is_changed()
        || tess.is_changed()
        || *last_material_generation != Some(material_generation);
    if catalog_reset {
        let (revision, generation, revisions) = (plan.revision, plan.generation, plan.revisions);
        *plan = ScriptModelDrawPlan::default();
        plan.revision = revision;
        plan.generation = generation;
        plan.revisions = revisions;
        *last_material_generation = Some(material_generation);
    }
    draws.clear();
    owners.clear();
    live_plan.clear();
    product_to_plan.clear();
    product_to_plan.resize(product.assets.len(), 0);
    for (product_index, posed) in product.assets.iter().enumerate() {
        let plan_index = if let Some(index) =
            plan.asset_index(&posed.key, &posed.dobj_state, &posed.camera_lods)
        {
            index
        } else {
            let materials = posed
                .surfaces
                .iter()
                .zip(posed.authored.iter().copied())
                .map(|(_surface, authored)| {
                    model_materials
                        .authored(&tess.catalog, authored?)
                        .cloned()
                })
                .collect::<Vec<_>>();
            append_or_overwrite_script_model(
                &mut plan,
                &live_plan,
                posed.key.clone(),
                posed.dobj_state.clone(),
                posed.camera_lods.clone(),
                &posed.surfaces,
                &materials,
            )
        };
        product_to_plan[product_index] = plan_index;
        live_plan.push(plan_index);
    }
    if product.surface_material_unbound != *last_unbound_materials {
        *last_unbound_materials = product.surface_material_unbound;
        diag::warn!(
            World,
            "script model surfaces without a bound material: {} (first {});              MapXModelSceneCatalog resolved={}",
            product.surface_material_unbound,
            product.surface_material_first.as_deref().unwrap_or("-"),
            u8::from(assets.materials_resolved()),
        );
    }
    keep.clear();
    keep.resize(plan.assets.len(), false);
    for &index in &live_plan {
        if let Some(slot) = keep.get_mut(index) {
            *slot = true;
        }
    }
    remap.clear();
    remap.resize(keep.len(), None);
    let mut next_index = 0usize;
    for (old_index, &is_live) in keep.iter().enumerate() {
        if is_live {
            remap[old_index] = Some(next_index);
            next_index += 1;
        }
    }
    retain_script_model_assets(&mut plan, &keep);

    for row in &product.rows {
        let Some(&plan_index) = product_to_plan.get(row.asset_index) else {
            if let Some(id) = row.focused_owner_id {
                focus.refuse(id, &row.model.0, "posed_asset_unavailable");
            }
            continue;
        };
        let Some(asset_index) = remap.get(plan_index).copied().flatten() else {
            if let Some(id) = row.focused_owner_id {
                focus.refuse(id, &row.model.0, "posed_asset_unavailable");
            }
            continue;
        };
        let surface_count = plan.assets[asset_index].surfaces.len();
        let pending_lighting = (!row.camera_hidden).then(|| {
            lighting_requests.request(ModelLightingRequest {
                owner: ModelLightingOwner::ScriptModel(row.entity),
                origin: row.lighting_origin,
                lookup_fallback: row.lookup_fallback,
            })
        });
        let object_id = (owners.len() as u16).saturating_add(0x200);
        if let Some(id) = row.focused_owner_id {
            focus.frame = Some(RenderFocusFrame {
                owner_id: id,
                model: Some(row.model.0.clone()),
                outcome: if surface_count == 0 {
                    "planned_empty"
                } else {
                    "planned"
                },
                object_id: Some(object_id),
                camera_hidden: Some(row.camera_hidden),
                lighting_handle: None,
                planned_surfaces: u32::try_from(surface_count).unwrap_or(u32::MAX),
            });
        }
        owners.push(ScriptModelOwnerDraw {
            entity: row.entity,
            current_model: row.model.clone(),
            object_id,
        });
        for &(surface, material) in &plan.assets[asset_index].surfaces {
            draws.push(XModelSurfaceDraw {
                surface,
                material,
                world_from_local: row.world_from_local,
                lighting_handle: 0,
                pending_lighting,
                colour_refusal: row
                    .camera_hidden
                    .then_some(XModelColourRefusal::CameraFrustum),
                object_id,
                scene_light_index: 0,
                reflection_probe_index: 0,
                packed_lighting: None,
                is_scope: false,
                scene_entnum: row.entnum,
                caster_bound: Some(row.caster_bound),
            });
        }
    }
    plan.publish_frame_rows(&mut draws, &mut owners);
}

fn append_or_overwrite_script_pose(
    product: &mut ScriptModelPoseProduct,
    live_assets: &[usize],
    key: assets::MapXModelAssetKey,
    dobj_state: assets::dobj::DObjSemanticState,
    camera_lods: Vec<Option<u8>>,
    surfaces: Vec<PosedModelSurface>,
    authored: Vec<Option<assets::MaterialIndex>>,
) -> usize {
    if let Some(index) = product
        .assets
        .iter()
        .enumerate()
        .find_map(|(index, asset)| {
            let matches = asset.key == key
                && asset.camera_lods == camera_lods
                && script_model_pose_topology_matches(&asset.dobj_state, &dobj_state)
                && !live_assets.contains(&index);
            matches.then_some(index)
        })
    {
        product.assets[index] = ScriptPosedAsset {
            key,
            dobj_state,
            camera_lods,
            surfaces,
            authored,
        };
        return index;
    }
    let index = product.assets.len();
    product.assets.push(ScriptPosedAsset {
        key,
        dobj_state,
        camera_lods,
        surfaces,
        authored,
    });
    index
}

fn script_dobj_reuse_key(state: &assets::dobj::DObjSemanticState) -> assets::dobj::DObjReuseKey {
    let mut parts = Vec::with_capacity(state.composition.models.len() * 2);
    for model in &state.composition.models {
        parts.push(model.model.as_str());
        parts.push(model.attach_tag.as_deref().unwrap_or(""));
    }
    assets::dobj::DObjReuseKey {
        e_type: entity_iw4::ET_SCRIPTMOVER,
        model: assets::dobj::dobj_model_token(&parts),
    }
}

fn append_or_overwrite_script_model(
    plan: &mut ScriptModelDrawPlan,
    live_assets: &[usize],
    key: assets::MapXModelAssetKey,
    dobj_state: assets::dobj::DObjSemanticState,
    camera_lods: Vec<Option<u8>>,
    surfaces: &[PosedModelSurface],
    materials: &[Option<SmodelPassMaterial>],
) -> usize {
    if let Some(index) = plan.assets.iter().enumerate().find_map(|(index, asset)| {
        let matches = asset.key == key
            && asset.camera_lods == camera_lods
            && script_model_pose_topology_matches(&asset.dobj_state, &dobj_state)
            && !live_assets.contains(&index);
        matches.then_some(index)
    }) && overwrite_script_model_asset(plan, index, dobj_state.clone(), surfaces, materials)
    {
        return index;
    }
    append_script_model_asset(plan, key, dobj_state, camera_lods, surfaces, materials)
}

fn script_model_pose_topology_matches(
    retained: &assets::dobj::DObjSemanticState,
    incoming: &assets::dobj::DObjSemanticState,
) -> bool {
    retained.composition == incoming.composition
        && retained.requested_parts == incoming.requested_parts
        && retained.hide_part_bits == incoming.hide_part_bits
        && match (&retained.tree, &incoming.tree) {
            (None, None) => true,
            (Some(retained), Some(incoming)) => {
                retained.definition_revision == incoming.definition_revision
                    && retained.nodes.len() == incoming.nodes.len()
                    && retained.nodes.iter().zip(&incoming.nodes).all(|(a, b)| {
                        a.parent == b.parent
                            && a.kind == b.kind
                            && a.clip == b.clip
                            && a.parts == b.parts
                    })
            }
            _ => false,
        }
}

pub(crate) fn presented_skel_arcs(
    catalog: &assets::MapXModelSceneCatalog,
    state: &assets::dobj::DObjSemanticState,
) -> Option<Vec<std::sync::Arc<assets::ModelSkel>>> {
    let mut skels = Vec::with_capacity(state.composition.models.len());
    for descriptor in &state.composition.models {
        let skel = match catalog.get_name(&descriptor.model)? {
            assets::MapXModelSceneAsset::Iw4(skel)
            | assets::MapXModelSceneAsset::Iw5(skel)
            | assets::MapXModelSceneAsset::T5(skel) => std::sync::Arc::clone(skel),
            assets::MapXModelSceneAsset::Unavailable { .. } => return None,
        };
        if skel.pose.is_none() {
            return None;
        }
        skels.push(skel);
    }
    Some(skels)
}

pub(crate) fn presented_skels<'a>(
    catalog: &'a assets::MapXModelSceneCatalog,
    state: &assets::dobj::DObjSemanticState,
) -> Option<Vec<&'a assets::ModelSkel>> {
    let mut skels = Vec::with_capacity(state.composition.models.len());
    for descriptor in &state.composition.models {
        let skel = match catalog.get_name(&descriptor.model)? {
            assets::MapXModelSceneAsset::Iw4(skel)
            | assets::MapXModelSceneAsset::Iw5(skel)
            | assets::MapXModelSceneAsset::T5(skel) => skel.as_ref(),
            assets::MapXModelSceneAsset::Unavailable { .. } => return None,
        };
        if skel.pose.is_none() {
            return None;
        }
        skels.push(skel);
    }
    Some(skels)
}

pub fn collect_presented_models<'a>(
    catalog: &'a assets::MapXModelSceneCatalog,
    state: &assets::dobj::DObjSemanticState,
) -> Option<(
    Vec<(&'a assets::ModelPoseSrc, Option<assets::Attach>)>,
    Vec<&'a assets::ModelSkel>,
)> {
    let mut skels = Vec::with_capacity(state.composition.models.len());
    let mut attaches = Vec::with_capacity(state.composition.models.len());
    for (index, descriptor) in state.composition.models.iter().enumerate() {
        let skel = match catalog.get_name(&descriptor.model)? {
            assets::MapXModelSceneAsset::Iw4(skel)
            | assets::MapXModelSceneAsset::Iw5(skel)
            | assets::MapXModelSceneAsset::T5(skel) => skel,
            assets::MapXModelSceneAsset::Unavailable { .. } => return None,
        };
        let attach = match (descriptor.parent_model, descriptor.attach_tag.as_ref()) {
            (None, None) if index == 0 => None,
            (Some(parent), Some(tag)) if usize::from(parent) < index => Some(assets::Attach {
                parent_model: usize::from(parent),
                tag: tag.clone(),
            }),
            _ => return None,
        };
        skels.push(skel.as_ref());
        attaches.push(attach);
    }
    let models = skels
        .iter()
        .zip(&attaches)
        .map(|(skel, attach)| Some((skel.pose.as_ref()?, attach.clone())))
        .collect::<Option<Vec<_>>>()?;
    Some((models, skels))
}

fn compose_or_reuse_script_dobj(
    persist: &mut ScriptModelDobjs,
    id: u32,
    catalog: &assets::MapXModelSceneCatalog,
    state: &assets::dobj::DObjSemanticState,
) -> Option<()> {
    let key = script_dobj_reuse_key(state);
    let reuse = persist
        .by_id
        .get(&id)
        .is_some_and(|slot| assets::dobj::dobj_reuse_matches(slot.reuse_key, key));
    if reuse {
        return Some(());
    }
    let (specs, _) = collect_presented_models(catalog, state)?;
    let dobj = assets::DObj::build(&specs).ok()?;
    persist.by_id.insert(
        id,
        PersistentScriptDobj {
            dobj,
            reuse_key: key,
        },
    );
    Some(())
}

pub fn pose_script_dobj_with_materials(
    catalog: Option<&assets::MapXModelSceneCatalog>,
    skels: &[&assets::ModelSkel],
    dobj: &assets::DObj,
    request: &assets::dobj::DObjPoseRequest,
    lod_view: Option<DObjLodView>,
    skin_entries: &[dpvs_iw4::SceneEntSkinEntry],
) -> Option<(Vec<PosedModelSurface>, Vec<Option<assets::MaterialIndex>>)> {
    let world = assets::dobj::pose_dobj(dobj, request, Mat4::IDENTITY).ok()?;
    let skin = dobj.skin_matrices(&world);
    let mut surfaces = Vec::new();
    let mut materials = Vec::new();
    for (model, skel) in skels.iter().enumerate() {
        let Some(lod) = submodel_camera_lod(skel, lod_view) else {
            continue;
        };
        if skel.surfaces_for_lod(lod).is_empty() {
            continue;
        }
        let base = dobj.models.get(model)?.base;
        let posed = skin_model_filtered(
            skel,
            |bone| skin[base + bone],
            &[],
            |surface_index| {
                dobj.surface_visible(
                    model,
                    &skel.surface_part_bits[surface_index],
                    &request.hide_part_bits,
                ) && dpvs_iw4::SceneEntSkinEntry::stream_draws(
                    skin_entries,
                    model as u16,
                    lod_local_surface(skel, lod, surface_index) as u16,
                )
            },
            |surface_index| {
                stream_lod_surface_rigid(skel, lod, skin_entries, model as u16, surface_index)
            },
            lod,
        )?;
        let key = assets::MapXModelAssetKey(skel.name.clone());
        let mut posed = posed;
        for surface in &mut posed {
            surface.model = model as u16;
        }
        for surface in &posed {
            materials.push(
                catalog.and_then(|catalog| catalog.surface_material(&key, surface.surface_index)),
            );
        }
        surfaces.extend(posed);
    }
    Some((surfaces, materials))
}

#[derive(Clone, Copy, Debug)]
pub struct DObjLodView {
    pub origin: [f32; 3],
    pub eye: Option<Vec3>,
    pub ramp: LodRampArgs,
}

pub(crate) fn submodel_camera_lod(
    skel: &assets::ModelSkel,
    view: Option<DObjLodView>,
) -> Option<u8> {
    let Some(view) = view else {
        return Some(0);
    };
    smodel_camera_lod(skel.lod, view.origin, 1.0, view.eye, view.ramp)
}

fn dobj_lod_culled(
    skels: &[&assets::ModelSkel],
    origin: [f32; 3],
    eye: Option<[f32; 3]>,
    ramp: LodRampArgs,
) -> bool {
    let Some(eye) = eye else {
        return false;
    };
    if skels.is_empty() {
        return false;
    }
    skels.iter().all(|skel| {
        smodel_camera_lod(skel.lod, origin, 1.0, Some(Vec3::from_array(eye)), ramp).is_none()
    })
}
