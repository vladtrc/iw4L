use crate::anim::remote_body::{
    AdvancedRemoteTree, AssembledRemoteBody, CpuBodyGeom, CpuSurfMeta, PendingGunSkin,
    PreparedRemoteKits, RemoteAnimTracks, RemoteBodySkinnedItem, RemoteBodySkinnedQueue,
    RemoteBodyState, RemoteGeometryRevision, SkinAfterPose, WorldGunGap, advance_remote_tree,
    bind_remote_skin_models, ensure_remote_dobj, hash_skin_matrices, occupy_lod_byte,
    occupy_remote_kit_dobj, pose_remote_dobj, push_cached_surfaces, radii,
    remote_player_controller, select_remote_lods, select_remote_models, skin_after_pose,
    skin_slot_need, skip_frozen_corpse_dobj, take_unique_geom, validate_remote_tracks,
};
use crate::anim::scene_submission::{AnimDObjSceneSkels, AnimDObjSceneSubmission, AnimSceneSubmit};
use crate::anim::world_weapon::PreparedItemCompositions;
use crate::anim::xmodel_pose::{build_skin_layout, stream_lod_surface_rigid};
use crate::lighting_box_half;
use crate::{
    RemoteBodyDrawPlan, append_remote_body_cpu_blob, finish_remote_body_draw_plan,
    install_body_packed_session, push_remote_body_cpu_draw, take_body_packed_session,
    topology_fingerprint,
};
use anim_iw4::{PLAYER_ANIM_RAW_MASK, PlayerAnimValue};
use asset_model::ModelSkel;
use assets::{PreparedBodies, PreparedWeapons, PreparedWorldWeapons};
use bevy::ecs::query::QueryData;
use bevy::prelude::*;
use bevy::tasks::ComputeTaskPool;
use entity_iw4::{ET_PLAYER, ET_PLAYER_CORPSE};
use frame::ScopeApp;
use frame::{ModelLightingSeated, ViewSubject, WorkerCmdSet};
use net::{
    CEntity, CEntityRuntime, ClientSet, FrameClock, PlayerDrawGate, PresentedPublished,
    remote_body_submits, remote_pose_sample,
};
use render_scene::HostGfxScene;
use render_scene::WorldModelLightingAtlas;
use render_scene::WorldScriptModelInstance;
use render_scene::{FlyCamera, FpvLens};
use render_scene::{LodRampArgs, smodel_camera_lod};
use render_scene::{
    ModelLightingOwner, ModelLightingRequest, ModelLightingRequests, ResolvedModelLighting,
    ResolvedModelLightingTable,
};
use std::borrow::Cow;
use std::collections::HashMap;
use std::hash::{Hash, Hasher};
use std::sync::Arc;

#[derive(Component, Debug, Clone, Copy)]
#[require(RemoteFxBolts, RemoteBodyState)]
pub struct RemotePlayer {
    pub ffa_team: Option<u8>,

    pub client_state_team: i32,
    pub shield: Option<sim::ShieldAttachment>,
}

fn drawn_shield(
    snapshot: &sim::Snapshot,
    number: u16,
    client: sim::ClientId,
) -> Option<sim::ShieldAttachment> {
    let body = snapshot
        .meta
        .corpses
        .slots
        .iter()
        .find(|slot| slot.occupied && slot.entnum == i32::from(number));
    match body {
        Some(body) => body.shield,
        None => snapshot
            .meta
            .for_client(client)
            .and_then(|meta| meta.shield),
    }
}

#[derive(Component, Default)]
pub struct RemoteFxBolts {
    pub flash: Option<fx::FxBoltTarget>,

    pub brass: Option<fx::FxBoltTarget>,

    pub knife: Option<fx::FxBoltTarget>,

    pub laser: Option<fx::FxBoltTarget>,
}

#[derive(Resource, Default)]
struct RemoteBodyLightingBinds {
    by_client: HashMap<u32, ModelLightingRequest>,
}

#[derive(Hash, PartialEq, Eq)]
struct BodySkinShapeKey {
    world_weapon: bool,
    product_revision: u64,
    namespace: asset_core::FamilyId,
    name: String,
    lod: u8,
}

#[derive(Resource, Default)]
struct BodySkinShapes {
    bodies: Option<Arc<asset_model::BodyMeshCatalog>>,
    weapons: Option<Arc<asset_model::WorldWeaponCatalog>>,
    shapes:
        std::sync::Mutex<HashMap<BodySkinShapeKey, Arc<render_frame::skinning::PackedSkinShape>>>,
}

impl BodySkinShapes {
    fn prepare(&mut self, bodies: &PreparedBodies, weapons: Option<&PreparedWorldWeapons>) {
        let same_bodies = self
            .bodies
            .as_ref()
            .is_some_and(|old| old.identity() != 0 && old.identity() == bodies.0.identity());
        let same_weapons = match (&self.weapons, weapons) {
            (Some(old), Some(new)) => old.identity() != 0 && old.identity() == new.0.identity(),
            (None, None) => true,
            _ => false,
        };
        let shapes = self
            .shapes
            .get_mut()
            .expect("body skin cache is never poisoned");
        if !same_bodies || !same_weapons {
            shapes.clear();
            self.bodies = Some(Arc::clone(&bodies.0));
            self.weapons = weapons.map(|owner| Arc::clone(&owner.0));
        }
    }

    fn shape(
        &self,
        skel: &ModelSkel,
        namespace: asset_core::FamilyId,
        world_weapon: bool,
        lod: u8,
        layout: &crate::anim::xmodel_pose::SkinLayout,
    ) -> Arc<render_frame::skinning::PackedSkinShape> {
        let product_revision = if world_weapon {
            self.weapons
                .as_ref()
                .map_or(0, |weapons| weapons.identity())
        } else {
            self.bodies.as_ref().map_or(0, |bodies| bodies.identity())
        };
        let key = BodySkinShapeKey {
            world_weapon,
            product_revision,
            namespace,
            name: skel.name.clone(),
            lod,
        };
        let mut shapes = self
            .shapes
            .lock()
            .expect("body skin cache is never poisoned");
        Arc::clone(shapes.entry(key).or_insert_with(|| {
            crate::anim::xmodel_pose::packed_skin_shape(skel, lod, layout, |_| false)
        }))
    }
}

pub fn register_remote_body_systems(app: &mut App) {
    app.add_observer(retire_remote_body);
    app.scoped::<RemoteBodyDrawPlan>(frame::MatchScope::Live)
        .scoped::<RemoteGeometryRevision>(frame::MatchScope::Live)
        .scoped::<RemoteBodySkinnedQueue>(frame::MatchScope::Live)
        .scoped::<RemoteBodyLightingBinds>(frame::MatchScope::Live)
        .scoped::<BodySkinShapes>(frame::MatchScope::Live)
        .scoped::<PreparedRemoteKits>(frame::MatchScope::Live)
        .add_systems(
            PreUpdate,
            (reset_remote_kits
                .in_set(frame::InMatch)
                .in_set(ClientSet::Load))
            .in_set(frame::InMatch),
        )
        .add_systems(
            Update,
            (prepare_remote_kits
                .in_set(frame::InMatch)
                .after(sync_remote_bodies)
                .before(occupy_remote_scene_ents)
                .before(pose_remote_bodies)
                .in_set(render_scene::GfxSceneAdd))
            .in_set(frame::InMatch),
        )
        .scoped::<crate::anim::dobj_pose::HostDObjPoseFrame>(frame::MatchScope::Live)
        .scoped::<crate::anim::dobj_pose::PosedPlayerFrame>(frame::MatchScope::Live)
        .add_systems(
            Update,
            (sync_remote_bodies
                .after(PresentedPublished)
                .before(occupy_remote_scene_ents)
                .in_set(render_scene::GfxSceneAdd))
            .in_set(frame::InMatch),
        )
        .add_systems(
            Update,
            (occupy_remote_scene_ents
                .in_set(frame::InMatch)
                .after(sync_remote_bodies)
                .after(PresentedPublished)
                .after(crate::occupancy::fpv_present::occupy_fpv_scene)
                .in_set(render_scene::GfxSceneAdd)
                .in_set(AnimSceneSubmit))
            .in_set(frame::InMatch),
        )
        .add_systems(
            Update,
            ((
                copy_corpse_animation
                    .after(sync_remote_bodies)
                    .before(pose_remote_bodies),
                begin_remote_body_tess
                    .after(sync_remote_bodies)
                    .before(pose_remote_bodies),
                pose_remote_bodies
                    .in_set(frame::InMatch)
                    .after(sync_remote_bodies)
                    .after(begin_remote_body_tess)
                    .after(crate::anim::dobj_pose::begin_dobj_pose_frame)
                    .after(WorkerCmdSet::WorldVisibility)
                    .after(WorkerCmdSet::SceneVisibility),
            )
                .after(PresentedPublished)
                .in_set(WorkerCmdSet::ModelPose))
            .in_set(frame::InMatch),
        )
        .add_systems(
            Update,
            (crate::anim::dobj_pose::begin_dobj_pose_frame
                .in_set(frame::InMatch)
                .after(PresentedPublished)
                .in_set(ClientSet::Present))
            .in_set(frame::InMatch),
        )
        .add_systems(
            Update,
            (enqueue_remote_body_lighting
                .in_set(frame::InMatch)
                .after(WorkerCmdSet::ModelPose)
                .after(crate::occupancy::script_model::ScriptModelSkinSet)
                .before(WorkerCmdSet::DynamicModels)
                .after(PresentedPublished)
                .in_set(ClientSet::Present))
            .in_set(frame::InMatch),
        )
        .add_systems(
            Update,
            ((
                submit_remote_bodies
                    .in_set(frame::InMatch)
                    .after(ModelLightingSeated),
                finish_body_draw_plan
                    .in_set(frame::InMatch)
                    .after(submit_remote_bodies),
            )
                .in_set(ClientSet::Present))
            .in_set(frame::InMatch),
        );
}

