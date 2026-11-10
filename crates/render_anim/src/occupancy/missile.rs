use frame::ScopeApp;

use bevy::prelude::*;

use crate::anim::scene_submission::{AnimDObjSceneSkels, AnimDObjSceneSubmission, AnimSceneSubmit};
use crate::anim::xmodel_pose::PosedModelSurface;
use crate::occupancy::script_model::pose_script_dobj_with_materials;
use crate::{
    MissileDrawPlan, MissileOwnerDraw, XMODEL_OBJECT_ID_MISSILE_BASE, append_missile_surfaces,
    lighting_box_half,
};
use anim_iw4::DOBJ_RADIUS_PARENT_ROOT;
use render_scene::{
    HostGfxScene, ModelLightingOwner, ModelLightingRequest, ModelLightingRequests,
    SmodelPassMaterial, TessMaterials, WorldModelLightingAtlas, WorldPresentFacts,
    WorldScriptModelInstance, XModelSurfaceDraw, scene_quat_from_angles,
};

pub const MISSILE_LIGHTING_Z_OFS: f32 = 4.0;

#[derive(Resource, Default)]
pub struct MissileOccupancy {
    pub rows: Vec<OccupiedMissile>,
}

pub struct OccupiedMissile {
    pub index: usize,
    pub entnum: Option<u32>,
    pub weapon: u32,
    pub ignited: bool,
    pub origin: [f32; 3],
    pub angles: [f32; 3],
    pub name: String,
    pub namespace: asset_core::FamilyId,
    pub lighting_origin: [f32; 3],
    pub lighting_owner: ModelLightingOwner,
}

#[derive(Resource, Default)]
struct MissilePoseProduct {
    rows: Vec<MissilePosed>,
}

struct MissilePosed {
    index: usize,
    entnum: Option<u32>,
    origin: [f32; 3],
    angles: [f32; 3],
    name: String,
    namespace: asset_core::FamilyId,
    lighting_origin: [f32; 3],
    lighting_owner: ModelLightingOwner,
    surfaces: Vec<PosedModelSurface>,
}

struct MissileSceneSlot<'a> {
    hidden: bool,
    skin_entries: &'a [dpvs_iw4::SceneEntSkinEntry],
}

fn missile_scene_slot(scene: &render_scene::GfxScene, entnum: u32) -> MissileSceneSlot<'_> {
    MissileSceneSlot {
        hidden: scene.scene_ent_skips_draw(entnum),
        skin_entries: scene
            .scene_ent_skinned_surfs(entnum)
            .map(|surfs| surfs.entries.as_slice())
            .unwrap_or(&[]),
    }
}

#[derive(Component, Default)]
pub struct MissileEffects {
    pub trail_played: bool,
    pub beacon_played: bool,
    pub ignition_played: bool,
    pub ignition_fx_played: bool,
}

#[derive(SystemSet, Debug, Clone, PartialEq, Eq, Hash)]
pub struct MissilePosesPublished;

pub fn register_missile_systems(app: &mut App) {
    app.scoped::<MissileDrawPlan>(frame::MatchScope::Live)
        .scoped::<MissileOccupancy>(frame::MatchScope::Live)
        .scoped::<MissilePoseProduct>(frame::MatchScope::Live)
        .add_systems(
            Update,
            (occupy_missile_scene_ents
                .in_set(frame::InMatch)
                .in_set(frame::RenderSet::Anim)
                .before(frame::WorkerCmdSet::DynamicModels)
                .in_set(render_scene::GfxSceneAdd)
                .in_set(AnimSceneSubmit))
            .in_set(frame::InMatch),
        )
        .add_systems(
            Update,
            (publish_missile_dobj_poses
                .in_set(frame::InMatch)
                .after(occupy_missile_scene_ents)
                .after(frame::WorkerCmdSet::ModelPose)
                .in_set(MissilePosesPublished)
                .in_set(frame::ClientSet::Present))
            .in_set(frame::InMatch),
        )
        .add_systems(
            Update,
            ((
                pose_missiles.in_set(frame::InMatch),
                append_missile_draws.in_set(frame::InMatch),
            )
                .chain()
                .after(frame::WorkerCmdSet::SceneVisibility)
                .before(frame::WorkerCmdSet::DynamicModels)
                .in_set(frame::WorkerCmdSet::ModelPose))
            .in_set(frame::InMatch),
        );
}