fn reset_remote_kits(
    bodies: Option<Res<PreparedBodies>>,
    weapons: Option<Res<PreparedWeapons>>,
    world_weapons: Option<Res<PreparedWorldWeapons>>,
    mut kits: ResMut<PreparedRemoteKits>,
) {
    let (Some(bodies), Some(weapons), Some(world)) = (bodies, weapons, world_weapons) else {
        kits.clear();
        return;
    };
    kits.reset_for(&bodies, &weapons, &world);
}

fn prepare_remote_kits(
    bodies: Option<Res<PreparedBodies>>,
    soldiers: Option<Res<asset_game::SoldierPresentations>>,
    weapons: Option<Res<PreparedWeapons>>,
    world_weapons: Option<Res<PreparedWorldWeapons>>,
    mut compositions: ResMut<PreparedItemCompositions>,
    mut kits: ResMut<PreparedRemoteKits>,
    remotes: Query<(&CEntityRuntime, &RemotePlayer)>,
) {
    let (Some(bodies), Some(soldiers), Some(weapons), Some(world)) =
        (bodies, soldiers, weapons, world_weapons)
    else {
        kits.clear();
        compositions.clear();
        return;
    };
    let mut batch = kits.prepare_batch(&bodies, &weapons, &world);
    compositions.reset_for(&weapons, &world);
    let owner = weapons.bound();
    for (runtime, remote) in &remotes {
        let axis = asset_model::kit_assignment_is_axis(remote.client_state_team, remote.ffa_team);
        let sample = remote_pose_sample(runtime);
        let Some(weapon) = owner.row(sample.weapon) else {
            continue;
        };
        for camo in [0, sample.weapon_model] {
            batch.prepare(&soldiers, &mut compositions, axis, weapon.handle(), camo);
        }
    }
}

pub fn finish_body_draw_plan(mut plan: ResMut<RemoteBodyDrawPlan>) {
    finish_remote_body_draw_plan(&mut plan);
}

fn begin_remote_body_tess() {}

fn remote_body_draw_gate(
    presentation: &net::LocalPresentation<'_>,
    subject: Option<&ViewSubject>,
    settings: &frame::GameSettings,
) -> PlayerDrawGate {
    let net::LocalPresentation { presented, local } = presentation;
    let in_killcam = subject.is_some_and(|s| s.in_killcam());
    let eyes = match subject {
        Some(ViewSubject::Seat {
            focus: Some(focus), ..
        }) => i32::try_from(*focus).unwrap_or(0),
        _ => i32::try_from(local.0.0).unwrap_or(0),
    };
    PlayerDrawGate {
        eyes_entity_num: eyes,
        other_flags: presented
            .player(local.0)
            .map(|ps| ps.other_flags)
            .unwrap_or(0),
        rendering_third_person: crate::occupancy::third_person::presented_is_third_person(
            presented,
            local.0,
            in_killcam,
            settings.third_person,
        ),
    }
}

#[expect(
    clippy::type_complexity,
    clippy::too_many_arguments,
    reason = "Bevy system parameters keep scheduler resource access explicit."
)]
fn occupy_remote_scene_ents(
    mut scene_skels: ResMut<AnimDObjSceneSkels>,
    mut scene_submissions: MessageWriter<AnimDObjSceneSubmission>,
    lod_skinned: Res<render_scene::LodRampSkinnedDvar>,
    cameras: Query<&GlobalTransform, With<Camera3d>>,
    presentation: net::LocalPresentation,
    subject: Option<Res<ViewSubject>>,
    settings: Res<frame::GameSettings>,
    bodies: Option<Res<PreparedBodies>>,
    soldiers: Option<Res<asset_game::SoldierPresentations>>,
    weapons: Option<Res<PreparedWeapons>>,
    world_weapons: Option<Res<PreparedWorldWeapons>>,
    prepared_kits: Res<PreparedRemoteKits>,
    compositions: Res<PreparedItemCompositions>,
    remotes: Query<
        (&CEntity, &CEntityRuntime, &Transform),
        (
            Without<WorldScriptModelInstance>,
            Without<FpvLens>,
            Without<FlyCamera>,
        ),
    >,
) {
    let Some(soldiers) = soldiers.as_deref() else {
        return;
    };
    let Some(bodies) = bodies.as_deref() else {
        return;
    };
    if let Some(world) = world_weapons.as_deref() {
        scene_skels.seed(bodies, world);
    }
    let live_kits = match (weapons.as_deref(), world_weapons.as_deref()) {
        (Some(weapons), Some(world)) if prepared_kits.owned_by(bodies, weapons, world) => {
            Some((&*prepared_kits, world))
        }
        _ => None,
    };
    let eye = cameras.single().ok().map(|xf| xf.translation());
    let gate = remote_body_draw_gate(&presentation, subject.as_deref(), &settings);
    let presented = &presentation.presented;
    for (identity, runtime, transform) in &remotes {
        if !remote_body_submits(identity.number(), runtime, gate) {
            continue;
        }
        let Some(client) = identity.client() else {
            continue;
        };
        let meta = presented
            .snapshot()
            .and_then(|snap| snap.meta.for_client(client));
        let corpse = presented.snapshot().and_then(|snapshot| {
            snapshot
                .meta
                .corpses
                .slots
                .iter()
                .find(|slot| slot.occupied && slot.entnum == i32::from(identity.number()))
        });
        let ffa_team = corpse.map_or_else(
            || meta.and_then(|meta| meta.ffa_team),
            |corpse| corpse.ffa_team,
        );
        let client_state_team = corpse.map_or_else(
            || meta.map_or(0, |meta| meta.client_state_team),
            |corpse| corpse.client_state_team,
        );
        let axis = asset_model::kit_assignment_is_axis(client_state_team, ffa_team);
        let Ok(soldier) = soldiers.side(axis) else {
            continue;
        };
        if !soldier.owns_bodies(&bodies.0) {
            continue;
        }
        let shield = presented
            .snapshot()
            .and_then(|snap| drawn_shield(snap, identity.number(), client));
        let held = remote_pose_sample(runtime).weapon;
        let (kit_models, radius, hide_part_bits) = match live_kits.filter(|_| shield.is_none()) {
            Some((kits, _world)) => {
                let Some(kit) = kits.get(axis, held, 0) else {
                    continue;
                };
                let Some(models) = kit.models() else {
                    continue;
                };
                (models, kit.radius, kit.hide_part_bits)
            }
            None => {
                let Some((models, radius)) = occupy_remote_kit_dobj(
                    soldier,
                    weapons.as_deref(),
                    world_weapons.as_deref(),
                    held,
                    true,
                    shield,
                    weapons
                        .as_deref()
                        .zip(world_weapons.as_deref())
                        .and_then(|(w, c)| compositions.get(w, c, held, 0)),
                ) else {
                    continue;
                };
                let bits = crate::anim::remote_body::kit_hide_part_bits(&models);
                (models, radius, bits)
            }
        };
        let origin = transform.translation.to_array();
        let quat = [
            transform.rotation.x,
            transform.rotation.y,
            transform.rotation.z,
            transform.rotation.w,
        ];
        let entnum = u32::from(identity.number());

        let ramp = lod_skinned.args();
        let lod_bytes: Vec<i8> = kit_models
            .iter()
            .map(|model| occupy_lod_byte(skel_camera_lod(model.skel, origin, eye, &ramp)))
            .collect();
        let models: Vec<_> = kit_models
            .iter()
            .enumerate()
            .map(|(i, model)| {
                scene_skels.model(
                    model.name,
                    model.skel,
                    lod_bytes.get(i).copied().unwrap_or(-1),
                )
            })
            .collect();
        scene_submissions.write(AnimDObjSceneSubmission {
            render_fx_flags: 0,
            has_tree: true,
            origin,
            lighting_origin: origin,
            radius,
            entnum,
            quat: Some(quat),
            occupy_model_n: u8::try_from(models.len()).unwrap_or(u8::MAX),
            models,
            hide_part_bits,
            store_skin: true,
        });
    }
}

fn sync_remote_bodies(
    mut commands: Commands,
    presentation: net::LocalPresentation,
    subject: Option<Res<ViewSubject>>,
    settings: Res<frame::GameSettings>,
    bodies: Option<Res<PreparedBodies>>,
    soldiers: Option<Res<asset_game::SoldierPresentations>>,
    existing: Query<(Entity, &CEntity, &CEntityRuntime, Option<&RemotePlayer>)>,
) {
    let Some(bodies) = bodies else {
        return;
    };

    let Some(soldiers) = soldiers else {
        return;
    };

    let gate = remote_body_draw_gate(&presentation, subject.as_deref(), &settings);
    let presented = &presentation.presented;

    for (entity, identity, runtime, remote) in &existing {
        let Some(client) = identity.client() else {
            if remote.is_some() {
                commands
                    .entity(entity)
                    .remove::<(RemotePlayer, RemoteFxBolts, RemoteBodyState)>();
            }
            continue;
        };
        if !remote_body_submits(identity.number(), runtime, gate) {
            if remote.is_some() {
                commands
                    .entity(entity)
                    .remove::<(RemotePlayer, RemoteFxBolts, RemoteBodyState)>();
            }
            continue;
        }
        let meta = presented
            .snapshot()
            .and_then(|snap| snap.meta.for_client(client));
        let corpse = presented.snapshot().and_then(|snapshot| {
            snapshot
                .meta
                .corpses
                .slots
                .iter()
                .find(|slot| slot.occupied && slot.entnum == i32::from(identity.number()))
        });
        let ffa_team = corpse.map_or_else(
            || meta.and_then(|meta| meta.ffa_team),
            |corpse| corpse.ffa_team,
        );
        let client_state_team = corpse.map_or_else(
            || meta.map_or(0, |meta| meta.client_state_team),
            |corpse| corpse.client_state_team,
        );
        let Ok(soldier) = soldiers.side(asset_model::kit_assignment_is_axis(
            client_state_team,
            ffa_team,
        )) else {
            if remote.is_some() {
                commands
                    .entity(entity)
                    .remove::<(RemotePlayer, RemoteFxBolts, RemoteBodyState)>();
            }
            continue;
        };
        if !soldier.owns_bodies(&bodies.0) {
            if remote.is_some() {
                commands
                    .entity(entity)
                    .remove::<(RemotePlayer, RemoteFxBolts, RemoteBodyState)>();
            }
            continue;
        };
        let body_entry = soldier.body();
        if body_entry.skel.positions.is_empty() || body_entry.skel.bones.is_empty() {
            if remote.is_some() {
                commands
                    .entity(entity)
                    .remove::<(RemotePlayer, RemoteFxBolts, RemoteBodyState)>();
            }
            continue;
        }

        let origin = runtime.origin;
        let yaw = runtime.angles[1];
        let pose = Transform {
            translation: Vec3::from_array(origin),
            rotation: Quat::from_rotation_z(yaw.to_radians()),
            scale: Vec3::ONE,
        };
        let marker = RemotePlayer {
            ffa_team,
            client_state_team,
            shield: presented
                .snapshot()
                .and_then(|snap| drawn_shield(snap, identity.number(), client)),
        };
        if remote.is_some() {
            commands.entity(entity).insert((pose, marker));
        } else {
            attach_remote_root(&mut commands, entity, pose, marker);
        }
    }
}

struct PendingBodySkin<'a> {
    entity: Entity,
    number: u32,
    transform: Transform,
    matrices: Vec<Mat4>,
    body: &'a asset_model::BodyMeshEntry,
    head: Option<(&'a asset_model::BodyMeshEntry, usize)>,
    gun: Option<PendingGunSkin<'a>>,
    body_lod: u8,
    head_lod: Option<u8>,
    gun_lod: Option<u8>,

    skin_entries: &'a [dpvs_iw4::SceneEntSkinEntry],
    head_model: Option<u16>,
    gun_model: Option<u16>,
    attachments: Vec<(PendingGunSkin<'a>, u16, Option<u8>)>,

    camouflage: Option<&'a asset_game::WeaponCamouflage>,
    dest: CpuBodyGeom,
}

#[derive(QueryData)]
#[query_data(mutable)]
struct RemoteBodyRoot {
    entity: Entity,
    identity: &'static CEntity,
    runtime: &'static CEntityRuntime,
    transform: &'static Transform,
    bolts: &'static mut RemoteFxBolts,
    remote: &'static RemotePlayer,
    state: &'static mut RemoteBodyState,
}

struct RemotePoseFrame<'a> {
    soldiers: &'a asset_game::SoldierPresentations,
    tree: &'a asset_anim::CompiledAnimTreeDefinition,
    bodies: &'a PreparedBodies,
    weapons: Option<&'a PreparedWeapons>,
    world_weapons: Option<&'a PreparedWorldWeapons>,
    kits: Option<&'a PreparedRemoteKits>,
    compositions: &'a PreparedItemCompositions,
    dt: f32,
    eye: Option<Vec3>,
    ramp: LodRampArgs,
    geometry_revision: &'a mut RemoteGeometryRevision,
    submit: &'a mut RemoteBodySkinnedQueue,
    dobj_poses: &'a mut crate::anim::dobj_pose::HostDObjPoseFrame,
    posed_players: &'a mut crate::anim::dobj_pose::PosedPlayerFrame,
    scene: &'a render_scene::GfxScene,
    pending: Vec<PendingBodySkin<'a>>,

    world_gun_gap: Option<WorldGunGap>,
}

struct RemoteBodySceneSlot<'a> {
    hidden: bool,
    surface_count: Option<u32>,
    lods: &'a [i8],
    skin_entries: &'a [dpvs_iw4::SceneEntSkinEntry],
}

fn remote_body_scene_slot(scene: &render_scene::GfxScene, number: u32) -> RemoteBodySceneSlot<'_> {
    RemoteBodySceneSlot {
        hidden: scene.scene_ent_hidden(number),
        surface_count: scene.scene_ent_surface_count(number),
        lods: scene
            .scene_dobj(number)
            .map(|dobj| dobj.lods.as_slice())
            .unwrap_or(&[]),
        skin_entries: scene
            .scene_ent_skinned_surfs(number)
            .map(|surfs| surfs.entries.as_slice())
            .unwrap_or(&[]),
    }
}