pub fn missile_lighting_origin(origin: [f32; 3]) -> [f32; 3] {
    [origin[0], origin[1], origin[2] + MISSILE_LIGHTING_Z_OFS]
}

pub fn missile_pose_catalog(
    assets: Option<&assets::PreparedProjectileMeshes>,
) -> Option<&asset_model::ProjectileMeshCatalog> {
    assets
        .map(|prepared| &prepared.0)
        .filter(|catalog| !catalog.is_empty())
}

pub fn missile_lighting_atlas<'a>(
    atlas: Option<&'a WorldModelLightingAtlas>,
    scene_atlas: Option<&'a WorldModelLightingAtlas>,
) -> Option<&'a WorldModelLightingAtlas> {
    atlas.or(scene_atlas)
}

#[derive(bevy::ecs::system::SystemParam)]
struct MissileSceneProducer<'w, 's> {
    level: Option<Res<'w, net::ClientLevel>>,
    occupancy: ResMut<'w, MissileOccupancy>,
    scene_skels: ResMut<'w, AnimDObjSceneSkels>,
    scene_submissions: MessageWriter<'w, AnimDObjSceneSubmission>,
    projectiles: Query<
        'w,
        's,
        (
            Entity,
            &'static net::ProjectileMotion,
            Option<&'static net::CEntity>,
        ),
    >,
    script_models: Query<
        'w,
        's,
        (
            Entity,
            &'static WorldScriptModelInstance,
            &'static Transform,
            &'static Visibility,
        ),
    >,
}