#[expect(
    clippy::type_complexity,
    clippy::too_many_arguments,
    reason = "Bevy system parameters keep scheduler resource access explicit."
)]
fn pose_remote_bodies(
    time: Res<Time>,
    sources: Option<Res<asset_anim::PlayerAnimSources>>,
    soldiers: Option<Res<asset_game::SoldierPresentations>>,
    bodies: Option<Res<PreparedBodies>>,
    weapons: Option<Res<PreparedWeapons>>,
    world_weapons: Option<Res<PreparedWorldWeapons>>,
    prepared_kits: Res<PreparedRemoteKits>,
    compositions: Res<PreparedItemCompositions>,
    mut geometry_revision: ResMut<RemoteGeometryRevision>,
    mut skin_shapes: ResMut<BodySkinShapes>,
    mut submit: ResMut<RemoteBodySkinnedQueue>,
    dpvs: (
        Query<&GlobalTransform, With<render_scene::FpvLens>>,
        Res<render_scene::LodRampSkinnedDvar>,
        ResMut<crate::anim::dobj_pose::HostDObjPoseFrame>,
        ResMut<crate::anim::dobj_pose::PosedPlayerFrame>,
        Res<HostGfxScene>,
        Option<Res<FrameClock>>,
    ),
    mut roots: Query<
        RemoteBodyRoot,
        (
            With<RemotePlayer>,
            Without<WorldScriptModelInstance>,
            Without<render_scene::FpvLens>,
            Without<render_scene::FlyCamera>,
        ),
    >,
) {
    let (cameras, lod_skinned, mut dobj_poses, mut posed_players, gfx, cg_clock) = dpvs;
    submit.clear();
    for mut root in &mut roots {
        *root.bolts = RemoteFxBolts::default();
    }
    let legs_written = remote_legs_written(roots.iter().map(|root| root.runtime));
    let leaves_bound = sources
        .as_deref()
        .map(|sources| remote_written_leaves_bound(roots.iter().map(|root| root.runtime), sources))
        .unwrap_or(false);

    let source_cause = match sources.as_deref() {
        None => Some(Cow::Borrowed("player animation sources not prepared")),
        Some(sources) if !sources.decode_errors().is_empty() => {
            Some(Cow::<'static, str>::Owned(format!(
                "player animation source decode failed: {first}",
                first = sources
                    .decode_errors()
                    .first()
                    .map(|error| format!("{error:?}"))
                    .unwrap_or(String::new())
            )))
        }
        Some(sources) if sources.multiplayer_atr().is_none() => {
            Some(Cow::Borrowed("animtrees/multiplayer.atr absent"))
        }
        Some(sources) if sources.playeranim_script().is_none() => {
            Some(Cow::Borrowed("mp/playeranim.script absent"))
        }
        Some(sources) => match sources.compiled() {
            None => Some(Cow::Borrowed("ATR/script compiler missing")),
            Some(Err(error)) => Some(Cow::<'static, str>::Owned(format!(
                "ATR compile failed: {reason}",
                reason = error
            ))),
            Some(Ok(_)) => match sources.parsed_script() {
                Some(Err(error)) => Some(Cow::<'static, str>::Owned(format!(
                    "playeranim.script parse failed: {reason}",
                    reason = error
                ))),
                Some(Ok(_)) if !legs_written => Some(Cow::Borrowed("playeranim evaluator missing")),
                Some(Ok(_)) if !leaves_bound => Some(Cow::Borrowed("XAnim leaf bind missing")),
                Some(Ok(_)) => None,
                _ => Some(Cow::Borrowed("playeranim evaluator missing")),
            },
        },
    };
    if let Some(cause) = source_cause {
        diag::warn_once!(World, "{}", cause);
        return;
    }

    let Some(sources) = sources.as_deref() else {
        diag::warn_once!(World, "player animation sources not prepared");
        return;
    };
    let Some(Ok(tree)) = sources.compiled() else {
        diag::warn_once!(World, "ATR/script compiler missing");
        return;
    };
    let Some(Ok(_script)) = sources.parsed_script() else {
        diag::warn_once!(
            World,
            "XAnimCalc failed: {reason}",
            reason = String::from("playeranim.script not parsed")
        );
        return;
    };
    let Some(soldiers) = soldiers.as_deref() else {
        diag::warn_once!(
            World,
            "XAnimCalc failed: {reason}",
            reason = String::from("soldier presentations not installed")
        );
        return;
    };
    let Some(bodies) = bodies.as_deref() else {
        diag::warn_once!(
            World,
            "XAnimCalc failed: {reason}",
            reason = String::from("PreparedBodies not installed")
        );
        return;
    };

    let eye = cameras.single().ok().map(|xf| xf.translation());
    let skinned_ramp = lod_skinned.args();
    let mut posed_bodies = 0usize;
    let mut scene_hidden_bodies = 0usize;
    let mut last_calc_error = None;
    let mut pose_frame = RemotePoseFrame {
        soldiers,
        tree,
        bodies,
        weapons: weapons.as_deref(),
        world_weapons: world_weapons.as_deref(),
        kits: match (weapons.as_deref(), world_weapons.as_deref()) {
            (Some(weapons), Some(world)) if prepared_kits.owned_by(bodies, weapons, world) => {
                Some(&*prepared_kits)
            }
            _ => None,
        },
        compositions: &compositions,
        dt: cg_clock
            .as_ref()
            .map(|clock| clock.frametime_secs())
            .unwrap_or_else(|| time.delta_secs()),
        eye,
        ramp: skinned_ramp,
        geometry_revision: &mut geometry_revision,
        submit: &mut submit,
        dobj_poses: &mut dobj_poses,
        posed_players: &mut posed_players,
        scene: &gfx.scene,
        pending: Vec::new(),
        world_gun_gap: None,
    };
    for root in &mut roots {
        let posed = pose_frame.pose_one_remote(root);
        match posed {
            Ok(PoseOneOutcome::Posed) => posed_bodies += 1,
            Ok(PoseOneOutcome::SceneHidden) => scene_hidden_bodies += 1,
            Ok(PoseOneOutcome::NoBodyLod | PoseOneOutcome::NoAnim) => {}
            Err(reason) => last_calc_error = Some(reason),
        }
    }

    if let Some(gap) = pose_frame.world_gun_gap.take() {
        diag::warn_once!(
            World,
            "remote body world gun missing: weapon {weapon} `{world_model}`",
            weapon = gap.weapon,
            world_model = gap.world_model
        )
    }

    if posed_bodies == 0 {
        if scene_hidden_bodies > 0 {
            return;
        }
        diag::warn_once!(
            World,
            "{}",
            match last_calc_error {
                Some(reason) => Cow::<'static, str>::Owned(format!(
                    "XAnimCalc failed: {reason}",
                    reason = (reason)
                )),
                None => Cow::Borrowed("XAnimCalc missing"),
            }
        );
        return;
    }

    skin_shapes.prepare(bodies, world_weapons.as_deref());
    let _skin_model = perf::span!("skin_model", "iw4l.render");
    let workers = skin_blend_worker_count(pose_frame.pending.len());
    let pending = std::mem::take(&mut pose_frame.pending);
    let assembled = assemble_meshes_parallel(pending, workers, &skin_shapes);
    for assembled in assembled {
        if let Ok(assembled) = assembled
            && let Ok(mut root) = roots.get_mut(assembled.entity)
        {
            assembled.commit(
                pose_frame.submit,
                &mut root.state.skin,
                pose_frame.geometry_revision,
            );
        }
    }
}

#[derive(Clone, Copy)]
enum PoseOneOutcome {
    Posed,
    SceneHidden,
    NoBodyLod,
    NoAnim,
}

impl<'a> RemotePoseFrame<'a> {
    fn pose_one_remote(
        &mut self,
        mut root: RemoteBodyRootItem<'_, '_>,
    ) -> Result<PoseOneOutcome, String> {
        let bolts = &mut *root.bolts;
        let state = &mut *root.state;
        *bolts = RemoteFxBolts::default();
        let number = u32::from(root.identity.number());
        let is_corpse = root.runtime.next_state.e_type == ET_PLAYER_CORPSE
            || root.runtime.pose_e_type == ET_PLAYER_CORPSE as u8;
        let corpse_age_ms = is_corpse.then_some(
            root.runtime
                .pose_time_ms
                .wrapping_sub(root.runtime.next_state.time2),
        );
        let last_cache_hit = std::mem::take(&mut state.skin.last_cache_hit);

        if root.runtime.previous_pose.is_none()
            && let Some(slot) = state.tree.as_mut()
        {
            slot.legs_rate_sample.time_ms = 0;
            slot.torso_rate_sample.time_ms = 0;
        }
        let sample = remote_pose_sample(root.runtime);
        if is_corpse {
            let legs_leaf =
                PlayerAnimValue::from_raw((sample.anim.legs_anim as u16) & PLAYER_ANIM_RAW_MASK)
                    .map(|v| v.effective_index())
                    .filter(|&index| index != 0)
                    .and_then(|index| self.tree.node(index).map(|n| n.name.as_str()));
            perf::instant!("corpse", "iw4l.sim", |ctx| {
                let (occupied, pose_e_type, legs_leaf_name): (i64, Option<i64>, Option<&str>) =
                    (1, Some(i64::from(ET_PLAYER_CORPSE)), legs_leaf);
                ctx.arg("occupied", occupied);

                ctx.arg("pose_e_type", pose_e_type);
                ctx.arg("legs_leaf_name", legs_leaf_name);
            });
        }
        let Some(tracks) = RemoteAnimTracks::from_pair(sample.anim) else {
            state.tree = None;
            return Ok(PoseOneOutcome::NoAnim);
        };
        let e_type = if is_corpse {
            ET_PLAYER_CORPSE
        } else {
            ET_PLAYER
        };
        let bodies = self.bodies;
        let weapons = self.weapons;
        let world_weapons = self.world_weapons;
        let kits = self.kits;
        let weapon = sample.weapon;
        let view_pitch_deg = sample.view_pitch_deg;
        let prone = sample.prone;
        let crouch = sample.crouch;
        let dt = self.dt;
        let trees = &mut state.tree;
        let pending = &mut self.pending;
        let pose_hashes = &mut state.skin;
        let submit = &mut *self.submit;
        let slot = remote_body_scene_slot(self.scene, number);
        let scene_ent_surface_count = slot.surface_count;
        let skin_entries = slot.skin_entries;
        let slot_lods = slot.lods;
        let scene_ent_hidden = slot.hidden;
        let axis = asset_model::kit_assignment_is_axis(
            root.remote.client_state_team,
            root.remote.ffa_team,
        );
        let eye = self.eye;
        let ramp = self.ramp.clone();
        let dobj_poses = &mut *self.dobj_poses;
        let posed_players = &mut *self.posed_players;
        let world_gun_gap = &mut self.world_gun_gap;

        (|| {
            let origin = root.transform.translation.to_array();
            let soldier = self.soldiers.side(axis).map_err(str::to_owned)?;
            if !soldier.owns_bodies(&bodies.0) {
                return Err("soldier body owner differs from prepared bodies".into());
            }
            let model_set = select_remote_models(
                soldier,
                weapons,
                world_weapons,
                weapon,
                sample.weapon_model,
                root.remote.shield,
                weapons
                    .zip(world_weapons)
                    .and_then(|(w, c)| self.compositions.get(w, c, weapon, sample.weapon_model)),
            )?;
            let binding = soldier.animation().map_err(str::to_owned)?;
            let advanced = advance_remote_tree(
                binding,
                tracks,
                dt,
                sample.rate_origin,
                sample.rate_time_ms,
                corpse_age_ms,
                trees,
            )?;

            if scene_ent_hidden {
                return Ok(PoseOneOutcome::SceneHidden);
            }
            let AdvancedRemoteTree {
                runtime: anim_runtime,
                clips,
                reused: reuse,
            } = advanced;
            if let Some(gap) = model_set.world_gun_gap.clone() {
                *world_gun_gap = Some(gap);
            }
            let body = model_set.body;
            let Some(lods) = select_remote_lods(&model_set, slot_lods, |skel| {
                skel_camera_lod(skel, origin, eye, &ramp)
            }) else {
                return Ok(PoseOneOutcome::NoBodyLod);
            };
            if skip_frozen_corpse_dobj(
                e_type == ET_PLAYER_CORPSE,
                reuse,
                last_cache_hit,
                pose_hashes.has_lods(lods.tuple()),
            ) {
                push_cached_surfaces(number, root.transform, pose_hashes, submit);
                return Ok(PoseOneOutcome::Posed);
            }
            let kit = kits
                .and_then(|kits| kits.get(axis, weapon, sample.weapon_model))
                .ok_or_else(|| format!("remote kit not prepared: axis={axis} weapon={weapon}"))?;
            let prepared_dobj = kit.dobj.as_ref();
            ensure_remote_dobj(&model_set, e_type, trees, prepared_dobj)?;
            let dobj = trees
                .as_ref()
                .and_then(|slot| slot.dobj.as_deref())
                .expect("composed");
            validate_remote_tracks(dobj, clips.as_ref(), body, &model_set.body_name)?;

            let world = pose_remote_dobj(
                dobj,
                anim_runtime,
                remote_player_controller(is_corpse, view_pitch_deg, prone, crouch),
            )?;
            publish_remote_bones(
                kit.bolt_bones,
                &world,
                number,
                root.runtime,
                root.transform,
                bolts,
                dobj_poses,
            )?;
            let skin = dobj.skin_matrices(&world);
            if !is_corpse {
                publish_posed_player_head(
                    kit.head_bone,
                    &world,
                    root.identity,
                    root.transform,
                    posed_players,
                )?;
            }
            let hash = hash_skin_matrices(&skin) ^ u64::from(sample.weapon_model);
            let pose_same = pose_hashes.remember_pose_hash(hash);

            let skin_models = bind_remote_skin_models(dobj, &model_set)?;
            let lods_reusable = pose_hashes.has_lods(lods.tuple());
            match skin_after_pose(scene_ent_surface_count == Some(0), pose_same, lods_reusable) {
                SkinAfterPose::Culled => {
                    drop((skin_models, lods, skin));
                }
                SkinAfterPose::ReuseCache => {
                    drop((skin_models, lods, skin));
                    push_cached_surfaces(number, root.transform, pose_hashes, submit);
                }
                SkinAfterPose::Blend => {
                    let mut job = PendingBodySkin {
                        entity: root.entity,
                        number,
                        transform: *root.transform,
                        matrices: skin,
                        body: skin_models.body,
                        head: skin_models.head,
                        gun: skin_models.gun,
                        body_lod: lods.body,
                        head_lod: lods.head,
                        gun_lod: lods.gun,
                        skin_entries,
                        head_model: skin_models.head_model,
                        gun_model: skin_models.gun_model,
                        attachments: skin_models
                            .attachments
                            .into_iter()
                            .zip(lods.attachments)
                            .map(|((skin, model), lod)| (skin, model, lod))
                            .collect(),
                        camouflage: None,
                        dest: CpuBodyGeom::default(),
                    };
                    job.camouflage = weapons.and_then(|registry| {
                        registry
                            .registry()
                            .select_appearance(weapon, sample.weapon_model)
                            .and_then(|appearance| appearance.material_camouflage())
                    });
                    job.dest = take_unique_geom(pose_hashes);
                    pending.push(job);
                }
            }
            Ok(PoseOneOutcome::Posed)
        })()
    }
}

fn publish_remote_bones(
    bolt_bones: [Option<u16>; 4],
    world: &[Mat4],
    number: u32,
    runtime: &CEntityRuntime,
    transform: &Transform,
    bolts: &mut RemoteFxBolts,
    dobj_poses: &mut crate::anim::dobj_pose::HostDObjPoseFrame,
) -> Result<(), String> {
    dobj_poses
        .publish(
            number,
            runtime.in_next_snap(),
            runtime.next_state.e_flags,
            transform.to_matrix(),
            world,
        )
        .map_err(|error| format!("DObj bone publication failed: {error:?}"))?;
    let target = |bone: Option<u16>| -> Option<fx::FxBoltTarget> {
        let bone = bone?;
        let orientation = dobj_poses.resolve(number, i32::from(bone)).ok()?;
        Some(fx::FxBoltTarget {
            dobj: number,
            bone,
            centity_teleport: fx_iw4::bolt_spawn_teleport_bit(number, runtime.next_state.e_flags),
            orientation,
        })
    };
    bolts.flash = target(bolt_bones[0]);
    bolts.brass = target(bolt_bones[1]);
    bolts.knife = target(bolt_bones[2]);
    bolts.laser = target(bolt_bones[3]);
    Ok(())
}