impl MissileSceneProducer<'_, '_> {
    fn produce(
        &mut self,
        presented: Option<Res<net::PresentedSnapshot>>,
        weapons: Option<Res<assets::PreparedWeapons>>,
        projectile_meshes: Option<Res<assets::PreparedProjectileMeshes>>,
        cg_clock: Option<Res<net::FrameClock>>,
        local: Option<Res<net::LocalPresentClient>>,
    ) {
        self.occupancy.rows.clear();
        let Some(snapshot) = presented.as_ref() else {
            return;
        };
        let mut rows: Vec<_> = self.projectiles.iter().collect();
        rows.sort_by_key(|(entity, _, identity)| {
            (identity.map_or(u16::MAX, |id| id.number()), *entity)
        });
        let at_time = cg_clock
            .as_ref()
            .filter(|clock| clock.started())
            .map(|clock| clock.time())
            .unwrap_or_else(|| snapshot.tick().map(sim::level_time_ms).unwrap_or(0));
        let at_time = snapshot.trajectory_time_ms(at_time);
        let bound = weapons.as_deref().map(|weapons| weapons.bound());
        let weapons_reg = bound
            .as_ref()
            .map(|w| w.registry())
            .filter(|reg| !reg.is_empty());
        let catalog = missile_pose_catalog(projectile_meshes.as_deref());
        let Some(catalog) = catalog else {
            return;
        };
        let piloted = local
            .and_then(|local| {
                snapshot
                    .snapshot()?
                    .meta
                    .for_client(local.0)?
                    .remote_missile
            })
            .map(|link| link.projectile);
        for (index, (entity, motion, identity)) in rows.iter().enumerate() {
            let entnum = identity.map(|identity| identity.number());
            let authoritative = entnum.and_then(|number| snapshot.projectile(number));
            if entity_iw4::missile_nodraw(0, motion.launch_time, at_time).is_some() {
                continue;
            }
            if piloted.is_some() && authoritative.map(|p| p.id) == piloted {
                continue;
            }
            let Some((ns, name)) =
                weapons_reg.and_then(|reg| reg.projectile_model_reference_of(motion.weapon))
            else {
                continue;
            };
            let Some(entry) = catalog.get(ns, name) else {
                continue;
            };
            let Some(_pose) = entry.skel.pose.as_ref() else {
                continue;
            };
            let origin = match authoritative {
                Some(projectile) => snapshot.projectile_origin_at(projectile, at_time),
                None => motion.origin_at(at_time),
            };
            let angles = entity_iw4::evaluate_trajectory(&motion.apos, at_time);
            let lighting_origin = missile_lighting_origin(origin);
            let pose = render_scene::place_simulation_pose(
                self.level
                    .as_ref()
                    .and_then(|level| level.content.atlas())
                    .map(|atlas| &atlas.space),
                origin,
                angles,
            );
            let (origin, angles) = (pose.origin, pose.angles);
            let entnum = entnum.map(u32::from);
            if let Some(entnum) = entnum {
                self.scene_submissions.write(AnimDObjSceneSubmission {
                    render_fx_flags: 0,
                    has_tree: false,
                    origin,
                    lighting_origin,
                    radius: entry.skel.radius,
                    entnum,
                    quat: Some(scene_quat_from_angles(angles)),
                    occupy_model_n: 1,
                    models: vec![self.scene_skels.shared(name, &entry.skel, 0)],
                    hide_part_bits: [0; 6],
                    store_skin: true,
                });
            }
            self.occupancy.rows.push(OccupiedMissile {
                index,
                entnum,
                weapon: motion.weapon,
                ignited: weapons_reg
                    .and_then(|reg| reg.bind_published_row(motion.weapon).ok())
                    .and_then(|weapon| weapon.event_facts())
                    .is_none_or(|facts| {
                        at_time >= motion.spawn_time_ms.saturating_add(facts.ignition_delay_ms)
                    }),
                origin,
                angles,
                name: name.to_owned(),
                namespace: ns,
                lighting_origin,
                lighting_owner: ModelLightingOwner::Missile(*entity),
            });
        }
        for (entity, owner, transform, visibility) in &self.script_models {
            if *visibility == Visibility::Hidden {
                continue;
            }
            let Some(entnum) = owner.gentity_number.map(u32::from) else {
                continue;
            };
            let Some((namespace, name, entry)) = owner
                .dobj_state
                .composition
                .models
                .iter()
                .skip(1)
                .find_map(|attached| {
                    let weapon = sim::weapon_model_attachment(&attached.model)?;
                    let reg = weapons_reg?;
                    let (ns, name) = reg.projectile_model_reference_of(weapon)?;
                    Some((ns, name, catalog.get(ns, name)?))
                })
            else {
                continue;
            };
            if entry.skel.pose.is_none() {
                continue;
            }
            let origin = transform.translation.to_array();
            let (yaw, pitch, roll) = transform.rotation.to_euler(EulerRot::ZYX);
            let angles = [pitch, yaw, roll].map(f32::to_degrees);
            let lighting_origin = missile_lighting_origin(origin);
            self.scene_submissions.write(AnimDObjSceneSubmission {
                render_fx_flags: 0,
                has_tree: false,
                origin,
                lighting_origin,
                radius: entry.skel.radius,
                entnum,
                quat: Some(scene_quat_from_angles(angles)),
                occupy_model_n: 1,
                models: vec![self.scene_skels.shared(name, &entry.skel, 0)],
                hide_part_bits: [0; 6],
                store_skin: true,
            });
            let index = rows.len() + self.occupancy.rows.len();
            self.occupancy.rows.push(OccupiedMissile {
                index,
                entnum: Some(entnum),
                weapon: 0,
                ignited: false,
                origin,
                angles,
                name: name.to_owned(),
                namespace,
                lighting_origin,
                lighting_owner: ModelLightingOwner::ScriptModel(entity),
            });
        }
    }
}

fn occupy_missile_scene_ents(
    mut producer: MissileSceneProducer,
    presented: Option<Res<net::PresentedSnapshot>>,
    weapons: Option<Res<assets::PreparedWeapons>>,
    projectile_meshes: Option<Res<assets::PreparedProjectileMeshes>>,
    cg_clock: Option<Res<net::FrameClock>>,
    local: Option<Res<net::LocalPresentClient>>,
) {
    producer.produce(presented, weapons, projectile_meshes, cg_clock, local);
}