fn publish_posed_player_head(
    head_bone: Option<usize>,
    world: &[Mat4],
    identity: &CEntity,
    transform: &Transform,
    frame: &mut crate::anim::dobj_pose::PosedPlayerFrame,
) -> Result<(), String> {
    let head = match head_bone {
        None => crate::anim::dobj_pose::PosedPlayerHead::NoDObjOrHead,
        Some(index) => {
            let bone = world.get(index).ok_or_else(|| {
                format!(
                    "DObj head index {index} exceeds {} published bones",
                    world.len()
                )
            })?;
            crate::anim::dobj_pose::PosedPlayerHead::Exact(
                (transform.to_matrix() * *bone).transform_point3(Vec3::ZERO),
            )
        }
    };
    frame.publish(crate::anim::dobj_pose::PosedPlayer {
        entnum: identity.number(),
        head,
    });
    Ok(())
}

fn skel_camera_lod(
    skel: &ModelSkel,
    origin: [f32; 3],
    eye: Option<Vec3>,
    ramp: &LodRampArgs,
) -> Option<u8> {
    smodel_camera_lod(skel.lod, origin, 1.0, eye, ramp)
}

fn surface_material_key(
    surface_index: usize,
    keys: &[Option<asset_core::MaterialKey>],
    edges: &[assets::AssetEdge<assets::MaterialSpace>],
) -> Option<asset_core::MaterialKey> {
    match edges.get(surface_index) {
        Some(assets::AssetEdge::Bound(_)) => Some(keys.get(surface_index)?.as_ref()?.clone()),
        _ => None,
    }
}

enum SkinSlotModel<'a> {
    Body(&'a asset_model::BodyMeshEntry),
    WorldWeapon(&'a asset_model::WorldWeaponEntry),
}

impl SkinSlotModel<'_> {
    fn skel(&self) -> &ModelSkel {
        match self {
            Self::Body(entry) => &entry.skel,
            Self::WorldWeapon(entry) => &entry.skel,
        }
    }

    fn namespace(&self) -> asset_core::FamilyId {
        match self {
            Self::Body(entry) => entry.namespace,
            Self::WorldWeapon(entry) => entry.namespace,
        }
    }

    fn world_weapon(&self) -> bool {
        matches!(self, Self::WorldWeapon(_))
    }

    fn material(&self, surface: usize) -> Option<asset_core::MaterialKey> {
        match self {
            Self::Body(entry) => {
                surface_material_key(surface, &entry.material_keys, &entry.material_edges)
            }
            Self::WorldWeapon(entry) => {
                surface_material_key(surface, &entry.material_keys, &entry.material_edges)
            }
        }
    }
}

struct SkinSlotStream<'a> {
    lod: u8,
    model: u16,
    entries: &'a [dpvs_iw4::SceneEntSkinEntry],
}

impl SkinSlotStream<'_> {
    fn draws(&self, surface: usize) -> bool {
        dpvs_iw4::SceneEntSkinEntry::stream_draws(self.entries, self.model, surface as u16)
    }

    fn rigid(&self, skel: &ModelSkel, surface: usize) -> bool {
        stream_lod_surface_rigid(skel, self.lod, self.entries, self.model, surface)
    }
}

fn skin_slot_into(
    source: SkinSlotModel,
    skin_shapes: &BodySkinShapes,
    skin: &[Mat4],
    base: usize,
    stream: SkinSlotStream,
    geom: &mut CpuBodyGeom,
    skip_empty: bool,
) -> Result<(), String> {
    let skel = source.skel();
    skin_slot_need(skel, skin, base)?;

    let layout = build_skin_layout(skel, stream.lod, |lod_local| stream.draws(lod_local))
        .ok_or_else(|| "skin_model refused the posed skeleton".to_string())?;
    let dest_first = geom.packed.len();
    geom.packed.resize(
        dest_first.saturating_add(layout.dest_vertex_n),
        [0u8; asset_iw4::size::GFX_PACKED_VERTEX],
    );
    let rigid = |surface| stream.rigid(skel, surface);
    if layout.dest_vertex_n != 0 {
        let shape = skin_shapes.shape(
            skel,
            source.namespace(),
            source.world_weapon(),
            stream.lod,
            &layout,
        );
        let bone_end = base
            .checked_add(shape.required_bones())
            .ok_or("body bone range overflow")?;
        let matrices = skin
            .get(base..bone_end)
            .ok_or("body bone range exceeds pose")?;
        geom.skin_jobs.push(render_frame::skinning::PackedSkinJob {
            ranges: if layout.surfaces.is_empty() {
                vec![render_frame::skinning::SkinVertexRange {
                    start: 0,
                    count: layout.dest_vertex_n as u32,
                    rigid: false,
                }]
                .into()
            } else {
                layout
                    .surfaces
                    .iter()
                    .filter(|surface| surface.visible && surface.index_count != 0)
                    .map(|surface| render_frame::skinning::SkinVertexRange {
                        start: surface.dest_first_vertex as u32,
                        count: surface.vertex_count as u32,
                        rigid: rigid(surface.surface_index),
                    })
                    .collect()
            },
            shape,
            bones: if matrices.is_empty() {
                Arc::new([Mat4::IDENTITY.to_cols_array_2d()])
            } else {
                matrices.iter().map(Mat4::to_cols_array_2d).collect()
            },
            destination: u32::try_from(dest_first)
                .map_err(|_| "body skin destination exceeds u32")?,
            offset: [0.0; 3],
        });
    }
    let vert_rebase = dest_first as u32;
    for surf in &layout.surfaces {
        if skip_empty && (!surf.visible || surf.index_count == 0) {
            continue;
        }
        if !surf.visible || surf.index_count == 0 {
            geom.surfaces.push(CpuSurfMeta {
                index_start: 0,
                index_count: 0,
                material: source.material(surf.surface_index),
            });
            continue;
        }
        let index_start = geom.indices.len() as u32;
        let src = surf.index_start as usize;
        let end = src.saturating_add(surf.index_count as usize);
        geom.indices.extend(
            layout.indices[src..end]
                .iter()
                .map(|&index| index.saturating_add(vert_rebase)),
        );
        geom.surfaces.push(CpuSurfMeta {
            index_start,
            index_count: surf.index_count,
            material: source.material(surf.surface_index),
        });
    }
    geom.decoded_n = geom.packed.len();
    Ok(())
}

fn skin_blend_worker_count(jobs: usize) -> usize {
    if jobs == 0 {
        return 1;
    }
    std::thread::available_parallelism()
        .map(|n| n.get().saturating_sub(1).max(1))
        .unwrap_or(1)
        .min(jobs)
}

fn assemble_meshes_parallel(
    jobs: Vec<PendingBodySkin<'_>>,
    workers: usize,
    skin_shapes: &BodySkinShapes,
) -> Vec<Result<AssembledRemoteBody, String>> {
    if jobs.is_empty() {
        return Vec::new();
    }
    if workers <= 1 || jobs.len() == 1 {
        return jobs
            .into_iter()
            .map(|job| assemble_meshes(job, skin_shapes))
            .collect();
    }
    let chunk_len = jobs.len().div_ceil(workers);
    let mut chunks = Vec::new();
    let mut rest = jobs;
    while !rest.is_empty() {
        let tail = if rest.len() > chunk_len {
            rest.split_off(chunk_len)
        } else {
            Vec::new()
        };
        chunks.push(rest);
        rest = tail;
    }

    ComputeTaskPool::get()
        .scope(|scope| {
            for chunk in chunks {
                scope.spawn(async move {
                    chunk
                        .into_iter()
                        .map(|job| assemble_meshes(job, skin_shapes))
                        .collect::<Vec<_>>()
                });
            }
        })
        .into_iter()
        .flatten()
        .collect()
}

fn assemble_meshes(
    job: PendingBodySkin<'_>,
    skin_shapes: &BodySkinShapes,
) -> Result<AssembledRemoteBody, String> {
    let entity = job.entity;
    let number = job.number;
    let transform = job.transform;
    let lods = (Some(job.body_lod), job.head_lod, job.gun_lod);
    let mut geom = job.dest;
    geom.skin_jobs.clear();
    skin_slot_into(
        SkinSlotModel::Body(job.body),
        skin_shapes,
        &job.matrices,
        0,
        SkinSlotStream {
            lod: job.body_lod,
            model: 0,
            entries: job.skin_entries,
        },
        &mut geom,
        false,
    )?;
    match (job.head, job.head_lod) {
        (None, _) | (Some(_), None) => {}
        (Some((head, base)), Some(lod)) => {
            skin_slot_into(
                SkinSlotModel::Body(head),
                skin_shapes,
                &job.matrices,
                base,
                SkinSlotStream {
                    lod,
                    model: job.head_model.unwrap_or(1),
                    entries: job.skin_entries,
                },
                &mut geom,
                false,
            )?;
        }
    }
    let gun_first = geom.surfaces.len();
    match (&job.gun, job.gun_lod) {
        (None, _) | (Some(_), None) => {}
        (Some(gun), Some(lod)) => {
            skin_slot_into(
                SkinSlotModel::WorldWeapon(gun.entry),
                skin_shapes,
                &job.matrices,
                gun.base,
                SkinSlotStream {
                    lod,
                    model: job.gun_model.unwrap_or(0),
                    entries: job.skin_entries,
                },
                &mut geom,
                true,
            )?;
        }
    }
    for (attachment, model, lod) in &job.attachments {
        let Some(lod) = *lod else {
            continue;
        };
        skin_slot_into(
            SkinSlotModel::WorldWeapon(attachment.entry),
            skin_shapes,
            &job.matrices,
            attachment.base,
            SkinSlotStream {
                lod,
                model: *model,
                entries: job.skin_entries,
            },
            &mut geom,
            true,
        )?;
    }
    if let Some(camo) = job.camouflage {
        for surface in &mut geom.surfaces[gun_first..] {
            if let Some((_, to)) = camo
                .materials
                .iter()
                .find(|(from, _)| surface.material.as_ref() == Some(from))
            {
                surface.material = Some(to.clone());
            }
        }
    }
    let (radii, radius_parents) = radii(
        job.body,
        job.head.map(|(head, _)| head),
        job.gun.as_ref().map(|gun| gun.entry),
    );
    Ok(AssembledRemoteBody {
        entity,
        number,
        transform,
        geom,
        radii,
        radius_parents,
        lods,
    })
}

fn enqueue_remote_body_lighting(
    submit: ResMut<RemoteBodySkinnedQueue>,
    mut binds: ResMut<RemoteBodyLightingBinds>,
    atpoint: Res<render_scene::DynAtPointLookup>,
    mut lighting_requests: ResMut<ModelLightingRequests>,
) {
    binds.by_client.clear();
    if submit.is_empty() {
        return;
    }
    for item in submit.iter() {
        let box_half = lighting_box_half(&item.radii, &item.radius_parents);
        let lookup_fallback = atpoint.fallback(item.origin, box_half);
        let client = u16::try_from(item.client).unwrap_or(u16::MAX);
        binds.by_client.insert(
            item.client,
            lighting_requests.request(ModelLightingRequest {
                owner: ModelLightingOwner::RemoteClient(client),
                origin: item.origin,
                lookup_fallback,
            }),
        );
    }
}

#[derive(bevy::ecs::system::SystemParam)]
struct RemoteBodyPublisher<'w, 's> {
    plan: ResMut<'w, RemoteBodyDrawPlan>,
    last_catalog: Local<'s, Option<Arc<render_material::RuntimeMaterialCatalog>>>,
}

impl RemoteBodyPublisher<'_, '_> {
    fn publish(
        mut self,
        mut submit: ResMut<RemoteBodySkinnedQueue>,
        mut binds: ResMut<RemoteBodyLightingBinds>,
        resolved: Res<ResolvedModelLightingTable>,
        atlas: Option<Res<WorldModelLightingAtlas>>,
        tess: Option<Res<render_scene::TessMaterials>>,
        model_materials: Res<crate::anim::model_materials::PreparedModelMaterials>,
    ) {
        let items = submit.take();
        let binds = std::mem::take(&mut binds.by_client);
        if items.is_empty() {
            if self.plan.decoded_n != 0 || self.plan.last_packed_id.is_some() {
                self.plan.clear_geometry();
            }
            *self.last_catalog = None;
            return;
        }
        if atlas.is_none() {
            self.plan.clear_geometry();
            *self.last_catalog = None;
            {
                perf::instant!("lighting_fail", "iw4l.render");
                diag::warn_once!(World, "remote body lighting alloc failed");
            };
            return;
        }
        let Some(tess) = tess else {
            self.plan.clear_geometry();
            *self.last_catalog = None;
            {
                perf::instant!("lighting_fail", "iw4l.render");
                diag::warn_once!(World, "remote body lighting alloc failed");
            };
            return;
        };
        if !self
            .last_catalog
            .as_ref()
            .is_some_and(|old| Arc::ptr_eq(old, tess.catalog()))
        {
            self.plan.clear_geometry();
            *self.last_catalog = Some(Arc::clone(tess.catalog()));
        }

        let mut last_cause = None;
        let mut seated = Vec::with_capacity(items.len());
        for item in items {
            let seated_light = binds
                .get(&item.client)
                .and_then(|request| resolved.get(request.owner));
            let Some((handle, scene_light, probe)) = (match seated_light {
                Some(ResolvedModelLighting::Seated {
                    handle,
                    scene_light_index,
                    reflection_probe_index,
                    ..
                }) => Some((handle, scene_light_index, reflection_probe_index)),
                Some(ResolvedModelLighting::Failed) | None => {
                    last_cause = Some(Cow::Borrowed("remote body lighting alloc failed"));
                    None
                }
            }) else {
                continue;
            };
            seated.push((item, handle, scene_light, probe));
        }
        if seated.is_empty() {
            if self.plan.decoded_n != 0 || self.plan.last_packed_id.is_some() {
                self.plan.clear_geometry();
            }
            *self.last_catalog = None;
            diag::warn_once!(
                World,
                "{}",
                last_cause.unwrap_or(Cow::Borrowed("remote body submit missing"))
            );
            return;
        }

        let packed_id = seated_packed_id(&seated);
        let draw_id = seated_draw_id(&seated);
        if self.plan.last_packed_id == Some(packed_id)
            && seated_clients_match(&self.plan, &seated)
            && refresh_remote_body_bones(&mut self.plan, &seated)
        {
            if self.plan.last_draw_id != Some(draw_id) {
                refresh_remote_body_draws(&mut self.plan, &seated);
                self.plan.revisions.bump_draws();
                self.plan.revision = self.plan.revision.wrapping_add(1);
                self.plan.last_draw_id = Some(draw_id);
            }

            return;
        }

        self.plan.clear_geometry();
        let vert_n: usize = seated
            .iter()
            .map(|(item, _, _, _)| item.geom.decoded_n)
            .sum();
        let idx_n: usize = seated
            .iter()
            .map(|(item, _, _, _)| item.geom.indices.len())
            .sum();
        self.plan.indices.reserve(idx_n);
        let mut session = take_body_packed_session(&mut self.plan);
        session.reserve_packed(vert_n);
        let mut mat_by_key: HashMap<asset_core::MaterialKey, u32> = HashMap::new();
        let mut submitted_verts = 0usize;
        let mut any_missing_material = false;
        for (item, handle, scene_light, probe) in seated {
            let before = self.plan.decoded_n;
            let appended = append_remote_body_cpu_blob(
                &mut self.plan,
                &mut session,
                &item.geom.packed,
                &item.geom.indices,
                item.geom.decoded_n,
            );
            if let Some((vertex_base, _)) = appended {
                for job in &item.geom.skin_jobs {
                    let mut job = job.clone();
                    job.destination = vertex_base
                        .checked_add(job.destination)
                        .expect("body skin destination fits u32");
                    self.plan.skin_jobs.push(job);
                }
            }
            let index_base = appended.map(|(_, index_base)| index_base);
            for surface in item.geom.surfaces.iter() {
                let Some(key) = surface.material.as_ref() else {
                    any_missing_material = true;
                    last_cause = Some({
                        let name: String = String::new();
                        Cow::<'static, str>::Owned(if name.is_empty() {
                            "remote body material missing: authored name not retained at capture"
                                .to_owned()
                        } else {
                            format!("remote body material missing: `{name}` not in global catalog")
                        })
                    });
                    continue;
                };
                let mat_idx = if let Some(&idx) = mat_by_key.get(key) {
                    idx
                } else {
                    let Some(material) = model_materials.material(tess.catalog(), key).cloned()
                    else {
                        any_missing_material = true;
                        last_cause = Some({
                            let name: String = format!("{}:{}", key.namespace.as_str(), key.name);
                            Cow::<'static, str>::Owned(if name.is_empty() {
                                "remote body material missing: authored name not retained at capture"
                                    .to_owned()
                            } else {
                                format!(
                                    "remote body material missing: `{name}` not in global catalog"
                                )
                            })
                        });
                        continue;
                    };
                    let idx = self.plan.materials.len() as u32;
                    self.plan.materials.push(material);
                    mat_by_key.insert(key.clone(), idx);
                    idx
                };
                if let Some(index_base) = index_base {
                    push_remote_body_cpu_draw(
                        &mut self.plan,
                        (
                            index_base.saturating_add(surface.index_start),
                            surface.index_count,
                        ),
                        &item,
                        handle,
                        scene_light,
                        probe,
                        mat_idx,
                    );
                }
            }
            if self.plan.decoded_n > before {
                submitted_verts += self.plan.decoded_n - before;
            }
        }
        install_body_packed_session(&mut self.plan, session);
        if submitted_verts > 0 && !any_missing_material {
            self.plan.revisions.bump_surfaces();
            self.plan.revisions.bump_draws();
            let topology = topology_fingerprint(
                &self.plan.indices,
                &self.plan.surface_ranges,
                self.plan.decoded_n,
            );
            if self.plan.revisions.topology != topology {
                self.plan.revisions.topology = topology;
            }
            self.plan.revision = self.plan.revision.wrapping_add(1);
            self.plan.last_packed_id = Some(packed_id);
            self.plan.last_draw_id = Some(draw_id);

            return;
        }
        diag::warn_once!(
            World,
            "{}",
            last_cause.unwrap_or(Cow::Borrowed("remote body submit missing"))
        );
    }
}