fn publish_missile_dobj_poses(
    occupancy: Res<MissileOccupancy>,
    prepared: Res<crate::anim::model_materials::PreparedModelMaterials>,
    mut dobj_poses: ResMut<crate::anim::dobj_pose::HostDObjPoseFrame>,
) {
    for row in &occupancy.rows {
        let Some(entnum) = row.entnum else {
            continue;
        };
        let Some(dobj) = prepared.projectile_dobj(row.namespace, &row.name) else {
            continue;
        };
        let entity_world = missile_world_from_local(row.origin, row.angles);
        let Ok(local) = xmodel_runtime::pose_dobj_with_controller(
            dobj,
            &xmodel_runtime::DObjPoseRequest::bind_pose(),
            Mat4::IDENTITY,
            |_, _, _| {},
        ) else {
            continue;
        };

        let _ = dobj_poses.publish(entnum, true, 0, entity_world, &local);
    }
}

fn pose_missiles(
    occupancy: Res<MissileOccupancy>,
    projectile_meshes: Option<Res<assets::PreparedProjectileMeshes>>,
    gfx: Res<HostGfxScene>,
    prepared: Res<crate::anim::model_materials::PreparedModelMaterials>,
    mut product: ResMut<MissilePoseProduct>,
    mut last_probe: Local<Option<(usize, bool, usize)>>,
) {
    product.rows.clear();
    let catalog = missile_pose_catalog(projectile_meshes.as_deref());
    let Some(catalog) = catalog else {
        return;
    };
    for row in &occupancy.rows {
        let slot = row
            .entnum
            .map(|entnum| missile_scene_slot(&gfx.scene, entnum))
            .unwrap_or(MissileSceneSlot {
                hidden: false,
                skin_entries: &[],
            });
        let probe = (
            sim::world_graph::world_at(row.origin),
            slot.hidden,
            slot.skin_entries.len(),
        );
        if *last_probe != Some(probe) {
            diag::info!(
                World,
                "missile pose: ent={:?} world={} hidden={} skin_entries={} origin={:?}",
                row.entnum,
                probe.0,
                probe.1,
                probe.2,
                row.origin
            );
            *last_probe = Some(probe);
        }
        if slot.hidden {
            continue;
        }
        let Some(entry) = catalog.get(row.namespace, &row.name) else {
            continue;
        };
        let Some(dobj) = prepared.projectile_dobj(row.namespace, &row.name) else {
            continue;
        };
        let dobj_state = xmodel_runtime::DObjSemanticState::bind_pose(row.name.clone(), 1, 1);
        let Ok(request) = dobj_state.resolve_request(|_| None) else {
            continue;
        };
        let Some((surfaces, _)) = pose_script_dobj_with_materials(
            None,
            &[&entry.skel],
            &[],
            dobj,
            &request,
            None,
            slot.skin_entries,
        ) else {
            continue;
        };
        product.rows.push(MissilePosed {
            index: row.index,
            entnum: row.entnum,
            origin: row.origin,
            angles: row.angles,
            name: row.name.clone(),
            namespace: row.namespace,
            lighting_origin: row.lighting_origin,
            lighting_owner: row.lighting_owner,
            surfaces,
        });
    }
}

#[derive(bevy::ecs::system::SystemParam)]
struct MissileDrawPublisher<'w, 's> {
    plan: ResMut<'w, MissileDrawPlan>,
    lighting_requests: ResMut<'w, ModelLightingRequests>,
    atpoint: Res<'w, render_scene::DynAtPointLookup>,
    staged: Local<'s, MissileDrawPlan>,
    draws: Local<'s, Vec<XModelSurfaceDraw>>,
    owners: Local<'s, Vec<MissileOwnerDraw>>,
}