fn submit_remote_bodies(
    publisher: RemoteBodyPublisher,
    submit: ResMut<RemoteBodySkinnedQueue>,
    binds: ResMut<RemoteBodyLightingBinds>,
    resolved: Res<ResolvedModelLightingTable>,
    atlas: Option<Res<WorldModelLightingAtlas>>,
    tess: Option<Res<render_scene::TessMaterials>>,
    model_materials: Res<crate::anim::model_materials::PreparedModelMaterials>,
) {
    publisher.publish(submit, binds, resolved, atlas, tess, model_materials);
}

fn refresh_remote_body_bones(
    plan: &mut RemoteBodyDrawPlan,
    seated: &[(RemoteBodySkinnedItem, u32, u8, u8)],
) -> bool {
    let mut slots = plan.skin_jobs.iter_mut();
    for job in seated.iter().flat_map(|(item, ..)| &item.geom.skin_jobs) {
        match slots.next() {
            Some(slot) if slot.shape.id() == job.shape.id() => slot.bones = Arc::clone(&job.bones),
            _ => return false,
        }
    }
    slots.next().is_none()
}

fn seated_packed_id(seated: &[(RemoteBodySkinnedItem, u32, u8, u8)]) -> u64 {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    seated.len().hash(&mut hasher);
    for (item, _, _, _) in seated {
        item.client.hash(&mut hasher);
        item.geometry_revision.hash(&mut hasher);
        item.geom.decoded_n.hash(&mut hasher);
    }
    hasher.finish()
}

fn seated_draw_id(seated: &[(RemoteBodySkinnedItem, u32, u8, u8)]) -> u64 {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    seated.len().hash(&mut hasher);
    for (item, handle, scene_light, probe) in seated {
        item.client.hash(&mut hasher);
        for lane in item.world_from_local.to_cols_array() {
            lane.to_bits().hash(&mut hasher);
        }
        handle.hash(&mut hasher);
        scene_light.hash(&mut hasher);
        probe.hash(&mut hasher);
    }
    hasher.finish()
}

fn seated_clients_match(
    plan: &RemoteBodyDrawPlan,
    seated: &[(RemoteBodySkinnedItem, u32, u8, u8)],
) -> bool {
    let mut live: Vec<u32> = seated.iter().map(|(item, _, _, _)| item.client).collect();
    let mut drawn: Vec<u32> = plan.draws.iter().filter_map(|d| d.scene_entnum).collect();
    live.sort_unstable();
    live.dedup();
    drawn.sort_unstable();
    drawn.dedup();
    live == drawn
}

fn refresh_remote_body_draws(
    plan: &mut RemoteBodyDrawPlan,
    seated: &[(RemoteBodySkinnedItem, u32, u8, u8)],
) {
    for draw in &mut plan.draws {
        let Some(ent) = draw.scene_entnum else {
            continue;
        };
        let Some((_, handle, scene_light, probe, world)) =
            seated
                .iter()
                .find_map(|(item, handle, scene_light, probe)| {
                    (item.client == ent).then_some((
                        item,
                        *handle,
                        *scene_light,
                        *probe,
                        item.world_from_local,
                    ))
                })
        else {
            continue;
        };
        draw.world_from_local = world;
        draw.lighting_handle = handle;
        draw.scene_light_index = scene_light;
        draw.reflection_probe_index = probe;
    }
}

fn remote_legs_written<'a>(mut runtimes: impl Iterator<Item = &'a CEntityRuntime>) -> bool {
    runtimes.any(|runtime| {
        let sample = remote_pose_sample(runtime);
        PlayerAnimValue::from_raw((sample.anim.legs_anim as u16) & PLAYER_ANIM_RAW_MASK)
            .is_some_and(|value| value.effective_index() != 0)
    })
}

fn remote_written_leaves_bound<'a>(
    runtimes: impl Iterator<Item = &'a CEntityRuntime>,
    sources: &asset_anim::PlayerAnimSources,
) -> bool {
    let Some(binds) = sources.leaf_binds() else {
        return false;
    };
    let mut any = false;
    for runtime in runtimes {
        let sample = remote_pose_sample(runtime);
        let Some(value) =
            PlayerAnimValue::from_raw((sample.anim.legs_anim as u16) & PLAYER_ANIM_RAW_MASK)
        else {
            continue;
        };
        let index = value.effective_index();
        if index == 0 {
            continue;
        }
        any = true;
        if !binds.is_bound(index) {
            return false;
        }
        let Some(torso) =
            PlayerAnimValue::from_raw((sample.anim.torso_anim as u16) & PLAYER_ANIM_RAW_MASK)
        else {
            continue;
        };
        let torso_index = torso.effective_index();
        if torso_index != 0 && !binds.is_bound(torso_index) {
            return false;
        }
    }
    any
}

fn attach_remote_root(
    commands: &mut Commands,
    entity: Entity,
    transform: Transform,
    remote: RemotePlayer,
) {
    commands
        .entity(entity)
        .insert((remote, transform, Visibility::default()));
}

fn retire_remote_body(
    event: On<Remove<CEntity>>,
    identities: Query<&CEntity>,
    lighting: Option<ResMut<RemoteBodyLightingBinds>>,
) {
    let Ok(identity) = identities.get(event.entity) else {
        return;
    };
    let number = u32::from(identity.number());
    if let Some(mut lighting) = lighting {
        lighting.by_client.remove(&number);
    }
}

fn copy_corpse_animation(
    mut commands: Commands,
    added: Query<(Entity, &CEntityRuntime), Added<RemotePlayer>>,
    states: Query<&RemoteBodyState>,
    slots: Res<net::CEntitySlots>,
) {
    for (entity, runtime) in &added {
        if runtime.next_state.e_type != ET_PLAYER_CORPSE {
            continue;
        }
        let Some(victim) = u16::try_from(runtime.next_state.client_num)
            .ok()
            .and_then(|number| slots.entity_for_number(number))
        else {
            continue;
        };
        let Some(mut tree) = states.get(victim).ok().and_then(|state| state.tree.clone()) else {
            continue;
        };
        tree.dobj = None;
        tree.reuse_key = None;
        commands.queue(move |world: &mut World| {
            if let Some(mut state) = world.get_mut::<RemoteBodyState>(entity) {
                state.tree = Some(tree);
            }
        });
    }
}