impl MissileDrawPublisher<'_, '_> {
    fn publish(
        mut self,
        product: Res<MissilePoseProduct>,
        projectile_meshes: Option<Res<assets::PreparedProjectileMeshes>>,
        atlas: Option<Res<WorldModelLightingAtlas>>,
        tess: Option<Res<TessMaterials>>,
        facts: Res<WorldPresentFacts>,
        prepared: Res<crate::anim::model_materials::PreparedModelMaterials>,
    ) {
        // Missiles are rebuilt from scratch every frame, so the rebuild goes to a
        // staging plan whose allocations survive the frame; `plan` keeps the
        // revisions, which is the part a consumer reads.
        let staging = &mut *self.staged;
        staging.clear_rebuild();
        self.draws.clear();
        self.owners.clear();
        let catalog = missile_pose_catalog(projectile_meshes.as_deref());
        let atlas_ref = missile_lighting_atlas(atlas.as_deref(), None);
        if catalog.is_none() || atlas_ref.is_none() || !facts.spawned || tess.is_none() {
            self.plan
                .publish_rebuild(staging, &mut self.draws, &mut self.owners);
            return;
        }
        let (Some(catalog), Some(_), Some(tess)) = (catalog, atlas_ref, tess.as_deref()) else {
            unreachable!("required missile draw resources checked above");
        };
        for row in &product.rows {
            let Some(entry) = catalog.get(row.namespace, &row.name) else {
                continue;
            };
            let materials: Vec<Option<SmodelPassMaterial>> = row
                .surfaces
                .iter()
                .map(|surface| {
                    let authored = entry.material_index(surface.surface_index)?;
                    prepared
                        .projectile_material(tess.catalog(), authored)
                        .cloned()
                })
                .collect();
            if materials.iter().all(Option::is_none) {
                continue;
            }
            let box_half = entry
                .skel
                .radius
                .and_then(|radius| lighting_box_half(&[radius], &[DOBJ_RADIUS_PARENT_ROOT]));
            let lookup_fallback = self.atpoint.fallback(row.lighting_origin, box_half);
            let pending_lighting = Some(self.lighting_requests.request(ModelLightingRequest {
                owner: row.lighting_owner,
                origin: row.lighting_origin,
                lookup_fallback,
            }));
            let surfaces_idx = append_missile_surfaces(staging, &row.surfaces, &materials);
            if surfaces_idx.is_empty() {
                continue;
            }
            let object_id = XMODEL_OBJECT_ID_MISSILE_BASE.saturating_add(row.index as u16);
            self.owners.push(MissileOwnerDraw {
                object_id,
                model: row.name.clone(),
            });
            let world_from_local = missile_world_from_local(row.origin, row.angles);
            let caster_bound = entry
                .skel
                .radius
                .map(|radius| render_scene::XModelCasterBound {
                    origin: row.origin,
                    radius: radius.max(1.0),
                });
            for (surface, material) in surfaces_idx {
                self.draws.push(XModelSurfaceDraw {
                    surface,
                    material,
                    world_from_local,
                    lighting_handle: 0,
                    pending_lighting,
                    colour_refusal: None,
                    object_id,
                    scene_light_index: 0,
                    reflection_probe_index: 0,
                    packed_lighting: None,
                    is_scope: false,
                    scene_entnum: row.entnum,
                    body_client: None,
                    caster_bound,
                });
            }
        }
        self.plan
            .publish_rebuild(staging, &mut self.draws, &mut self.owners);
    }
}

fn append_missile_draws(
    publisher: MissileDrawPublisher,
    product: Res<MissilePoseProduct>,
    projectile_meshes: Option<Res<assets::PreparedProjectileMeshes>>,
    atlas: Option<Res<WorldModelLightingAtlas>>,
    tess: Option<Res<TessMaterials>>,
    facts: Res<WorldPresentFacts>,
    prepared: Res<crate::anim::model_materials::PreparedModelMaterials>,
) {
    publisher.publish(product, projectile_meshes, atlas, tess, facts, prepared);
}

pub(crate) fn missile_world_from_local(origin: [f32; 3], angles: [f32; 3]) -> Mat4 {
    let [pitch, yaw, roll] = angles;
    Transform {
        translation: Vec3::from_array(origin),
        rotation: Quat::from_euler(
            EulerRot::ZYX,
            yaw.to_radians(),
            pitch.to_radians(),
            roll.to_radians(),
        ),
        scale: Vec3::ONE,
    }
    .to_matrix()
}
