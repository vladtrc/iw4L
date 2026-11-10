use frame::ScopeApp;
use std::collections::HashMap;
use std::sync::Arc;

use asset_game::{FxDefinitions, OwnedFxVisual, lookup_fx_color_image};
use assets::PreparedWeapons;
use bevy::ecs::system::SystemParam;
use bevy::prelude::*;
use dpvs_iw4::pack_mark_mesh_draw_surf;
use entity_iw4::{is_left_hand_fire_event, is_weapon_fire_last_shot_event};
use frame::ViewSubject;
use fx::{
    FxMsec, FxSparkCloudInstance, FxSpriteInstance, FxSystemHost, MarkReceiverEnable, PlayResult,
    axis_from_hit_normal,
};
use fx_iw4::{
    FX_GLASS_SHATTER_FX_PER_FRAME, elem_run_mode, elem_spawn_offset_mode,
    glass_shatter_fx_fallback, glass_shatter_fx_name, laser_post_light_allows, tail_anchor_origin,
    tail_sprite_axes, tail_sprite_full_extent,
};
use net::{
    CEntity, CEntitySlots, ClientPredictionState, ClientSet, GameActive, LastAdoptedSnapshot,
    LocalPresentClient, PendingPelletFx, PlayerDrawGate, PresentedSnapshot, WeaponFirePing,
    WeaponFirePingBus,
};

use render_fx::{
    CombatFxDump, EntityMarks, FireFxOccurrence, FireFxOutcome, FireFxRequest, FxCameraOrigin,
    FxDumpRequest, FxFrameOutcome, FxGeneratedFrame, FxJournalCursor, FxMarkDvars,
    FxUnavailableCause, FxWorldColorImages, HostFxDlights, HostFxPostLights, HostFxSystem,
    LaserDvars, PreparedFxCatalog, PreparedFxElemInfos, PreparedFxModels, PreparedImpactFx,
    PresentedFireFx, PresentedVehicleFx, PresentedVehicleFxRow, TracerDrawGate, TracerWorld,
    clear_fx_owned_plans, publish_empty_fx_owned_plans, tick_tracer_beams,
};

use render_fx::combat::{
    IDENTITY_AXIS, PelletFxPresentation, ShellEjectRequest, explosion_fx_names, log_combat_fx_gaps,
    missile_bolt_target, present_weapon_fx_bolted, sync_combat_dump, try_play_weapon_fx_at_origin,
    try_play_weapon_fx_bolted,
};
use render_fx::fire_weapon_fx_should_client_trace;
use render_fx::system::stamp_fx_camera_origin;

use crate::{
    adapters::fx::{
        present::{
            FxDrawCull, FxScene, boot_createfx_effects, build_fx_verts,
            play_named_oriented_at_msec, play_named_oriented_in_world,
            restamp_missing_packed_lighting, spawn_named_oriented_in_world, tick_fx_non_dependent,
            tick_fx_remaining,
        },
        tracer::present_tracer_beams,
        world_mark::FrontendFxScene,
    },
    assemble::drawsurf::{
        FxCodeMeshPlan, FxModelDrawPlan, FxParticleCloudPlan, GfxMarkMeshPlan, GfxMarkSubKey,
        MapOutdoor, MaterialGeneration, tan_half_fov_from_clip,
    },
    prepare::scene::{
        camera::FlyCamera,
        model_lighting_atlas::WorldModelLightingAtlas,
        model_lighting_cache::{
            ModelLightingOwner, ModelLightingRequest, ModelLightingRequests,
            WorldModelLightingCache,
        },
        smodel_geom_cache::LodRampSkinnedDvar,
        world::WorldScene,
    },
};

// Resolve muzzle tags after this frame's weapon poses, before advancing FX.
#[derive(Message)]
struct WeaponFireFx(net::EntityWeaponFire);

#[derive(Message)]
struct BulletHitFx(net::DispatchedEntityEvent);

struct FxFrameTransaction {
    outcome: FxFrameOutcome,
    _present_span: perf::SpanGuard,
}

#[derive(SystemParam)]
pub(crate) struct FxSceneAccess<'w> {
    scene: Option<Res<'w, WorldScene>>,
    marks: Res<'w, EntityMarks>,
}

impl FxSceneAccess<'_> {
    fn scene(&self) -> Option<&WorldScene> {
        self.scene.as_deref()
    }

    fn marks(&self) -> &EntityMarks {
        &self.marks
    }

    fn view(&self) -> Option<FrontendFxScene<'_>> {
        FrontendFxScene::wrap(self.scene(), self.marks())
    }
}

pub(crate) fn register_combat_fx_systems(app: &mut App) {
    app.scoped_message::<BulletHitFx>(frame::Timeline::Live)
        .scoped_message::<WeaponFireFx>(frame::Timeline::Live)
        .scoped::<PresentedFireFx>(frame::Timeline::Live)
        .scoped::<TrackerLight>(frame::Timeline::Live)
        .scoped::<super::flame::FlameStreams>(frame::Timeline::Live)
        .scoped::<render_fx::FxModelStaging>(frame::Timeline::Live)
        .scoped::<crate::assemble::drawsurf::GfxGlassMeshPlan>(frame::Timeline::Live)
        .scoped::<crate::assemble::drawsurf::GlassTable>(frame::Timeline::Live)
        .add_systems(
            Update,
            (
                boot_createfx_oneshots.in_set(frame::InMatch),
                sync_script_fx.in_set(frame::InMatch),
                crate::assemble::drawsurf::tess::glass::apply_glass_host.in_set(frame::InMatch),
                tick_fx_non_dependent_update.in_set(frame::InMatch),
            )
                .chain()
                .after(stamp_fx_camera_origin)
                .in_set(net::ClientSet::Present),
        )
        .add_systems(
            Update,
            tick_fx_remaining_update
                .after(tick_fx_non_dependent_update)
                .after(render_anim::occupancy::missile::MissilePosesPublished)
                .in_set(frame::InMatch)
                .after(frame::WorkerCmdSet::ModelPose)
                .in_set(net::ClientSet::Present),
        )
        .add_systems(
            crate::assemble::StaticSunAndFx,
            (
                generate_fx_transaction.pipe(commit_fx_transaction),
                super::super::motion_tracker::draw_motion_tracker,
            )
                .chain(),
        )
        .add_systems(
            Update,
            (
                maintain_fire_fx.in_set(frame::InMatch),
                drain_weapon_fire_fx.in_set(frame::InMatch),
                super::flame::tick_flame_streams.in_set(frame::InMatch),
                drain_bullet_hit_fx.in_set(frame::InMatch),
                drain_pellet_fx.in_set(frame::InMatch),
                present_tracker_light.in_set(frame::InMatch),
                drain_tag_fx.in_set(frame::InMatch),
            )
                .chain()
                .after(render_anim::occupancy::fpv_present::publish_fpv_dobj_pose)
                .after(frame::WorkerCmdSet::ModelPose)
                .before(boot_createfx_oneshots)
                .in_set(ClientSet::Present),
        )
        .add_systems(
            Update,
            tick_missile_present_state
                .in_set(frame::InMatch)
                .in_set(ClientSet::Effects),
        )
        .add_observer(publish_weapon_fire)
        .add_observer(eject_brass)
        .add_observer(explosion)
        .add_observer(retire_entity_bolts)
        .scoped::<ScriptFxRows>(frame::Timeline::Live)
        .add_systems(
            OnEnter(frame::Timeline::Live),
            reset_timeline_fx.after(frame::ScopeSet::Install),
        )
        .add_systems(
            OnEnter(frame::Timeline::Killcam),
            reset_timeline_fx.after(frame::ScopeSet::Install),
        )
        .scoped::<PendingTagFx>(frame::Timeline::Live)
        .add_observer(play_fx)
        .add_observer(play_fx_bullet_hit)
        .add_observer(melee_blood);
}

#[derive(SystemParam)]
pub(crate) struct FxRuntimeAccess<'w> {
    catalog: Option<Res<'w, PreparedFxCatalog>>,
    elem_infos: ResMut<'w, PreparedFxElemInfos>,
    host: ResMut<'w, HostFxSystem>,
}

#[derive(SystemParam)]
struct FxCombatJournal<'w> {
    cursor: ResMut<'w, FxJournalCursor>,
    combat: ResMut<'w, CombatFxDump>,
}

#[derive(SystemParam)]
struct WeaponSoundOutput<'w> {
    sound_bank: Option<Res<'w, audio::SoundBank>>,
    sounds: Option<ResMut<'w, Messages<audio::WeaponSound>>>,
}

#[derive(Resource, Default)]
struct TrackerLight {
    owner: Option<(sim::ClientId, sim::LifeSequence, u32)>,
    bolt: Option<(u32, u16)>,
}

fn present_tracker_light(
    presented: Res<PresentedSnapshot>,
    local: Res<LocalPresentClient>,
    prepared: Res<render_anim::PreparedFpv>,
    bolts: Res<render_anim::FpvBoltTargets>,
    runtime: FxRuntimeAccess,
    fx_world: FxSceneAccess,
    mut light: ResMut<TrackerLight>,
) {
    let FxRuntimeAccess {
        catalog,
        mut elem_infos,
        mut host,
    } = runtime;

    const EFFECT: &str = "misc/light_motion_tracker";
    let owner = presented.snapshot().and_then(|snapshot| {
        let meta = snapshot.meta.for_client(local.0)?;
        let ps = presented.player(local.0)?;
        let weapon = weapon_iw4::get_viewmodel_weapon_index(ps);
        (meta.lifecycle == sim::ClientLifecycle::Alive
            && ps.other_flags & 0x400 == 0
            && prepared.table()?.motion_tracker(weapon, ps.weapon_primary))
        .then_some((local.0, meta.life_sequence, weapon))
    });
    if owner != light.owner {
        if light.owner.is_some()
            && let Some((dobj, bone)) = light.bolt
        {
            host.0.stop_bolted(EFFECT, dobj, bone);
        }
        light.owner = owner;
        light.bolt = None;
    }
    if owner.is_none() || light.bolt.is_some() {
        return;
    }
    let (Some(target), Some(catalog)) = (bolts.tracker_light, catalog) else {
        return;
    };
    elem_infos.0.sync(&catalog.0);
    if render_fx::present::play_named_bolted_in_world(
        &mut host.0,
        &catalog.0,
        &elem_infos.0,
        asset_game::FxName::engine(EFFECT),
        target,
        fx_world.view().as_ref().map(|scene| scene as &dyn FxScene),
    )
    .and_then(PlayResult::handle)
    .is_some()
    {
        light.bolt = Some((target.dobj, target.bone));
    }
}

#[derive(SystemParam)]
struct TagLaserLights<'w, 's> {
    post_lights: ResMut<'w, HostFxPostLights>,
    fpv_bolts: Res<'w, crate::adapters::anim::fpv_present::FpvBoltTargets>,
    remotes: Query<'w, 's, &'static crate::adapters::anim::remote_body::RemoteFxBolts>,
    dvars: Res<'w, LaserDvars>,
}

impl TagLaserLights<'_, '_> {
    fn queue(&mut self, view: [f32; 3]) {
        let post_lights: &mut HostFxPostLights = &mut self.post_lights;
        let dvars = *self.dvars;
        let push = |post_lights: &mut HostFxPostLights, target: fx::FxBoltTarget, range: f32| {
            let Some(light) = fx_iw4::FxLaserRay {
                origin: target.orientation.origin,
                direction: target.orientation.axis[0],
                range,
            }
            .light_from_tag(
                view,
                fx_iw4::FxLaserLightSettings {
                    enabled: laser_post_light_allows(dvars.light, 1),
                    begin_pad: 0.0,
                    end_nudge: dvars.end_offset,
                    width: dvars.radius,
                },
            ) else {
                return;
            };
            post_lights.add(light);
        };
        for hand in 0..2 {
            if let Some(target) = self.fpv_bolts.laser[hand] {
                push(post_lights, target, dvars.range_player);
            }
        }
        for bolts in self.remotes.iter() {
            if let Some(target) = bolts.laser {
                push(post_lights, target, dvars.range);
            }
        }
    }

    fn take(&mut self) -> Vec<fx_iw4::FxPostLight> {
        std::mem::take(&mut self.post_lights.queued)
    }
}

fn boot_createfx_oneshots(
    emitters: Option<Res<asset_audio::CreateFxOneshotEmitters>>,
    runtime: FxRuntimeAccess,
    mut cursor: ResMut<FxJournalCursor>,
    fx_world: FxSceneAccess,
    cgame_active: Res<GameActive>,
    adopted: Option<Res<LastAdoptedSnapshot>>,
) {
    let FxRuntimeAccess {
        catalog,
        mut elem_infos,
        mut host,
    } = runtime;

    if cursor.createfx_booted {
        return;
    }
    if !cgame_active.get() {
        return;
    }
    if adopted
        .as_ref()
        .and_then(|a| a.next())
        .is_some_and(|snap| snap.meta.objectives.scripted_effects)
    {
        cursor.createfx_booted = true;
        return;
    }
    let Some(catalog) = catalog else {
        return;
    };
    let Some(emitters) = emitters else {
        return;
    };
    cursor.createfx_booted = true;
    cursor.createfx_boot_msec = Some(host.0.msec_now);
    if emitters.0.is_empty() {
        return;
    }
    let msec = host.0.msec_now;
    elem_infos.0.sync(&catalog.0);
    let census = boot_createfx_effects(
        &mut host.0,
        &catalog.0,
        &elem_infos.0,
        &emitters.0,
        msec,
        fx_world.view().as_ref().map(|s| s as &dyn FxScene),
    );
    cursor.createfx_miss = cursor.createfx_miss.saturating_add(census.miss_def);
    for failure in &census.failed {
        diag::warn!(World, "fx: CreateFX spawn_oriented failed — {failure}");
    }
    diag::info!(
        World,
        "fx: CreateFX oneshots — named={} held={} miss_def={} (Billboard/Oriented sprites; Beam/Trail/… still gaps)",
        census.named,
        census.held,
        census.miss_def
    );
}

pub(crate) fn tick_fx_non_dependent_update(
    runtime: FxRuntimeAccess,
    dvars: Res<FxMarkDvars>,
    collision: Option<Res<net::ClientCollision>>,
    camera_origin: Res<FxCameraOrigin>,
    fx_world: FxSceneAccess,
    mut post_lights: ResMut<HostFxPostLights>,
    mut aliases: MessageWriter<audio::AliasCommand>,
) {
    let FxRuntimeAccess {
        catalog,
        mut elem_infos,
        mut host,
    } = runtime;

    let msec = host.0.msec_now;
    let clip_world = collision
        .as_ref()
        .filter(|scene| scene.0.has_world_clip())
        .map(|scene| &scene.0);
    {
        let glass_clip = clip_world.map(ClientGlassTrace);
        host.0.glass.advance_in_world(
            msec,
            glass_clip.as_ref().map(|g| g as &dyn fx::GlassWorldTrace),
        );
    }
    let mut shatter_fx = 0u32;
    let mut pending_fx = Vec::new();
    for ev in host.0.glass.take_events() {
        if !ev.play_oneshot {
            continue;
        }
        let landing = ev.landing;
        if !landing {
            if shatter_fx >= FX_GLASS_SHATTER_FX_PER_FRAME {
                continue;
            }
            shatter_fx = shatter_fx.saturating_add(1);
        }
        let (alias, fallback) = if landing {
            ("glass_pane_shatter", "glass_pane_blowout")
        } else {
            glass_break_alias(ev.cause)
        };
        aliases.write(audio::AliasCommand::Play(audio::PlayAlias {
            event: None,
            namespace: asset_core::FamilyId::IW4,
            alias: alias.to_owned(),
            fallback: Some(fallback.to_owned()),
            origin_inches: Some(ev.origin),
            snd_ent: Some(fx_iw4::FX_ENTITYNUM_WORLD),
        }));
        pending_fx.push(ev);
    }
    let Some(catalog) = catalog else {
        return;
    };

    post_lights.queued.clear();
    post_lights.cap_full = 0;
    host.0.mark_receivers = MarkReceiverEnable {
        fx_marks: dvars.fx_marks,
        fx_marks_ents: dvars.fx_marks_ents,
        fx_marks_smodels: dvars.fx_marks_smodels,
    };
    let _fx_update = perf::span!("fx_update", "iw4l.fx");

    elem_infos.0.sync(&catalog.0);
    if !pending_fx.is_empty() {
        let world = fx_world.view();
        let scene = world.as_ref().map(|s| s as &dyn FxScene);
        for ev in pending_fx {
            let axis = axis_from_hit_normal(ev.normal);
            let name = glass_shatter_fx_name(ev.landing);
            let spawned = play_named_oriented_in_world(
                &mut host.0,
                &catalog.0,
                &elem_infos.0,
                asset_game::FxName::engine(name),
                ev.origin,
                axis,
                scene,
            );
            if spawned.is_none()
                && let Some(fallback) = glass_shatter_fx_fallback(ev.landing)
            {
                let _ = play_named_oriented_in_world(
                    &mut host.0,
                    &catalog.0,
                    &elem_infos.0,
                    asset_game::FxName::engine(fallback),
                    ev.origin,
                    axis,
                    scene,
                );
            }
        }
    }
    tick_fx_non_dependent(
        &mut host.0,
        &catalog.0,
        &elem_infos.0,
        msec,
        camera_origin.0,
        clip_world,
        fx_world.view().as_ref().map(|s| s as &dyn FxScene),
    );
}

fn glass_break_alias(cause: u8) -> (&'static str, &'static str) {
    match cause {
        1 => ("glass_pane_blowout", "glass_pane_shatter"),
        2 => ("glass_pane_breakout", "glass_pane_shatter"),
        _ => ("glass_pane_shatter", "glass_pane_blowout"),
    }
}

struct ClientGlassTrace<'a>(&'a sim::CollisionScene);

impl fx::GlassWorldTrace for ClientGlassTrace<'_> {
    fn sweep(&self, start: [f32; 3], end: [f32; 3]) -> Option<fx::GlassWorldContact> {
        let hit = self
            .0
            .trace_static_world(start, end, [0.0; 3], [0.0; 3], sim::MASK_SHOT);
        if hit.startsolid != 0 {
            return Some(fx::GlassWorldContact {
                fraction: 0.0,
                end: start,
                normal: hit.normal,
                startsolid: true,
                pane: sim::glass_piece_from_hit(hit.hit_type, hit.hit_id),
            });
        }
        if hit.fraction >= 1.0 {
            return None;
        }
        Some(fx::GlassWorldContact {
            fraction: hit.fraction,
            end: hit.endpos,
            normal: hit.normal,
            startsolid: false,
            pane: sim::glass_piece_from_hit(hit.hit_type, hit.hit_id),
        })
    }
}

pub(crate) fn tick_fx_remaining_update(
    runtime: FxRuntimeAccess,
    collision: Option<Res<net::ClientCollision>>,
    camera_origin: Res<FxCameraOrigin>,
    fx_world: FxSceneAccess,
    dobj_poses: Option<Res<crate::adapters::anim::dobj_pose::HostDObjPoseFrame>>,
) {
    let FxRuntimeAccess {
        catalog,
        mut elem_infos,
        mut host,
    } = runtime;

    let msec = host.0.msec_now;
    let Some(catalog) = catalog else {
        return;
    };
    let _fx_update = perf::span!("fx_update", "iw4l.fx");
    let clip_world = collision
        .as_ref()
        .filter(|scene| scene.0.has_world_clip())
        .map(|scene| &scene.0);
    host.0.refresh_bolt_poses(|dobj, bone| {
        dobj_poses
            .as_deref()
            .and_then(|poses| poses.resolve_live_bolt(dobj, bone))
    });
    elem_infos.0.sync(&catalog.0);
    tick_fx_remaining(
        &mut host.0,
        &catalog.0,
        &elem_infos.0,
        msec,
        camera_origin.0,
        clip_world,
        fx_world.view().as_ref().map(|s| s as &dyn FxScene),
    );
    perf::counter!("fx_elem_live", "iw4l.fx", f64::from(host.0.elem_live_count));
    perf::counter!(
        "fx_elem_alloc_fail",
        "iw4l.fx",
        f64::from(host.0.elem_alloc_failures)
    );
}

#[derive(SystemParam)]
struct FxPresentEnv<'w> {
    outdoor: Option<Res<'w, MapOutdoor>>,
    dlights: ResMut<'w, HostFxDlights>,
    post_lights: ResMut<'w, HostFxPostLights>,
    models: Option<Res<'w, PreparedFxModels>>,
    model_geometry: Option<Res<'w, render_fx::PreparedFxModelGeometry>>,
    atlas: Option<Res<'w, WorldModelLightingAtlas>>,
    atpoint: Res<'w, render_scene::DynAtPointLookup>,
    lod_skinned: Res<'w, LodRampSkinnedDvar>,
    staged_models: ResMut<'w, render_fx::FxModelStaging>,
}

#[derive(SystemParam)]
struct EntityMarkGeneration<'w, 's> {
    models: Option<Res<'w, asset_world::MapXModelSceneCatalog>>,
    owners: Query<
        'w,
        's,
        (
            &'static crate::prepare::scene::world::WorldScriptModelInstance,
            &'static Transform,
            &'static Visibility,
        ),
    >,
    xanims: Option<Res<'w, assets::PreparedXAnims>>,
}

impl EntityMarkGeneration<'_, '_> {
    fn generate(&self, host: &mut FxSystemHost, fx_world: &FxSceneAccess) {
        if let (Some(scene), Some(models)) = (fx_world.scene(), self.models.as_deref()) {
            super::entity_mark::generate(
                host,
                scene,
                fx_world.marks(),
                &self.owners,
                self.xanims.as_deref(),
                models,
            );
        }
    }
}

#[derive(SystemParam)]
struct FxFrameGenerator<'w> {
    host: ResMut<'w, HostFxSystem>,
    catalog: Option<Res<'w, PreparedFxCatalog>>,
    dvars: Res<'w, FxMarkDvars>,
    scene_view: Option<Res<'w, crate::prepare::scene::view_parms::PreparedSceneView>>,
    tracers: ResMut<'w, TracerWorld>,
}

impl FxFrameGenerator<'_> {
    fn generate(
        &mut self,
        cam_q: Query<&Transform, With<FlyCamera>>,
        fx_world: FxSceneAccess,
        lasers: &mut TagLaserLights,
        marks: &EntityMarkGeneration,
    ) -> FxFrameTransaction {
        let present_span = perf::span!("fx_present", "iw4l.fx");
        let Some(catalog) = self.catalog.as_ref() else {
            return FxFrameTransaction {
                outcome: FxFrameOutcome::Unavailable(FxUnavailableCause::CatalogAbsent),
                _present_span: present_span,
            };
        };
        let cam_tf = cam_q.iter().next().copied().unwrap_or(Transform::IDENTITY);
        lasers.queue(cam_tf.translation.to_array());
        self.host.0.generate_world_mark_verts();
        marks.generate(&mut self.host.0, &fx_world);

        restamp_missing_packed_lighting(
            &mut self.host.0,
            fx_world.view().as_ref().map(|s| s as &dyn FxScene),
        );
        let frustum_planes = self
            .scene_view
            .as_ref()
            .map(|view| view.frustum_planes.to_vec())
            .unwrap_or(Vec::new());
        let (out, verts_gaps) = build_fx_verts(
            &mut self.host.0,
            &catalog.0,
            FxDrawCull {
                elem_draw: self.dvars.fx_cull_elem_draw,
                planes: &frustum_planes,
            },
            cam_tf.translation.to_array(),
        );
        for def_index in &verts_gaps.lighting_frac_def_indices {
            diag::warn_once!(
                World,
                "elem {def_index} authored lightingFrac with no light grid",
                def_index = *def_index
            );
        }
        let mut vis_write = self.host.0.vis_blocker_write;
        let mut vis_read = self.host.0.vis_blocker_read;
        fx_iw4::vis_blocker_generate_verts(&mut vis_write, &mut vis_read);
        self.host.0.vis_blocker_write = vis_write;
        self.host.0.vis_blocker_read = vis_read;
        tick_tracer_beams(&mut self.tracers, FxMsec(self.host.0.msec_now));

        FxFrameTransaction {
            outcome: FxFrameOutcome::Generated(FxGeneratedFrame {
                out,
                verts_gaps,
                mark_mesh: self.host.0.last_mark_mesh.take(),
                post_lights: lasers.take(),
                tracers: std::mem::take(&mut *self.tracers),
                cam_tf,
                frustum_planes,
                clip_from_world: self
                    .scene_view
                    .as_ref()
                    .map(|view| view.clip_from_world.to_cols_array()),
                tan_half_fov: self
                    .scene_view
                    .as_ref()
                    .map(|view| tan_half_fov_from_clip(view.clip_from_view)),
                world_present: fx_world.scene().is_some_and(|scene| scene.spawned),
            }),
            _present_span: present_span,
        }
    }
}

fn generate_fx_transaction(
    _main_thread: bevy::ecs::system::NonSendMarker,
    cam_q: Query<&Transform, With<FlyCamera>>,
    fx_world: FxSceneAccess,
    mut lasers: TagLaserLights,
    marks: EntityMarkGeneration,
    mut generator: FxFrameGenerator,
) -> FxFrameTransaction {
    generator.generate(cam_q, fx_world, &mut lasers, &marks)
}

#[expect(
    clippy::too_many_arguments,
    reason = "Bevy system parameters keep scheduler resource access explicit."
)]
fn commit_fx_transaction(
    In(transaction): In<FxFrameTransaction>,
    mut host: ResMut<HostFxSystem>,
    catalog: Option<Res<PreparedFxCatalog>>,
    color_images: Res<FxWorldColorImages>,
    runtime: Res<MaterialGeneration>,
    scene: Option<Res<WorldScene>>,
    journal: FxCombatJournal,
    mut plan: ResMut<FxCodeMeshPlan>,
    mut spark_plan: ResMut<FxParticleCloudPlan>,
    mut mark_plan: ResMut<GfxMarkMeshPlan>,
    mut model_plan: ResMut<FxModelDrawPlan>,
    mut dump_req: ResMut<FxDumpRequest>,
    (mut tracer_world, flames): (ResMut<TracerWorld>, Res<super::flame::FlameStreams>),
    mut env: FxPresentEnv,
    lighting_cache: Option<Res<WorldModelLightingCache>>,
    mut lighting_requests: ResMut<ModelLightingRequests>,
) {
    let FxCombatJournal {
        mut cursor,
        mut combat,
    } = journal;

    let FxFrameTransaction {
        outcome,
        _present_span,
    } = transaction;
    clear_fx_owned_plans(&mut plan, &mut spark_plan, &mut mark_plan);
    env.staged_models.0.clear();
    if let Some(prepared) = &env.model_geometry {
        env.staged_models.0.use_prepared(prepared);
    } else {
        env.staged_models.0 = FxModelDrawPlan::default();
    }
    env.dlights.scene.clear();
    env.dlights.cap_full = 0;
    env.post_lights.queued.clear();
    env.post_lights.cap_full = 0;
    env.post_lights.drawn = 0;
    env.post_lights.skipped_short = 0;
    env.post_lights.miss_material = 0;

    let generated = match outcome {
        FxFrameOutcome::Generated(generated) => generated,
        FxFrameOutcome::Unavailable(FxUnavailableCause::CatalogAbsent) => {
            host.0.last_mark_mesh = None;
            fill_mark_mesh_plan(&mut host.0, None, &mut mark_plan, &runtime);
            publish_empty_fx_owned_plans(&mut plan, &mut spark_plan);
            return;
        }
    };
    let FxGeneratedFrame {
        out,
        verts_gaps,
        mark_mesh,
        post_lights,
        tracers,
        cam_tf,
        frustum_planes,
        clip_from_world,
        tan_half_fov,
        world_present,
    } = generated;
    let Some(catalog) = catalog else {
        *tracer_world = tracers;
        fill_mark_mesh_plan(&mut host.0, None, &mut mark_plan, &runtime);
        publish_empty_fx_owned_plans(&mut plan, &mut spark_plan);
        return;
    };
    let mut zero_half = 0u32;

    fill_mark_mesh_plan(&mut host.0, mark_mesh, &mut mark_plan, &runtime);
    let planes = frustum_planes.as_slice();
    env.fill_model_plan(
        &out.models,
        scene.as_deref(),
        cam_tf.translation,
        planes,
        lighting_cache.as_deref(),
        &mut lighting_requests,
    );
    model_plan.publish_rebuild(&mut env.staged_models.0);
    let spot_cone = lighting_iw4::SpotLightConeDvars::register_defaults();
    for light in &out.spot_lights {
        match lighting_iw4::add_omni_light_to_scene_allows(
            world_present,
            light.radius,
            env.dlights.scene.len() as u32,
        ) {
            Ok(()) => env.dlights.scene.push(lighting_iw4::spot_light_pack(
                light.origin,
                light.axis[0],
                light.radius,
                light.color_rgb,
                spot_cone,
            )),
            Err(lighting_iw4::AddOmniLightRefuse::Cap) => {
                env.dlights.cap_full = env.dlights.cap_full.saturating_add(1);
            }
            Err(_) => {}
        }
    }
    for light in &out.omni_lights {
        match lighting_iw4::add_omni_light_to_scene_allows(
            world_present,
            light.radius,
            env.dlights.scene.len() as u32,
        ) {
            Ok(()) => env.dlights.scene.push(lighting_iw4::omni_light_pack(
                light.origin,
                light.radius,
                light.color_rgb,
            )),
            Err(lighting_iw4::AddOmniLightRefuse::Cap) => {
                env.dlights.cap_full = env.dlights.cap_full.saturating_add(1);
            }
            Err(_) => {}
        }
    }
    let eye = cam_tf.translation.to_array();
    for light in &post_lights {
        let Some(tess) = fx_iw4::post_light_generate_verts(light, eye) else {
            env.post_lights.skipped_short = env.post_lights.skipped_short.saturating_add(1);
            continue;
        };
        match code_mesh_bind(light.material_name, &color_images, &runtime) {
            FxCodeMeshBind::Skip(cause) => {
                count_fx_present_skip(&mut cursor, cause);
                env.post_lights.miss_material = env.post_lights.miss_material.saturating_add(1);
                continue;
            }
            FxCodeMeshBind::Ready {
                color,
                sort_key,
                ordinal,
            } => {
                let slot = plan.begin_material_draw(color, sort_key, Some(ordinal));
                if !plan.push_post_light(&tess) {
                    plan.end_material_draw(slot);
                    continue;
                }
                plan.end_material_draw(slot);
                env.post_lights.drawn = env.post_lights.drawn.saturating_add(1);
            }
        }
    }

    let mut batches: HashMap<(usize, bool), Vec<&FxSpriteInstance>> = HashMap::new();
    let sprites_total = out.sprites.len();
    for sprite in &out.sprites {
        let Some(asset_id) = sprite.material_index else {
            cursor.draw_miss_material = cursor.draw_miss_material.saturating_add(1);
            plan.miss_material = plan.miss_material.saturating_add(1);
            continue;
        };
        batches
            .entry((asset_id, sprite.viewmodel))
            .or_default()
            .push(sprite);
    }

    let mut drawn = 0usize;
    let mut alpha_min = u8::MAX;
    let mut alpha_max = 0u8;
    let mut size_min = f32::MAX;
    let mut size_max = 0.0f32;
    let mut dist_min = f32::MAX;
    let mut dist_max = 0.0f32;
    let mut in_front = 0u32;
    let mut behind = 0u32;
    let cam_fwd = cam_tf.forward();
    for ((asset_id, viewmodel), sprites) in &batches {
        match code_mesh_bind_asset(*asset_id, &color_images, &runtime) {
            FxCodeMeshBind::Skip(cause) => count_fx_present_skip(&mut cursor, cause),
            FxCodeMeshBind::Ready {
                color,
                sort_key,
                ordinal,
            } => {
                if sort_key == 0 && ordinal == 0 {
                    zero_half = zero_half.saturating_add(1);
                }
                let slot = plan.begin_material_draw(color, sort_key, Some(ordinal));
                plan.draws[slot as usize].viewmodel = *viewmodel
                    || runtime
                        .catalog
                        .derived(assets::MaterialIndex::from_order(*asset_id))
                        .is_some_and(|material| {
                            material.camera_region == asset_iw4::CAMERA_REGION_DEPTH_HACK
                        });
                for sprite in sprites {
                    if !plan.push_quad(
                        sprite_transform(sprite, &cam_tf),
                        sprite.color_rgba,
                        sprite.atlas,
                    ) {
                        continue;
                    }
                    drawn = drawn.saturating_add(1);
                    let alpha = sprite.color_rgba[3];
                    alpha_min = alpha_min.min(alpha);
                    alpha_max = alpha_max.max(alpha);
                    size_min = size_min.min(sprite.size0);
                    size_max = size_max.max(sprite.size0);
                    let delta = Vec3::from_array(sprite.origin) - cam_tf.translation;
                    let dist = delta.length();
                    dist_min = dist_min.min(dist);
                    dist_max = dist_max.max(dist);
                    if delta.dot(*cam_fwd) > 0.0 {
                        in_front = in_front.saturating_add(1);
                    } else {
                        behind = behind.saturating_add(1);
                    }
                }
                plan.end_material_draw(slot);
            }
        }
    }

    let mut trails_drawn = 0usize;
    for mesh in &out.trail_meshes {
        let Some(asset_id) = mesh.material_index else {
            diag::warn_once!(
                World,
                "a CPU ribbon was refused at the {step}",
                step = "material bind"
            );
            cursor.draw_miss_material = cursor.draw_miss_material.saturating_add(1);
            continue;
        };
        match code_mesh_bind_asset(asset_id, &color_images, &runtime) {
            FxCodeMeshBind::Skip(cause) => count_fx_present_skip(&mut cursor, cause),
            FxCodeMeshBind::Ready {
                color,
                sort_key,
                ordinal,
            } => {
                if sort_key == 0 && ordinal == 0 {
                    zero_half = zero_half.saturating_add(1);
                }
                let slot = plan.begin_material_draw(color, sort_key, Some(ordinal));
                let vert_used = plan.mesh.vert_used;
                let index_used = plan.mesh.index_used;
                let mut verts_ok = true;
                for v in &mesh.verts {
                    if !plan.push_trail_vert(
                        v.xyz,
                        v.color_rgba,
                        v.texcoord_packed,
                        v.normal_packed,
                        v.tangent_packed,
                    ) {
                        verts_ok = false;
                        break;
                    }
                }
                if !verts_ok {
                    plan.shrink_verts_to(vert_used);
                    diag::warn_once!(
                        World,
                        "a CPU ribbon was refused at the {step}",
                        step = "vert reserve"
                    );
                    plan.end_material_draw(slot);
                    continue;
                }
                let base = vert_used;

                let mut index_ok = true;
                for chunk in mesh.index_pairs.as_chunks::<3>().0 {
                    let pairs = [chunk[0], chunk[1], chunk[2]];
                    let tris = fx_iw4::trail_index_quad_tris(pairs);
                    let mut six = [0u32; 6];
                    let mut i = 0;
                    for tri in tris {
                        six[i] = base.saturating_add(u32::from(tri[0]));
                        six[i + 1] = base.saturating_add(u32::from(tri[1]));
                        six[i + 2] = base.saturating_add(u32::from(tri[2]));
                        i += 3;
                    }
                    if !plan.extend_indices(&six) {
                        index_ok = false;
                        break;
                    }
                }
                if !index_ok {
                    plan.shrink_verts_to(vert_used);
                    plan.shrink_indices_to(index_used);
                    diag::warn_once!(
                        World,
                        "a CPU ribbon was refused at the {step}",
                        step = "index reserve"
                    );
                    plan.end_material_draw(slot);
                    continue;
                }
                if mesh.index_pairs.len() % 3 != 0 {
                    diag::warn_once!(
                        World,
                        "a CPU ribbon was refused at the {step}",
                        step = "index reserve"
                    );
                }
                plan.end_material_draw(slot);
                trails_drawn = trails_drawn.saturating_add(1);
            }
        }
    }

    present_tracer_beams(
        &tracers,
        &cam_tf,
        clip_from_world,
        tan_half_fov,
        &mut plan,
        &mut combat,
        |asset_id| {
            let has_color = color_images.colors_by_asset.contains_key(&asset_id);
            let binding = code_mesh_bind_asset(asset_id, &color_images, &runtime);
            (has_color, binding)
        },
    );
    *tracer_world = tracers;
    flames.present(&cam_tf, &color_images, &runtime, host.0.msec_now, &mut plan);

    let mut spark_drawn = 0usize;
    for spark in &out.spark_clouds {
        let Some(asset_id) = spark_elem_asset_id(&catalog.0, spark.catalog_index, spark.def_index)
        else {
            spark_plan.miss_material = spark_plan.miss_material.saturating_add(1);
            continue;
        };
        match code_mesh_bind_asset(asset_id, &color_images, &runtime) {
            FxCodeMeshBind::Skip(cause) => count_fx_present_skip(&mut cursor, cause),
            FxCodeMeshBind::Ready {
                color,
                sort_key,
                ordinal,
            } => {
                spark_plan.begin_material_draw(color, sort_key, Some(ordinal), spark.clouds);
                spark_drawn = spark_drawn.saturating_add(1);
            }
        }
    }
    for cloud in &out.clouds {
        let Some(asset_id) = spark_elem_asset_id(&catalog.0, cloud.catalog_index, cloud.def_index)
        else {
            spark_plan.miss_material = spark_plan.miss_material.saturating_add(1);
            continue;
        };
        match code_mesh_bind_asset(asset_id, &color_images, &runtime) {
            FxCodeMeshBind::Skip(cause) => count_fx_present_skip(&mut cursor, cause),
            FxCodeMeshBind::Ready {
                color,
                sort_key,
                ordinal,
            } => {
                spark_plan.begin_material_draw(
                    color,
                    sort_key,
                    Some(ordinal),
                    [cloud.cloud, cloud.cloud, cloud.cloud],
                );
            }
        }
    }
    for fountain in &out.fountains {
        let Some(asset_id) =
            spark_elem_asset_id(&catalog.0, fountain.catalog_index, fountain.def_index)
        else {
            spark_plan.miss_material = spark_plan.miss_material.saturating_add(1);
            continue;
        };
        match code_mesh_bind_asset(asset_id, &color_images, &runtime) {
            FxCodeMeshBind::Skip(cause) => count_fx_present_skip(&mut cursor, cause),
            FxCodeMeshBind::Ready {
                color,
                sort_key,
                ordinal,
            } => {
                if spark_plan
                    .begin_custom_draw(
                        color,
                        sort_key,
                        Some(ordinal),
                        fountain.cloud,
                        &fountain.cells,
                    )
                    .is_none()
                {
                    spark_plan.miss_material = spark_plan.miss_material.saturating_add(1);
                }
            }
        }
    }
    spark_plan.bump();
    spark_plan.publish_share();

    plan.bump();
    plan.publish_share();

    let cam_origin = cam_tf.translation.to_array();
    let first_census = !cursor.draw_logged
        && (sprites_total > 0
            || !out.trail_meshes.is_empty()
            || out.skipped_unsupported_type > 0
            || out.skipped_no_trail_def > 0
            || !out.omni_lights.is_empty()
            || !out.spot_lights.is_empty()
            || drawn > 0
            || trails_drawn > 0
            || combat.beam_queued > 0
            || combat.tracer_live > 0);
    let dump_now = dump_req.pending || first_census;
    if dump_now {
        let (alpha_min, alpha_max, size_min, size_max, dist_min, dist_max) = if drawn == 0 {
            (0, 0, 0.0, 0.0, 0.0, 0.0)
        } else {
            (alpha_min, alpha_max, size_min, size_max, dist_min, dist_max)
        };
        diag::info!(
            World,
            "fx: generate_verts sprites={sprites_total} drawn={drawn} miss_material={} \
             trails={} trails_drawn={trails_drawn} miss_lookup={} unsupported={} \
             cloud={} spark_cloud={} spark_cpu={} spark_hist_empty={} spark_no_size1={} \
             spark_vis_size1={:?} fountain={} skipped_fountain={} model_cpu={} model_draws={} \
             model_skip_lookup={} model_skip_catalog={} model_skip_pose={} model_skip_lod={} model_skip_culled={} model_skip_material={} model_skip_lighting={} model_skip_flags={} \
             omni_cpu={} scene_dlights={} \
             omni_skip={} omni_cap={} spot_cpu={} spot_skip={} postlight={} skipped_postlight={} postlight_miss={} \
             vis_blocker_w={} vis_blocker_r={} other_type={} \
             null_handler={} no_trail_def={} dormant={} \
             zero_material_half={} codemesh_draws={} \
             spark_drawn={spark_drawn} sparkcloud_draws={} spark_miss_material={} \
             skip_no_ordinal={} skip_not_emissive={} \
             lookup_split=[no_def={} no_elem={} no_material_visual={} no_size0={} \
             size0<=0={} no_size1={} size1<=0={}] \
             exact_sprites alpha={alpha_min}..{alpha_max} size0={size_min:.2}..{size_max:.2} \
             dist={dist_min:.1}..{dist_max:.1} in_front={in_front} behind={behind}",
            cursor.draw_miss_material,
            out.trail_meshes.len(),
            out.skipped_no_lookup,
            out.skipped_unsupported_type,
            out.skipped_cloud,
            out.skipped_spark_cloud,
            out.spark_clouds.len(),
            out.spark_cloud_history_empty,
            out.spark_cloud_no_size1,
            out.spark_clouds.first().map(|s| s.vis_size1),
            out.fountains.len(),
            out.skipped_spark_fountain,
            out.models.len(),
            model_plan.draws().len(),
            out.skipped_model,
            model_plan.skipped_no_catalog,
            model_plan.skipped_no_pose,
            model_plan.skipped_no_lod,
            model_plan.skipped_culled,
            model_plan.skipped_no_material,
            model_plan.skipped_no_lighting,
            model_plan.skipped_render_fx_flags,
            out.omni_lights.len(),
            env.dlights.scene.len(),
            out.skipped_omni_light,
            env.dlights.cap_full,
            out.spot_lights.len(),
            out.skipped_spot_light,
            env.post_lights.drawn,
            env.post_lights.skipped_short,
            env.post_lights.miss_material,
            host.0.vis_blocker_write.count,
            host.0.vis_blocker_read.count,
            out.skipped_other_type,
            out.skipped_null_handler,
            out.skipped_no_trail_def,
            out.skipped_dormant,
            zero_half,
            plan.draws.len(),
            spark_plan.draws.len(),
            spark_plan.miss_material,
            cursor.skipped_no_ordinal,
            cursor.skipped_not_emissive,
            verts_gaps.no_def,
            verts_gaps.no_elem,
            verts_gaps.no_material_visual,
            verts_gaps.no_size0_sample,
            verts_gaps.size0_not_positive,
            verts_gaps.no_size1_sample,
            verts_gaps.size1_not_positive
        );
        diag::info!(
            World,
            "fx: tracer live={} queued={} drawn={} miss_mat={} miss_color={} miss_unprep={} miss_ord={} miss_emis={} spawned={} skip_interval={} skip_short={} skip_no_def={} name={:?} mat={:?} bind={:?} has_color={:?} speed={:?} beam_len={:?}",
            combat.tracer_live,
            combat.beam_queued,
            combat.beam_drawn,
            combat.beam_miss_material,
            combat.beam_miss_color,
            combat.beam_miss_unprepared,
            combat.beam_miss_ordinal,
            combat.beam_miss_emissive,
            combat.tracer_spawned,
            combat.tracer_skip_interval,
            combat.tracer_skip_short,
            combat.tracer_skip_no_def,
            combat.last_tracer_name,
            combat.last_tracer_material,
            combat.last_tracer_bind,
            combat.last_tracer_has_color,
            combat.last_tracer_speed,
            combat.last_tracer_beam_length
        );
        log_fx_draw_summary(
            &out,
            &batches,
            cam_origin,
            *cam_fwd,
            dump_req.radius,
            &spark_plan,
            env.outdoor.as_ref().and_then(|o| o.image).map(|id| id.0),
        );
        log_fx_near_camera(
            &host.0,
            &catalog.0,
            &out.sprites,
            &out.spark_clouds,
            cam_origin,
            *cam_fwd,
            dump_req.radius,
        );
        cursor.draw_logged = true;
        dump_req.pending = false;
    }
}

impl FxPresentEnv<'_> {
    fn fill_model_plan(
        &mut self,
        instances: &[fx::FxModelInstance],
        scene: Option<&WorldScene>,
        camera_origin: Vec3,
        frustum_planes: &[[f32; 4]],
        cache: Option<&WorldModelLightingCache>,
        lighting_requests: &mut ModelLightingRequests,
    ) {
        let models = self.models.as_deref().map(|prepared| &prepared.0);
        let atlas = self.atlas.as_deref();
        let atpoint: &render_scene::DynAtPointLookup = &self.atpoint;
        let lod_ramp = self.lod_skinned.args();
        let plan = &mut self.staged_models.0;
        plan.generated = u32::try_from(instances.len()).unwrap_or(u32::MAX);
        if instances.is_empty() {
            return;
        }
        let Some(models) = models.filter(|catalog| !catalog.is_empty()) else {
            plan.skipped_no_catalog = plan.generated;
            return;
        };
        let scene_atlas = scene.and_then(|world| {
            Some(WorldModelLightingAtlas {
                image: world.model_lighting_image.clone()?,
                dims: world.model_lighting_dims?,
            })
        });
        let Some(_) = atlas.or(scene_atlas.as_ref()) else {
            plan.skipped_no_lighting = plan.generated;
            return;
        };
        let Some(_) = scene else {
            plan.skipped_no_lighting = plan.generated;
            return;
        };
        if cache.is_none() {
            plan.skipped_no_lighting = plan.generated;
            return;
        }

        for instance in instances {
            let Some(entry) = models.get_at(instance.model_index) else {
                plan.skipped_no_catalog = plan.skipped_no_catalog.saturating_add(1);
                continue;
            };
            if instance.flags & 0x800 != 0 {
                plan.skipped_render_fx_flags = plan.skipped_render_fx_flags.saturating_add(1);
                continue;
            }
            let Some(lod) = crate::assemble::drawsurf::tess::smodel::smodel_camera_lod(
                entry.skel.lod,
                instance.origin,
                instance.scale.abs(),
                Some(camera_origin),
                &lod_ramp,
            ) else {
                plan.skipped_no_lod = plan.skipped_no_lod.saturating_add(1);
                continue;
            };
            if entry.skel.radius.is_some_and(|radius| {
                dpvs_iw4::scene_ent_sphere_hides(
                    instance.origin,
                    radius * instance.scale.abs(),
                    frustum_planes,
                )
            }) {
                plan.skipped_culled = plan.skipped_culled.saturating_add(1);
                continue;
            }
            let Some(asset) = plan.asset(instance.model_index, lod) else {
                plan.skipped_no_material = plan.skipped_no_material.saturating_add(1);
                continue;
            };
            let asset_surfaces = asset.surfaces.clone();
            if !plan.packed_exact() {
                plan.skipped_no_material = plan.skipped_no_material.saturating_add(1);
                continue;
            }

            let box_half = entry.skel.radius.and_then(|radius| {
                crate::prepare::scene::model_lighting_cache::lighting_box_half(
                    &[radius * instance.scale.abs()],
                    &[anim_iw4::DOBJ_RADIUS_PARENT_ROOT],
                )
            });
            let lookup_fallback = atpoint.fallback(instance.origin, box_half);
            let lighting = lighting_requests.request(ModelLightingRequest {
                owner: ModelLightingOwner::FxModel(instance.elem_handle),
                origin: instance.origin,
                lookup_fallback,
                hero_lighting: false,
            });
            let quat = Quat::from_array(fx_iw4::axis_to_quat(instance.axis));
            let world_from_local = Mat4::from_scale_rotation_translation(
                Vec3::splat(instance.scale),
                quat,
                Vec3::from_array(instance.origin),
            );
            let caster_bound = entry
                .skel
                .radius
                .map(|radius| render_scene::XModelCasterBound {
                    origin: instance.origin,
                    radius: (radius * instance.scale.abs()).max(1.0),
                });
            for (surface, material) in asset_surfaces {
                plan.push_draw(crate::assemble::drawsurf::tess::xmodel::XModelSurfaceDraw {
                    surface,
                    material,
                    world_from_local,
                    lighting_handle: 0,
                    pending_lighting: Some(lighting),
                    colour_refusal: None,
                    object_id: instance.elem_handle,
                    scene_light_index: 0,
                    reflection_probe_index: 0,
                    packed_lighting: None,
                    is_scope: false,
                    scene_entnum: None,
                    body_client: None,
                    caster_bound,
                });
            }
        }
    }
}

fn log_fx_draw_summary(
    out: &fx::FxGenerateVertsOut,
    batches: &HashMap<(usize, bool), Vec<&FxSpriteInstance>>,
    cam: [f32; 3],
    cam_fwd: Vec3,
    radius: f32,
    spark_plan: &FxParticleCloudPlan,
    outdoor_image: Option<u32>,
) {
    let spark_clouds = &out.spark_clouds;
    let spark_cloud = out.skipped_spark_cloud;
    let cloud = out.skipped_cloud;
    let spark_hist_empty = out.spark_cloud_history_empty;
    let spark_tmpl = spark_plan.tmpl_first_xyz;
    let spark_tmpl_r2 = spark_plan.tmpl_first_r2;
    let spark_tmpl_holdrand = spark_plan.tmpl_holdrand;
    let radius = radius.max(1.0);
    let mut batch_rows: Vec<(f32, String)> = batches
        .iter()
        .map(|((mat, viewmodel), list)| {
            let nearest = list
                .iter()
                .map(|s| sprite_dist(s.origin, cam))
                .fold(f32::MAX, f32::min);
            let amax = list.iter().map(|s| s.color_rgba[3]).max().unwrap_or(0);
            (
                nearest,
                format!(
                    "#{mat} `{}` viewmodel={viewmodel} n={} near={nearest:.0} amax={amax}",
                    list.first()
                        .map(|s| s.material_name.as_ref())
                        .unwrap_or("?"),
                    list.len()
                ),
            )
        })
        .collect();
    batch_rows.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal));
    let batch_summary = batch_rows
        .iter()
        .take(12)
        .map(|(_, row)| row.as_str())
        .collect::<Vec<_>>()
        .join(" | ");
    diag::info!(
        World,
        "fx_dump cam={:.0},{:.0},{:.0} fwd={:.2},{:.2},{:.2} radius={radius:.0} spark_cloud={spark_cloud} \
         spark_cpu={} spark_hist_empty={spark_hist_empty} cloud={cloud} outdoor_image={} \
         spark_tmpl={:.5},{:.5},{:.5} r2={:.5} holdrand=0x{:08x} batches={}",
        cam[0],
        cam[1],
        cam[2],
        cam_fwd.x,
        cam_fwd.y,
        cam_fwd.z,
        spark_clouds.len(),
        outdoor_image
            .map(|id| id.to_string())
            .unwrap_or_else(|| "NONE".into()),
        spark_tmpl[0],
        spark_tmpl[1],
        spark_tmpl[2],
        spark_tmpl_r2,
        spark_tmpl_holdrand,
        batch_summary
    );
}

fn log_fx_near_camera(
    host: &FxSystemHost,
    catalog: &FxDefinitions,
    sprites: &[FxSpriteInstance],
    spark_clouds: &[FxSparkCloudInstance],
    cam: [f32; 3],
    cam_fwd: Vec3,
    radius: f32,
) {
    let radius = radius.max(1.0);
    let mut near: Vec<&FxSpriteInstance> = sprites.iter().collect();
    near.sort_by(|a, b| {
        sprite_dist(a.origin, cam)
            .partial_cmp(&sprite_dist(b.origin, cam))
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    let inside = near
        .iter()
        .filter(|s| sprite_dist(s.origin, cam) <= radius)
        .count();
    diag::info!(
        World,
        "fx_dump near_total={} inside_radius={inside}",
        near.len()
    );
    log_fx_run_mode_census(host, sprites, catalog, cam, radius);
    log_fx_impacts(host, sprites, cam, cam_fwd);
    for sprite in near.iter().take(12) {
        log_fx_near_sprite("fx_near", sprite, cam, cam_fwd);
    }

    for sprite in near.iter().filter(|s| s.atlas.entry_count > 1).take(8) {
        log_fx_near_sprite("fx_near_cell", sprite, cam, cam_fwd);
    }

    for sprite in near
        .iter()
        .filter(|s| s.atlas.entry_count > 1)
        .filter(|s| {
            let delta = Vec3::from_array(s.origin) - Vec3::from_array(cam);
            delta.dot(cam_fwd) > 0.0
        })
        .take(8)
    {
        log_fx_near_sprite("fx_near_front_cell", sprite, cam, cam_fwd);
    }
    if let Some(sprite) = sprites
        .iter()
        .filter(|s| s.atlas.entry_count > 1)
        .max_by(|a, b| {
            a.size0
                .partial_cmp(&b.size0)
                .unwrap_or(std::cmp::Ordering::Equal)
        })
    {
        log_fx_near_sprite("fx_near_cell_max", sprite, cam, cam_fwd);
    }
    if let Some(sprite) = sprites.iter().max_by(|a, b| {
        a.size0
            .partial_cmp(&b.size0)
            .unwrap_or(std::cmp::Ordering::Equal)
    }) {
        log_fx_near_sprite("fx_near_size_max", sprite, cam, cam_fwd);
    }

    let small: Vec<&FxSpriteInstance> = sprites
        .iter()
        .filter(|s| s.def_name.contains("firelp_small"))
        .collect();
    let small_dist = small
        .iter()
        .map(|s| sprite_dist(s.origin, cam))
        .fold(f32::MAX, f32::min);
    let (sy_min, sy_max) = small.iter().fold((f32::MAX, f32::MIN), |(mn, mx), s| {
        (mn.min(s.origin[1]), mx.max(s.origin[1]))
    });
    diag::info!(
        World,
        "fx_firelp_small n={} dist_min={} y={:.0}..{:.0}",
        small.len(),
        if small.is_empty() {
            "NULL".into()
        } else {
            format!("{small_dist:.0}")
        },
        if small.is_empty() { 0.0 } else { sy_min },
        if small.is_empty() { 0.0 } else { sy_max }
    );
    for effect in host
        .live_effects()
        .filter(|e| e.def_name.contains("firelp_small"))
    {
        let pending = (effect.status & fx_iw4::FX_STATUS_HAS_PENDING_LOOP_ELEMS) != 0;
        let dist = sprite_dist(effect.origin, cam);
        diag::info!(
            World,
            "fx_held_small origin={:.0},{:.0},{:.0} dist={dist:.0} msec_begin={} pending={pending} def={}",
            effect.origin[0],
            effect.origin[1],
            effect.origin[2],
            effect.msec_begin,
            effect.def_name
        );
    }
    for sprite in small.iter().take(8) {
        log_fx_near_sprite("fx_near_small", sprite, cam, cam_fwd);
    }
    for spark in spark_clouds.iter().take(8) {
        let dist = sprite_dist(spark.origin, cam);
        let mat = spark_elem_material(catalog, spark.catalog_index, spark.def_index)
            .unwrap_or_else(|| "-".into());
        let c0 = &spark.clouds[0];
        let c1 = &spark.clouds[1];
        let c2 = &spark.clouds[2];
        diag::info!(
            World,
            "fx_spark cpu writeIdx={} size0={:.1} size1={:.1} scale={:.1}/{:.1}/{:.1} flags=0x{:x} \
             def={} mat={} origin={:.0},{:.0},{:.0} pos1={:.0},{:.0},{:.0} pos2={:.0},{:.0},{:.0} \
             dist={dist:.0}",
            spark.write_idx,
            spark.size0,
            c0.size1,
            c0.placement_scale,
            c1.placement_scale,
            c2.placement_scale,
            c0.flags,
            spark.def_name,
            mat,
            spark.origin[0],
            spark.origin[1],
            spark.origin[2],
            c1.pos[0],
            c1.pos[1],
            c1.pos[2],
            c2.pos[0],
            c2.pos[1],
            c2.pos[2]
        );
    }
}

fn is_impact_def(name: &str) -> bool {
    name.contains("impacts/")
}

fn log_fx_impacts(host: &FxSystemHost, sprites: &[FxSpriteInstance], cam: [f32; 3], cam_fwd: Vec3) {
    for sprite in sprites
        .iter()
        .filter(|sprite| is_impact_def(&sprite.def_name))
        .take(16)
    {
        log_fx_near_sprite("fx_near_impact", sprite, cam, cam_fwd);
    }
    for effect in host
        .live_effects()
        .filter(|effect| is_impact_def(&effect.def_name))
    {
        let dist = sprite_dist(effect.origin, cam);
        diag::info!(
            World,
            "fx_held_impact origin={:?} dist={dist:.0} msec_begin={} status=0x{:x} def={}",
            effect.origin,
            effect.msec_begin,
            effect.status,
            effect.def_name
        );
    }
    for (elem, effect) in host
        .live_elems()
        .filter_map(|elem| {
            let effect = host.effect_at(elem.owner_effect_slot as usize)?;
            is_impact_def(&effect.def_name).then_some((elem, effect))
        })
        .take(16)
    {
        diag::info!(
            World,
            "fx_elem_impact stored={:?} run=0x{:x} flags=0x{:x} type={} def={}",
            elem.origin,
            elem_run_mode(elem.flags),
            elem.flags,
            elem.elem_type,
            effect.def_name
        );
    }
}

fn log_fx_run_mode_census(
    host: &FxSystemHost,
    sprites: &[FxSpriteInstance],
    catalog: &FxDefinitions,
    cam: [f32; 3],
    radius: f32,
) {
    let mut run0 = 0u32;
    let mut run40 = 0u32;
    let mut run80 = 0u32;
    let mut run_c0 = 0u32;
    let mut stored0 = 0u32;
    for elem in host.live_elems() {
        match elem_run_mode(elem.flags) {
            0 => run0 += 1,
            0x40 => run40 += 1,
            0x80 => run80 += 1,
            0xc0 => run_c0 += 1,
            _ => {}
        }
        if elem.origin[0].abs() < 1.0 && elem.origin[1].abs() < 1.0 && elem.origin[2].abs() < 1.0 {
            stored0 += 1;
        }
    }
    let world0 = sprites
        .iter()
        .filter(|s| s.origin[0].abs() < 1.0 && s.origin[1].abs() < 1.0 && s.origin[2].abs() < 1.0)
        .count();
    diag::info!(
        World,
        "fx_dump run=0:{run0} 0x40:{run40} 0x80:{run80} 0xc0:{run_c0} stored0={stored0} world0={world0}"
    );

    let dust: Vec<&FxSpriteInstance> = sprites
        .iter()
        .filter(|s| s.def_name.contains("dust"))
        .collect();
    if dust.is_empty() {
        return;
    }
    let (mut mn, mut mx) = (dust[0].origin, dust[0].origin);
    for s in &dust {
        for i in 0..3 {
            mn[i] = mn[i].min(s.origin[i]);
            mx[i] = mx[i].max(s.origin[i]);
        }
    }
    let inside = dust
        .iter()
        .filter(|s| sprite_dist(s.origin, cam) <= radius)
        .count();
    let nearest = dust.iter().min_by(|a, b| {
        sprite_dist(a.origin, cam)
            .partial_cmp(&sprite_dist(b.origin, cam))
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    let (offset_r, offset_h, flags) = nearest
        .and_then(|s| {
            let effect = render_fx::present::catalog_lookup(catalog, s.catalog_index)?;
            let elem = effect.elems.get(s.def_index as usize)?;
            Some((
                elem.view.spawn_offset_radius_base,
                elem.view.spawn_offset_height_base,
                elem.view.flags,
            ))
        })
        .unwrap_or((0.0, 0.0, 0));
    let mode = elem_spawn_offset_mode(flags);
    diag::info!(
        World,
        "fx_dump dust n={} inside={inside} spread={:.0}..{:.0},{:.0}..{:.0},{:.0}..{:.0} \
         nearest_run=0x{:x} offset_r={offset_r:.0} offset_h={offset_h:.0} spawn_off={mode:?}",
        dust.len(),
        mn[0],
        mx[0],
        mn[1],
        mx[1],
        mn[2],
        mx[2],
        nearest.map(|s| elem_run_mode(s.flags)).unwrap_or(0),
    );
}

fn log_fx_near_sprite(tag: &str, sprite: &FxSpriteInstance, cam: [f32; 3], cam_fwd: Vec3) {
    let dist = sprite_dist(sprite.origin, cam);
    let delta = Vec3::from_array(sprite.origin) - Vec3::from_array(cam);
    let front = delta.dot(cam_fwd) > 0.0;
    diag::info!(
        World,
        "{tag} dist={dist:.0} front={front} rgb={},{},{} alpha={} size0={:.1} type={} def={} mat={} \
         origin={:.0},{:.0},{:.0} run=0x{:x} flags=0x{:x} atlas={:.3},{:.3},{:.3},{:.3} entry={} idx={}",
        sprite.color_rgba[0],
        sprite.color_rgba[1],
        sprite.color_rgba[2],
        sprite.color_rgba[3],
        sprite.size0,
        sprite.elem_type,
        sprite.def_name,
        sprite.material_name,
        sprite.origin[0],
        sprite.origin[1],
        sprite.origin[2],
        elem_run_mode(sprite.flags),
        sprite.flags,
        sprite.atlas.s0,
        sprite.atlas.ds,
        sprite.atlas.t0,
        sprite.atlas.dt,
        sprite.atlas.entry_count,
        sprite.atlas.atlas_index
    );
}

fn sprite_dist(origin: [f32; 3], cam: [f32; 3]) -> f32 {
    let dx = origin[0] - cam[0];
    let dy = origin[1] - cam[1];
    let dz = origin[2] - cam[2];
    (dx * dx + dy * dy + dz * dz).sqrt()
}

#[derive(Clone, Copy, Debug)]
pub(crate) enum FxPresentSkip {
    NoOrdinal,
}

pub(crate) enum FxCodeMeshBind {
    Ready {
        color: Option<Handle<Image>>,
        sort_key: u8,
        ordinal: u32,
    },
    Skip(FxPresentSkip),
}

fn count_fx_present_skip(cursor: &mut FxJournalCursor, cause: FxPresentSkip) {
    match cause {
        FxPresentSkip::NoOrdinal => {
            cursor.skipped_no_ordinal = cursor.skipped_no_ordinal.saturating_add(1);
        }
    }
}

fn spark_elem_material(
    catalog: &FxDefinitions,
    catalog_index: u16,
    def_index: u8,
) -> Option<String> {
    let effect = render_fx::present::catalog_lookup(catalog, catalog_index)?;
    let elem = effect.elems.get(def_index as usize)?;
    elem.visuals
        .iter()
        .find_map(OwnedFxVisual::present_name)
        .map(str::to_owned)
}

fn spark_elem_asset_id(
    catalog: &FxDefinitions,
    catalog_index: u16,
    def_index: u8,
) -> Option<usize> {
    let effect = render_fx::present::catalog_lookup(catalog, catalog_index)?;
    let elem = effect.elems.get(def_index as usize)?;
    elem.visuals.iter().find_map(OwnedFxVisual::bound_index)
}

fn fill_mark_mesh_plan(
    host: &mut FxSystemHost,
    mesh: Option<fx::GfxMarkMesh>,
    plan: &mut GfxMarkMeshPlan,
    runtime: &MaterialGeneration,
) {
    let Some(mesh) = mesh else {
        host.gfx_mark_draw_n = None;
        host.gfx_mark_draw_tri = None;
        host.gfx_mark_draw_skip_why = None;
        host.last_mark_lmap = None;
        host.last_mark_primary_light = None;
        host.last_mark_probe = None;
        host.last_mark_lmap_none_n = None;
        host.last_mark_lmap_page_n = None;
        plan.bump();
        plan.publish_share();
        return;
    };
    plan.vertices = Arc::new(mesh.packed);
    Arc::make_mut(&mut plan.indices).reserve(mesh.indices.len());
    let mut skip_why = None;

    let mut admitted: Vec<(u64, u8, u32, GfxMarkSubKey, u32, u32)> =
        Vec::with_capacity(mesh.surfs.len());
    for surf in &mesh.surfs {
        if surf.index_count == 0 {
            continue;
        }
        let Some(name) = host.marks.material_name(surf.mark_slot) else {
            skip_why = Some("no_material");
            continue;
        };
        let Some((sort_key, ordinal)) = mark_mesh_bind(name, runtime) else {
            skip_why = Some("no_ordinal");
            continue;
        };
        let sub_key = GfxMarkSubKey::from_context(&surf.context);
        let packed = pack_mark_mesh_draw_surf(
            sort_key,
            render_material::sort_band(ordinal),
            0,
            sub_key.lmap,
            sub_key.primary_light,
            sub_key.probe,
        )
        .packed;
        admitted.push((
            packed,
            sort_key,
            ordinal,
            sub_key,
            surf.index_start,
            surf.index_count,
        ));
    }
    admitted.sort_by_key(|row| row.0);
    for (_, sort_key, ordinal, sub_key, index_start, index_count) in admitted {
        let first = index_start as usize;
        let Some(indices) = mesh
            .indices
            .get(first..first.saturating_add(index_count as usize))
        else {
            skip_why = Some("range_out_of_mesh");
            continue;
        };
        plan.append_run_indices(sort_key, Some(ordinal), sub_key, indices);
    }
    if mesh.budget.surf_n > 0 && plan.draws.is_empty() && skip_why.is_none() {
        skip_why = Some("empty_range");
    }
    plan.skip_why = skip_why;
    plan.bump();
    host.gfx_mark_draw_n = Some(plan.draws.len() as u32);
    host.gfx_mark_draw_tri = Some(
        plan.draws
            .iter()
            .map(|d| d.index_count / 3)
            .fold(0u32, u32::saturating_add),
    );
    host.gfx_mark_draw_skip_why = skip_why.map(str::to_owned);
    if let Some(first) = plan.draws.first() {
        host.last_mark_lmap = Some(first.sub_key.lmap);
        host.last_mark_primary_light = Some(first.sub_key.primary_light);
        host.last_mark_probe = Some(first.sub_key.probe);
    } else {
        host.last_mark_lmap = None;
        host.last_mark_primary_light = None;
        host.last_mark_probe = None;
    }
    let mut none_n = 0u32;
    let mut page_n = 0u32;
    for draw in &plan.draws {
        if draw.sub_key.lmap == marks_iw4::GFX_SURFACE_LIGHTMAP_NONE {
            none_n = none_n.saturating_add(1);
        } else {
            page_n = page_n.saturating_add(1);
        }
    }
    host.last_mark_lmap_none_n = Some(none_n);
    host.last_mark_lmap_page_n = Some(page_n);
    plan.publish_share();
}

fn mark_mesh_bind(name: &str, runtime: &MaterialGeneration) -> Option<(u8, u32)> {
    let bind = asset_game::material_bind_name(name);
    let ordinal = runtime.catalog.ordinal_for_material_name(bind)?;
    let material = runtime_material_for_bind(&runtime.catalog, bind)?;
    let sort_key = material
        .baked_draw_surf
        .map(|packed| dpvs_iw4::GfxDrawSurf { packed }.primary_sort_key())
        .unwrap_or(0);
    Some((sort_key, ordinal.get()))
}

pub(crate) fn code_mesh_bind(
    material_name: &str,
    color_images: &FxWorldColorImages,
    runtime: &MaterialGeneration,
) -> FxCodeMeshBind {
    let bind = asset_game::material_bind_name(material_name);
    let color = lookup_fx_color_image(&color_images.colors, material_name).cloned();
    let Some(ordinal) = runtime.catalog.ordinal_for_material_name(bind) else {
        return FxCodeMeshBind::Skip(FxPresentSkip::NoOrdinal);
    };
    let Some(material) = runtime_material_for_bind(&runtime.catalog, bind) else {
        return FxCodeMeshBind::Skip(FxPresentSkip::NoOrdinal);
    };

    let sort_key = color_images
        .keys
        .get(material_name)
        .or_else(|| color_images.keys.get(bind))
        .map(|(sort_key, _)| *sort_key)
        .or_else(|| {
            material
                .baked_draw_surf
                .map(|packed| dpvs_iw4::GfxDrawSurf { packed }.primary_sort_key())
        })
        .unwrap_or(0);
    FxCodeMeshBind::Ready {
        color,
        sort_key,
        ordinal: ordinal.get(),
    }
}

pub(crate) fn code_mesh_bind_asset(
    asset_id: usize,
    color_images: &FxWorldColorImages,
    runtime: &MaterialGeneration,
) -> FxCodeMeshBind {
    let Some(material) = runtime
        .catalog
        .derived(assets::MaterialIndex::from_order(asset_id))
    else {
        return FxCodeMeshBind::Skip(FxPresentSkip::NoOrdinal);
    };
    let color = color_images.colors_by_asset.get(&asset_id).cloned();
    let Some(ordinal) = runtime
        .catalog
        .parts()
        .sorted_materials
        .ordinal_for_asset_id(asset_id)
    else {
        return FxCodeMeshBind::Skip(FxPresentSkip::NoOrdinal);
    };

    let sort_key = color_images
        .keys_by_asset
        .get(&asset_id)
        .map(|(sort_key, _)| *sort_key)
        .or_else(|| {
            material
                .baked_draw_surf
                .map(|packed| dpvs_iw4::GfxDrawSurf { packed }.primary_sort_key())
        })
        .unwrap_or(0);
    FxCodeMeshBind::Ready {
        color,
        sort_key,
        ordinal: ordinal.get(),
    }
}

fn runtime_material_for_bind<'a>(
    catalog: &'a crate::assemble::drawsurf::RuntimeMaterialCatalog,
    bind: &str,
) -> Option<&'a crate::assemble::drawsurf::RuntimeMaterial> {
    catalog.material_for_name(bind)
}

fn sprite_transform(sprite: &FxSpriteInstance, cam: &Transform) -> Transform {
    let spin = Quat::from_rotation_z(sprite.rotation_rad);
    if sprite.elem_type == 0 {
        Transform {
            translation: Vec3::from_array(sprite.origin),
            rotation: cam.rotation * spin,

            scale: Vec3::new(sprite.size0 * 2.0, sprite.size0 * 2.0, 1.0),
        }
    } else if sprite.elem_type == 2 {
        let origin = tail_anchor_origin(sprite.origin, sprite.vel_dir, sprite.size1);
        let cam_o = cam.translation.to_array();
        let Some(axes) = tail_sprite_axes(sprite.vel_dir, cam_o, origin) else {
            return Transform {
                translation: Vec3::from_array(origin),
                rotation: cam.rotation,
                scale: Vec3::ZERO,
            };
        };

        let right = Vec3::from_array(axes[0]).normalize_or_zero();
        let up = Vec3::from_array(axes[1]).normalize_or_zero();
        let forward = Vec3::from_array(axes[2]).normalize_or_zero();
        let mat = Mat3::from_cols(right, up, forward);
        Transform {
            translation: Vec3::from_array(origin),
            rotation: Quat::from_mat3(&mat) * spin,
            scale: Vec3::new(
                tail_sprite_full_extent(sprite.size0),
                tail_sprite_full_extent(sprite.size1),
                1.0,
            ),
        }
    } else {
        let right = Vec3::from_array(sprite.axis[1]).normalize_or_zero();
        let up = Vec3::from_array(sprite.axis[2]).normalize_or_zero();
        let forward = right.cross(up).normalize_or_zero();
        let mat = Mat3::from_cols(right, up, forward);
        Transform {
            translation: Vec3::from_array(sprite.origin),
            rotation: Quat::from_mat3(&mat) * spin,

            scale: Vec3::new(sprite.size0 * 2.0, sprite.size0 * 2.0, 1.0),
        }
    }
}

#[expect(
    clippy::too_many_arguments,
    reason = "Bevy system parameters keep scheduler resource access explicit."
)]
fn publish_weapon_fire(
    fire: On<net::EntityWeaponFire>,
    mut pending: MessageWriter<WeaponFireFx>,
    identities: Query<&CEntity>,
    local: Res<LocalPresentClient>,
    presented: Res<PresentedSnapshot>,
    (view, settings): (Res<ViewSubject>, Res<frame::GameSettings>),
    weapons: Option<Res<PreparedWeapons>>,
    sound_output: WeaponSoundOutput,
    prediction: Res<ClientPredictionState>,
    mut flames: Option<ResMut<Messages<audio::FlameFire>>>,
    journal: FxCombatJournal,
) {
    let FxCombatJournal {
        mut cursor,
        mut combat,
    } = journal;
    let WeaponSoundOutput {
        sound_bank,
        mut sounds,
    } = sound_output;

    let weapons = weapons.as_deref().map(|weapons| weapons.bound());
    pending.write(WeaponFireFx(*fire));
    let eyes = match *view {
        ViewSubject::Seat {
            focus: Some(focus), ..
        } => i32::try_from(focus).unwrap_or(0),
        _ => i32::try_from(local.0.0).unwrap_or(0),
    };
    let player = if view.in_killcam() {
        presented.player(local.0)
    } else {
        prediction
            .0
            .predicted_local()
            .or_else(|| presented.player(local.0))
    };
    let gate = PlayerDrawGate {
        eyes_entity_num: eyes,
        other_flags: player.map_or(0, |ps| ps.other_flags),
        rendering_third_person: crate::adapters::anim::third_person::presented_is_third_person(
            &presented,
            local.0,
            view.in_killcam(),
            settings.third_person,
        ),
    };
    let player_view = identities
        .get(fire.entity)
        .ok()
        .is_some_and(|identity| gate.skip_self_fpv(identity.number()));
    if let (Some(flames), Some(weapons), Some(bank)) = (
        flames.as_deref_mut(),
        weapons.as_ref(),
        sound_bank.as_deref(),
    ) && weapons
        .registry()
        .weapon_sound_alias(
            fire.event.payload.weapon,
            asset_game::WeaponSoundSlot::FireStart,
            &bank.0,
        )
        .is_some()
    {
        flames.write(audio::FlameFire {
            entity: fire.entity,
            weapon: fire.event.payload.weapon,
            player_view,
            origin_inches: fire.event.payload.origin,
        });
    }
    let last_shot = is_weapon_fire_last_shot_event(fire.event.event);
    let alias = identities.get(fire.entity).ok().and_then(|_| {
        let weapons = weapons.as_ref()?;
        let bank = sound_bank.as_deref()?;
        let weapon = fire.event.payload.weapon;
        audio::select_cg_fire_alias(
            last_shot,
            player_view,
            weapons.registry().weapon_sound_alias(
                weapon,
                asset_game::WeaponSoundSlot::Fire,
                &bank.0,
            ),
            weapons.registry().weapon_sound_alias(
                weapon,
                asset_game::WeaponSoundSlot::FirePlayer,
                &bank.0,
            ),
            weapons.registry().weapon_sound_alias(
                weapon,
                asset_game::WeaponSoundSlot::FireLast,
                &bank.0,
            ),
            weapons.registry().weapon_sound_alias(
                weapon,
                asset_game::WeaponSoundSlot::FireLastPlayer,
                &bank.0,
            ),
        )
        .map(|alias| (alias, player_view))
    });
    if let (Some((alias, player_view)), Some(sounds)) = (alias, sounds.as_deref_mut()) {
        if audio::audio_diagnostics_enabled() {
            audio::emit_audio_diagnostic(format!(
                "audio diag: fire producer weapon={} alias={alias} player_view={player_view} event={:?}",
                fire.event.payload.weapon,
                audio::AudioEvent::from_entity(fire.entity, &fire.event, 0).id
            ));
        }
        perf::instant!("weapon_fire_audio", "iw4l.sim", |ctx| {
            ctx.arg("entity", fire.entity.to_bits());
            ctx.arg("sequence", i64::from(fire.event.sequence.0));
            ctx.arg("tick", i64::from(fire.event.tick.0));
            ctx.arg("alias", alias);
            ctx.arg("player_view", player_view);
            ctx.arg(
                "domain",
                match fire.event.domain {
                    net::EntityEventDomain::Predicted => "predicted",
                    net::EntityEventDomain::Snapshot => "snapshot",
                    net::EntityEventDomain::Ring => "ring",
                },
            );
        });
        combat.last_fire_alias = Some(alias.to_owned());

        let sound_origin = fire.event.payload.origin;
        let Some(sound_namespace) = weapons.as_ref().and_then(|w| {
            w.registry().component_namespace_of(
                fire.event.payload.weapon,
                asset_game::WeaponComponent::Sound,
            )
        }) else {
            return;
        };
        sounds.write(audio::WeaponSound {
            event: Some(audio::AudioEvent::from_entity(fire.entity, &fire.event, 0)),
            namespace: sound_namespace,
            alias: alias.to_owned(),
            origin_inches: (!player_view).then_some(sound_origin),
            snd_ent: audio::ent_from_number(fire.event.payload.number),
        });
    } else {
        combat.last_fire_alias = None;
        cursor.fire_sound_gap = cursor.fire_sound_gap.saturating_add(1);
        if audio::audio_diagnostics_enabled() {
            audio::emit_audio_diagnostic(format!(
                "audio diag: fire producer gap weapon={} player_view={player_view} event={:?}",
                fire.event.payload.weapon,
                audio::AudioEvent::from_entity(fire.entity, &fire.event, 0).id
            ));
        }
    }
}

#[expect(
    clippy::too_many_arguments,
    reason = "Bevy system parameters keep scheduler resource access explicit."
)]
fn drain_weapon_fire_fx(
    mut pending: MessageReader<WeaponFireFx>,
    identities: Query<&CEntity>,
    world_bolts: Query<(
        &net::CEntityRuntime,
        &crate::adapters::anim::remote_body::RemoteFxBolts,
    )>,
    fpv_bolts: Res<crate::adapters::anim::fpv_present::FpvBoltTargets>,
    (local, mut occurrences): (Res<LocalPresentClient>, ResMut<PresentedFireFx>),
    presented: Res<PresentedSnapshot>,
    view_settings: (Res<ViewSubject>, Res<frame::GameSettings>),
    weapons: Option<Res<PreparedWeapons>>,
    runtime: FxRuntimeAccess,
    journal: FxCombatJournal,
    (mut ping_bus, mut flames): (
        ResMut<WeaponFirePingBus>,
        ResMut<super::flame::FlameStreams>,
    ),
    fx_world: FxSceneAccess,
) {
    let FxCombatJournal {
        mut cursor,
        mut combat,
    } = journal;
    let FxRuntimeAccess {
        catalog,
        mut elem_infos,
        mut host,
    } = runtime;

    let (view, settings) = view_settings;
    for fire in pending.read().map(|record| record.0) {
        let weapons = weapons.as_deref().map(|weapons| weapons.bound());
        let msec = host.0.msec_now;
        let combat_fx = weapons
            .as_ref()
            .and_then(|weapons| weapons.registry().combat_fx_of(fire.event.payload.weapon));

        let eyes = match *view {
            ViewSubject::Seat {
                focus: Some(focus), ..
            } => i32::try_from(focus).unwrap_or(0),
            _ => i32::try_from(local.0.0).unwrap_or(0),
        };
        let gate = PlayerDrawGate {
            eyes_entity_num: eyes,
            other_flags: presented
                .player(local.0)
                .map(|ps| ps.other_flags)
                .unwrap_or(0),
            rendering_third_person: crate::adapters::anim::third_person::presented_is_third_person(
                &presented,
                local.0,
                view.in_killcam(),
                settings.third_person,
            ),
        };
        let player_view = identities
            .get(fire.entity)
            .ok()
            .is_some_and(|identity| gate.skip_self_fpv(identity.number()));
        combat.last_fire_player_view = Some(i64::from(player_view));
        if let Some(stream) = weapons.as_ref().and_then(|weapons| {
            weapons
                .registry()
                .flame_stream_of(fire.event.payload.weapon)
        }) {
            flames.note_fire(
                fire.event.payload.number,
                fire.entity,
                player_view,
                stream.view(player_view),
                msec,
            );
        }
        let last_shot = is_weapon_fire_last_shot_event(fire.event.event);
        combat.last_fire_lastshot = Some(i64::from(last_shot));
        combat.last_tracer_name = combat_fx.and_then(|fx| fx.tracer_hint.clone());
        combat.last_weapon_tracer_edge = combat_fx.map(|fx| fx.tracer.edge_kind().to_owned());
        combat.last_weapon_flash_edge =
            combat_fx.map(|fx| fx.flash_edge(player_view).edge_kind().to_owned());
        let muzzle_name = combat_fx.and_then(|fx| fx.flash_present(player_view));
        combat.last_muzzle_name = muzzle_name.map(|n| n.name.to_owned());

        let hand = usize::from(is_left_hand_fire_event(fire.event.event));
        let remote_bolts = world_bolts
            .get(fire.entity)
            .ok()
            .filter(|(runtime, _)| runtime.in_next_snap())
            .map(|(_, bolts)| bolts);
        let flash_target = if player_view {
            fpv_bolts.flash[hand]
        } else {
            remote_bolts.and_then(|bolts| bolts.flash)
        };
        let brass_target = if player_view {
            fpv_bolts.brass[hand]
        } else {
            remote_bolts.and_then(|bolts| bolts.brass)
        };
        if let Some(catalog) = catalog.as_deref() {
            elem_infos.0.sync(&catalog.0);
            let mut muzzle_played = cursor.muzzle_played;
            let mut muzzle_gap = cursor.muzzle_gap;

            occurrences.execute(
                FireFxRequest::event(&fire.event, FireFxOccurrence::Muzzle, msec),
                || {
                    let outcome = present_weapon_fx_bolted(
                        &mut host.0,
                        &catalog.0,
                        &mut elem_infos.0,
                        muzzle_name,
                        flash_target,
                        &mut muzzle_played,
                        fx_world.view().as_ref().map(|s| s as &dyn FxScene),
                    );
                    if matches!(outcome, FireFxOutcome::Created(_)) {
                        cursor.muzzle_bolted = cursor.muzzle_bolted.saturating_add(1);
                    } else {
                        muzzle_gap = muzzle_gap.saturating_add(1);
                    }
                    outcome
                },
            );
            if muzzle_played > cursor.muzzle_played {
                combat.muzzle_msec = Some(msec);
            }
            cursor.muzzle_played = muzzle_played;
            cursor.muzzle_gap = muzzle_gap;
            let delayed_brass = weapons
                .as_ref()
                .and_then(|weapons| {
                    weapons
                        .row(fire.event.payload.weapon)
                        .and_then(|weapon| weapon.event_facts())
                })
                .is_some_and(|facts| facts.bolt_action);
            if !delayed_brass {
                occurrences.execute(
                    FireFxRequest::event(&fire.event, FireFxOccurrence::Brass, msec),
                    || {
                        ShellEjectRequest {
                            combat_fx,
                            player_view,
                            last_shot,
                            target: brass_target,
                        }
                        .present(
                            &mut host.0,
                            &catalog.0,
                            &mut elem_infos.0,
                            &mut cursor,
                            &mut combat,
                            fx_world.view().as_ref().map(|s| s as &dyn FxScene),
                        )
                    },
                );
            }
        } else {
            for product in [FireFxOccurrence::Muzzle, FireFxOccurrence::Brass] {
                occurrences.execute(FireFxRequest::event(&fire.event, product, msec), || {
                    FireFxOutcome::DependencyRefused("fx_catalog")
                });
            }
            cursor.muzzle_gap = cursor.muzzle_gap.saturating_add(1);
            cursor.brass_gap = cursor.brass_gap.saturating_add(1);
        }
        if let Some(facts) = weapons.as_ref().and_then(|weapons| {
            weapons
                .row(fire.event.payload.weapon)
                .and_then(|weapon| weapon.event_facts())
        }) && fire_weapon_fx_should_client_trace(facts.impact_type)
        {
            combat.last_impact_miss_why = Some("authority_segments".into());
        }

        let local_number = i32::try_from(local.0.0).unwrap_or(-1);
        let hide_fire_ping = weapons.as_ref().is_some_and(|weapons| {
            weapons
                .row(fire.event.payload.weapon)
                .and_then(|weapon| weapon.event_facts())
                .is_some_and(|facts| facts.hides_fire_ping())
        });
        if fire.event.payload.number != local_number && !player_view && !hide_fire_ping {
            ping_bus.pings.push(WeaponFirePing {
                number: fire.event.payload.number,
                origin_xy: [fire.event.payload.origin[0], fire.event.payload.origin[1]],
            });
        }
        log_combat_fx_gaps(&mut cursor, &combat);
        sync_combat_dump(&cursor, &mut combat);
    }
}

#[expect(
    clippy::too_many_arguments,
    reason = "Bevy system parameters keep scheduler resource access explicit."
)]
fn eject_brass(
    brass: On<net::EntityEjectBrass>,
    identities: Query<&CEntity>,
    world_bolts: Query<(
        &net::CEntityRuntime,
        &crate::adapters::anim::remote_body::RemoteFxBolts,
    )>,
    fpv_bolts: Res<crate::adapters::anim::fpv_present::FpvBoltTargets>,
    local_view: (
        Res<LocalPresentClient>,
        Res<PresentedSnapshot>,
        Res<ViewSubject>,
        Res<frame::GameSettings>,
    ),
    mut occurrences: ResMut<PresentedFireFx>,
    weapons: Option<Res<PreparedWeapons>>,
    runtime: FxRuntimeAccess,
    journal: FxCombatJournal,
    fx_world: FxSceneAccess,
) {
    let FxCombatJournal {
        mut cursor,
        mut combat,
    } = journal;

    let FxRuntimeAccess {
        catalog,
        mut elem_infos,
        mut host,
    } = runtime;

    let weapons = weapons.as_deref().map(|weapons| weapons.bound());
    let (local, presented, view, settings) = local_view;
    let msec = host.0.msec_now;
    let third_person = crate::adapters::anim::third_person::presented_is_third_person(
        &presented,
        local.0,
        view.in_killcam(),
        settings.third_person,
    );
    let combat_fx = weapons
        .as_ref()
        .and_then(|weapons| weapons.registry().combat_fx_of(brass.event.payload.weapon));
    let player_view = identities
        .get(brass.entity)
        .ok()
        .is_some_and(|identity| identity.client() == Some(local.0) && !third_person);
    let last_shot = is_weapon_fire_last_shot_event(brass.event.event);
    let hand = usize::from(is_left_hand_fire_event(brass.event.event));
    let target = if player_view {
        fpv_bolts.brass[hand]
    } else {
        world_bolts
            .get(brass.entity)
            .ok()
            .filter(|(runtime, _)| runtime.in_next_snap())
            .and_then(|(_, bolts)| bolts.brass)
    };
    occurrences.execute(
        FireFxRequest::event(&brass.event, FireFxOccurrence::Brass, msec),
        || {
            if let Some(catalog) = catalog.as_deref() {
                elem_infos.0.sync(&catalog.0);
                ShellEjectRequest {
                    combat_fx,
                    player_view,
                    last_shot,
                    target,
                }
                .present(
                    &mut host.0,
                    &catalog.0,
                    &mut elem_infos.0,
                    &mut cursor,
                    &mut combat,
                    fx_world.view().as_ref().map(|s| s as &dyn FxScene),
                )
            } else {
                cursor.brass_gap = cursor.brass_gap.saturating_add(1);
                FireFxOutcome::DependencyRefused("fx_catalog")
            }
        },
    );
    log_combat_fx_gaps(&mut cursor, &combat);
    sync_combat_dump(&cursor, &mut combat);
}

#[expect(
    clippy::too_many_arguments,
    reason = "Bevy system parameters keep scheduler resource access explicit."
)]
fn tick_missile_present_state(
    occupancy: Option<Res<crate::adapters::anim::missile::MissileOccupancy>>,
    weapons: Option<Res<assets::PreparedWeapons>>,
    projectile_meshes: Option<Res<assets::PreparedProjectileMeshes>>,
    runtime: FxRuntimeAccess,
    mut commands: Commands,
    slots: Res<CEntitySlots>,
    mut effects: Query<Option<&mut crate::adapters::anim::missile::MissileEffects>, With<CEntity>>,
    poses: Option<Res<crate::adapters::anim::dobj_pose::HostDObjPoseFrame>>,
    sound_output: WeaponSoundOutput,
    fx_world: FxSceneAccess,
) {
    let WeaponSoundOutput {
        sound_bank,
        mut sounds,
    } = sound_output;

    let FxRuntimeAccess {
        catalog,
        mut elem_infos,
        mut host,
    } = runtime;

    let Some(occupancy) = occupancy.as_deref() else {
        return;
    };
    let Some(weapons) = weapons
        .as_deref()
        .map(|prepared| prepared.registry())
        .filter(|registry| !registry.is_empty())
    else {
        return;
    };
    let Some(meshes) = projectile_meshes
        .as_deref()
        .map(|prepared| &prepared.0)
        .filter(|catalog| !catalog.is_empty())
    else {
        return;
    };
    let Some(catalog) = catalog.as_deref().map(|catalog| &catalog.0) else {
        return;
    };
    elem_infos.0.sync(catalog);
    for row in &occupancy.rows {
        let Some(entnum) = row.entnum else {
            continue;
        };
        let Some(entity) = u16::try_from(entnum)
            .ok()
            .and_then(|number| slots.entity_for_number(number))
        else {
            continue;
        };
        let Ok(mut owned) = effects.get_mut(entity) else {
            continue;
        };
        let mut fresh = crate::adapters::anim::missile::MissileEffects::default();
        let state = owned.as_deref_mut().unwrap_or(&mut fresh);
        let want_trail = row.ignited && !state.trail_played;
        let want_beacon = !state.beacon_played;
        let want_ignition = row.ignited && !state.ignition_played;

        let mut play = |name: Option<asset_game::FxName<'_>>| {
            let Some(name) = name else {
                return false;
            };
            let Some(target) =
                missile_bolt_target(poses.as_deref(), meshes, row.namespace, &row.name, entnum)
            else {
                return false;
            };
            try_play_weapon_fx_bolted(
                &mut host.0,
                catalog,
                &mut elem_infos.0,
                Some(name),
                Some(target),
                &mut 0,
                fx_world.view().as_ref().map(|s| s as &dyn FxScene),
            )
        };
        if want_trail {
            state.trail_played = play(weapons.proj_trail_of(row.weapon));
        }
        if want_beacon {
            state.beacon_played = play(weapons.proj_beacon_of(row.weapon));
        }
        if row.ignited && !state.ignition_fx_played {
            state.ignition_fx_played = play(weapons.proj_ignition_of(row.weapon));
        }

        if want_ignition
            && weapons
                .authored_weapon_sound(row.weapon, asset_game::WeaponSoundSlot::ProjIgnition)
                .is_some()
            && let Some(alias) = sound_bank.as_deref().and_then(|bank| {
                weapons.weapon_sound_alias(
                    row.weapon,
                    asset_game::WeaponSoundSlot::ProjIgnition,
                    &bank.0,
                )
            })
            && let Some(sound_namespace) =
                weapons.component_namespace_of(row.weapon, asset_game::WeaponComponent::Sound)
            && let Some(sounds) = sounds.as_deref_mut()
        {
            sounds.write(audio::WeaponSound {
                event: None,
                namespace: sound_namespace,
                alias: alias.to_owned(),
                origin_inches: Some(row.origin),
                snd_ent: Some(entnum),
            });
            state.ignition_played = true;
        }

        if owned.is_none() {
            commands.entity(entity).insert(fresh);
        }
    }
}

fn explosion(
    explosion: On<net::EntityExplosion>,
    weapons: Option<Res<PreparedWeapons>>,
    sound_output: WeaponSoundOutput,
    runtime: FxRuntimeAccess,
    impact_fx: Option<Res<PreparedImpactFx>>,
    journal: FxCombatJournal,
    fx_world: FxSceneAccess,
) {
    let FxCombatJournal {
        mut cursor,
        mut combat,
    } = journal;
    let WeaponSoundOutput {
        sound_bank,
        mut sounds,
    } = sound_output;

    let FxRuntimeAccess {
        catalog,
        mut elem_infos,
        mut host,
    } = runtime;

    let weapons = weapons.as_deref().map(|weapons| weapons.bound());
    let msec = host.0.msec_now;
    let payload = explosion.event.payload;
    let impact_type = weapons
        .as_ref()
        .and_then(|weapons| {
            weapons
                .row(payload.weapon)
                .and_then(|weapon| weapon.event_facts())
        })
        .map(|facts| facts.impact_type);
    let combat_fx = weapons
        .as_ref()
        .and_then(|weapons| weapons.registry().combat_fx_of(payload.weapon));
    combat.last_weapon_explosion_edge = combat_fx.map(|fx| fx.explosion.edge_kind().to_owned());
    let slot = combat_fx.and_then(|fx| fx.explosion_present());
    let weapon_namespace = weapons
        .as_ref()
        .and_then(|weapons| weapons.registry().identity_namespace_of(payload.weapon));
    let names = explosion_fx_names(
        impact_type,
        payload.surf_type,
        impact_fx
            .as_ref()
            .and_then(|fx| fx.explosion_table(weapon_namespace)),
        slot,
    );
    combat.last_explosion_table = names.table.map(|n| n.name.to_owned());
    combat.last_explosion_slot = names.slot.map(|n| n.name.to_owned());
    combat.last_explosion_name = names.table.or(names.slot).map(|n| n.name.to_owned());
    if let Some(row) = names.row {
        combat.last_row = Some(row as i64);
        combat.last_surf = Some(i64::from(payload.surf_type));
        combat.last_impact_def = names.table.map(|n| n.name.to_owned());
    }
    let axis = if payload.direction == [0.0, 0.0, 0.0] {
        IDENTITY_AXIS
    } else {
        axis_from_hit_normal(payload.direction)
    };
    if let Some(catalog) = catalog.as_deref() {
        elem_infos.0.sync(&catalog.0);
        let mut boom_played = cursor.boom_played;
        let orientation = fx_iw4::FxOrientation {
            origin: payload.origin,
            axis,
        };
        let table_ok = try_play_weapon_fx_at_origin(
            &mut host.0,
            &catalog.0,
            &mut elem_infos.0,
            names.table,
            orientation,
            &mut boom_played,
            fx_world.view().as_ref().map(|s| s as &dyn FxScene),
        );
        let slot_ok = try_play_weapon_fx_at_origin(
            &mut host.0,
            &catalog.0,
            &mut elem_infos.0,
            names.slot,
            orientation,
            &mut boom_played,
            fx_world.view().as_ref().map(|s| s as &dyn FxScene),
        );
        cursor.boom_played = boom_played;
        if table_ok || slot_ok {
            combat.explosion_msec = Some(msec);
        }
        if !table_ok && !slot_ok {
            cursor.explosion_gap = cursor.explosion_gap.saturating_add(1);
        }
    } else {
        cursor.explosion_gap = cursor.explosion_gap.saturating_add(1);
    }
    let mut sound_submitted = false;
    if explosion.event.event != entity_iw4::EntityEventKind::FLASHBANG_EXPLODE {
        let surface_sound =
            weapons
                .as_ref()
                .zip(sound_bank.as_deref())
                .and_then(|(weapons, bank)| {
                    let namespace = weapons.registry().component_namespace_of(
                        payload.weapon,
                        asset_game::WeaponComponent::Sound,
                    )?;
                    let resolve = asset_audio::audio_family(namespace)?.explosion_surfaces?;
                    let (alias, fallback) = resolve(impact_type?, payload.surf_type)?;
                    let alias = [alias, fallback]
                        .into_iter()
                        .find(|alias| bank.0.sound_in(namespace, alias).is_some())?;
                    Some((namespace, alias))
                });
        if let (Some((namespace, alias)), Some(sounds)) = (surface_sound, sounds.as_deref_mut()) {
            sound_submitted = true;
            sounds.write(audio::WeaponSound {
                event: Some(audio::AudioEvent::from_entity(
                    explosion.entity,
                    &explosion.event,
                    1,
                )),
                namespace,
                alias,
                origin_inches: Some(payload.origin),
                snd_ent: audio::ent_from_number(payload.number),
            });
        }
    }
    let alias = weapons
        .as_ref()
        .zip(sound_bank.as_deref())
        .and_then(|(weapons, bank)| {
            weapons.registry().weapon_sound_alias(
                payload.weapon,
                asset_game::WeaponSoundSlot::ProjectileExplosion,
                &bank.0,
            )
        });
    if let (Some(alias), Some(sounds)) = (alias, sounds.as_deref_mut()) {
        let Some(sound_namespace) = weapons.as_ref().and_then(|w| {
            w.registry()
                .component_namespace_of(payload.weapon, asset_game::WeaponComponent::Sound)
        }) else {
            return;
        };
        sounds.write(audio::WeaponSound {
            event: Some(audio::AudioEvent::from_entity(
                explosion.entity,
                &explosion.event,
                0,
            )),
            namespace: sound_namespace,
            alias: alias.to_owned(),
            origin_inches: Some(payload.origin),
            snd_ent: audio::ent_from_number(payload.number),
        });
    } else if !sound_submitted {
        cursor.explosion_sound_gap = cursor.explosion_sound_gap.saturating_add(1);
    }
    log_combat_fx_gaps(&mut cursor, &combat);
    sync_combat_dump(&cursor, &mut combat);
}

fn reset_timeline_fx(mut host: ResMut<HostFxSystem>, scene: Option<Res<WorldScene>>) {
    if let Some(glass) = scene.as_ref().and_then(|scene| scene.fx_glass.as_ref()) {
        host.0.glass.reset(fx::FxGlassInitTables {
            piece_limit: glass.piece_limit as u32,
            geo_data_limit: glass.geo_data_limit as u32,
            init_states: &glass.init_piece_states,
            init_geo: &glass.init_geo_data,
            defs: &glass.defs,
        });
        host.0.glass.moved = true;
    }
}

#[derive(Resource, Default)]
struct ScriptFxRows(HashMap<u32, ScriptFxRow>);

#[derive(Default)]
struct ScriptFxRow {
    effect: u8,
    start: Option<i32>,
    held: Option<u16>,
    next_ms: i32,
}

fn axis(forward: [f32; 3], up: [f32; 3]) -> [[f32; 3]; 3] {
    let forward = Vec3::from_array(forward);
    let up = Vec3::from_array(up);
    [
        forward.to_array(),
        up.cross(forward).to_array(),
        up.to_array(),
    ]
}

fn sync_script_fx(
    adopted: Option<Res<LastAdoptedSnapshot>>,
    presented: Res<PresentedSnapshot>,
    runtime: FxRuntimeAccess,
    camera: Option<Res<FxCameraOrigin>>,
    fx_world: FxSceneAccess,
    local: Res<LocalPresentClient>,
    mut rows: ResMut<ScriptFxRows>,
) {
    let FxRuntimeAccess {
        catalog,
        mut elem_infos,
        mut host,
    } = runtime;

    let (Some(adopted), Some(catalog)) = (adopted, catalog) else {
        return;
    };
    let Some(snap) = presented.snapshot() else {
        return;
    };
    let rows = &mut rows.0;
    let effects: Vec<&sim::ScriptEffect> = snap
        .meta
        .objectives
        .effects
        .iter()
        .filter(|fx| fx.plays_for(local.0.0))
        .collect();
    rows.retain(|id, row| {
        let keep = effects
            .iter()
            .any(|fx| fx.id == *id && fx.effect == row.effect);
        if !keep && let Some(handle) = row.held {
            host.0.stop_owned(handle);
        }
        keep
    });
    if effects.is_empty() {
        return;
    }
    let server_now = sim::level_time_ms(snap.view_tick(local.0));
    let offset = host.0.msec_now.wrapping_sub(server_now);
    let eye = camera.map(|c| Vec3::from_array(c.0));
    elem_infos.0.sync(&catalog.0);
    let scene = fx_world.view();
    let scene = scene.as_ref().map(|s| s as &dyn FxScene);
    for fx in effects {
        let row = rows.entry(fx.id).or_insert_with(|| ScriptFxRow {
            effect: fx.effect,
            ..Default::default()
        });
        let Some(name) = adopted.effect_name(fx.effect) else {
            continue;
        };
        let name = catalog.0.map_fx_name(name);
        let axis = axis(fx.forward, fx.up);
        let retriggered = row.start != fx.start_ms;
        row.start = fx.start_ms;
        let Some(start) = fx.start_ms else {
            continue;
        };
        if fx.repeat_ms <= 0 {
            if retriggered {
                if let Some(handle) = row.held.take() {
                    host.0.stop_owned(handle);
                }
                row.held = spawn_named_oriented_in_world(
                    &mut host.0,
                    &catalog.0,
                    &elem_infos.0,
                    name,
                    fx::FxPlayPose {
                        origin: fx.origin,
                        axis,
                        msec: start.wrapping_add(offset),
                    },
                    scene,
                )
                .and_then(|result| match result {
                    PlayResult::Held { handle } => Some(handle),
                    _ => None,
                });
            }
            continue;
        }
        if retriggered {
            row.next_ms = start;
        }
        let behind = server_now.saturating_sub(row.next_ms);
        if behind > fx.repeat_ms.saturating_mul(4) {
            row.next_ms = server_now - behind % fx.repeat_ms;
        }
        while row.next_ms <= server_now {
            let culled = fx.cull_distance > 0.0
                && eye.is_some_and(|eye| {
                    eye.distance(Vec3::from_array(fx.origin)) > fx.cull_distance
                });
            if !culled {
                play_named_oriented_at_msec(
                    &mut host.0,
                    &catalog.0,
                    &elem_infos.0,
                    name,
                    fx::FxPlayPose {
                        origin: fx.origin,
                        axis,
                        msec: row.next_ms.wrapping_add(offset),
                    },
                    scene,
                );
            }
            row.next_ms = row.next_ms.saturating_add(fx.repeat_ms);
        }
    }
}

#[derive(Resource, Default)]
struct PendingTagFx(Vec<TagFxRequest>);

struct TagFxRequest {
    stop: bool,
    def_name: String,
    entnum: u16,
    bone: u16,
    origin: [f32; 3],
    direction: [f32; 3],
    waited: u8,
}

const TAG_FX_POSE_WAIT: u8 = 8;

fn drain_tag_fx(
    mut pending: ResMut<PendingTagFx>,
    runtime: FxRuntimeAccess,
    poses: Option<Res<crate::adapters::anim::dobj_pose::HostDObjPoseFrame>>,
    mut demand: ResMut<render_anim::ScriptModelBoltDemand>,
    fx_world: FxSceneAccess,
) {
    let FxRuntimeAccess {
        catalog,
        mut elem_infos,
        mut host,
    } = runtime;

    let Some(catalog) = catalog else {
        return;
    };
    if pending.0.is_empty() {
        return;
    }
    elem_infos.0.sync(&catalog.0);
    let scene = fx_world.view();
    let scene = scene.as_ref().map(|s| s as &dyn FxScene);
    let mut waiting = Vec::new();
    for mut request in std::mem::take(&mut pending.0) {
        let name = catalog.0.map_fx_name(&request.def_name);
        if request.stop {
            waiting.retain(|queued: &TagFxRequest| {
                queued.def_name != request.def_name
                    || queued.entnum != request.entnum
                    || queued.bone != request.bone
            });
            if let Some(effect) = name.resolve(&catalog.0) {
                host.0.stop_bolted(
                    effect.name.as_str(),
                    u32::from(request.entnum),
                    request.bone,
                );
            }
            continue;
        }
        let entnum = u32::from(request.entnum);
        let resolved = poses
            .as_deref()
            .and_then(|poses| poses.resolve_live_bolt(entnum, request.bone))
            .and_then(|resolved| {
                Some(fx::FxBoltTarget {
                    dobj: entnum,
                    bone: request.bone,
                    centity_teleport: resolved.centity_teleport,
                    orientation: resolved.orientation?,
                })
            });
        match resolved {
            Some(target) => {
                render_fx::present::play_named_bolted_in_world(
                    &mut host.0,
                    &catalog.0,
                    &elem_infos.0,
                    name,
                    target,
                    scene,
                );
            }
            None if request.waited < TAG_FX_POSE_WAIT => {
                demand.insert(request.entnum);
                request.waited += 1;
                waiting.push(request);
            }
            None => {
                play_named_oriented_in_world(
                    &mut host.0,
                    &catalog.0,
                    &elem_infos.0,
                    name,
                    request.origin,
                    axis_from_hit_normal(request.direction),
                    scene,
                );
            }
        }
    }
    pending.0 = waiting;
}

fn play_fx(
    play: On<net::EntityPlayFx>,
    runtime: FxRuntimeAccess,
    adopted: Option<Res<LastAdoptedSnapshot>>,
    mut presented: ResMut<PresentedVehicleFx>,
    mut tag_fx: ResMut<PendingTagFx>,
    mut demand: ResMut<render_anim::ScriptModelBoltDemand>,
    fx_world: FxSceneAccess,
) {
    let FxRuntimeAccess {
        catalog,
        mut elem_infos,
        mut host,
    } = runtime;

    let Some(catalog) = catalog else {
        return;
    };
    let payload = play.event.payload;
    if play.event.event != entity_iw4::EntityEventKind::PLAY_FX {
        let (Ok(entnum), Ok(bone)) = (
            u16::try_from(payload.number),
            u16::try_from(payload.event_parm >> 8),
        ) else {
            return;
        };
        let index = (payload.event_parm & 0xff) as u8;
        let Some(def_name) = adopted
            .as_ref()
            .and_then(|snap| snap.effect_name(index).map(str::to_owned))
        else {
            return;
        };
        demand.insert(entnum);
        tag_fx.0.push(TagFxRequest {
            stop: play.event.event == entity_iw4::EntityEventKind::STOP_FX_ON_TAG,
            def_name,
            entnum,
            bone,
            origin: payload.origin,
            direction: if payload.direction == [0.0; 3] {
                [0.0, 0.0, 1.0]
            } else {
                payload.direction
            },
            waited: 0,
        });
        return;
    }
    let index = u8::try_from(payload.event_parm).unwrap_or(0);
    let owner = (payload.correlation != 0).then_some(payload.correlation);
    let Some(def_name) = adopted
        .as_ref()
        .and_then(|snap| snap.effect_name(index).map(str::to_owned))
    else {
        if let Some(owner) = owner {
            presented.by_id.insert(
                owner,
                PresentedVehicleFxRow {
                    fx_spawn: Some("miss_cs"),
                    fx_spawn_def: None,
                },
            );
        }
        return;
    };
    let normal = if payload.direction == [0.0, 0.0, 0.0] {
        [0.0, 0.0, 1.0]
    } else {
        payload.direction
    };
    elem_infos.0.sync(&catalog.0);
    let spawn = match play_named_oriented_in_world(
        &mut host.0,
        &catalog.0,
        &elem_infos.0,
        catalog.0.map_fx_name(&def_name),
        payload.origin,
        axis_from_hit_normal(normal),
        fx_world.view().as_ref().map(|s| s as &dyn FxScene),
    ) {
        Some(PlayResult::PlayedReleased { .. } | PlayResult::Held { .. }) => "spawned",
        Some(PlayResult::Failed(_)) | None => "miss_def",
    };
    if let Some(owner) = owner {
        presented.by_id.insert(
            owner,
            PresentedVehicleFxRow {
                fx_spawn: Some(spawn),
                fx_spawn_def: Some(def_name),
            },
        );
    }
}

fn play_fx_bullet_hit(hit: On<net::EntityBulletHit>, mut hits: MessageWriter<BulletHitFx>) {
    hits.write(BulletHitFx(hit.event));
}

fn maintain_fire_fx(
    host: Res<HostFxSystem>,
    mut occurrences: ResMut<PresentedFireFx>,
    mut gate: ResMut<TracerDrawGate>,
) {
    occurrences.maintain(host.0.msec_now);
    gate.maintain(host.0.msec_now);
}

fn drain_bullet_hit_fx(
    mut hits: MessageReader<BulletHitFx>,
    mut presentation: PelletFxPresentation,
    fx_world: FxSceneAccess,
) {
    for hit in hits.read() {
        let payload = hit.0.payload;
        let now = presentation.now();
        if payload.attacker_entity_num == presentation.local_number()
            && let Some(cause) = payload.fire_cause
        {
            perf::instant!("owner_impact", "iw4l.sim", |ctx| {
                let (source, client, command, pellet, segment): (&str, u32, u32, u16, u16) = (
                    match hit.0.domain {
                        net::EntityEventDomain::Predicted => "predicted",
                        _ => "authority",
                    },
                    cause.client.0,
                    cause.command.0,
                    payload.pellet,
                    payload.segment,
                );
                ctx.arg("source", source);
                ctx.arg("client", i64::from(client));
                ctx.arg("command", i64::from(command));
                ctx.arg("pellet", i64::from(pellet));
                ctx.arg("segment", i64::from(segment));
            });
        }
        let record = sim::PelletFxRecord {
            attacker: payload.attacker_entity_num,
            weapon: payload.weapon,
            correlation: payload.correlation,
            fire_cause: payload.fire_cause,
            pellet: payload.pellet,
            segment: payload.segment,
            hand: payload.hand,
            start: payload.origin2,
            end: payload.origin,
            normal: payload.direction,
            surf_type: payload.surf_type,
            surface_flags: payload.surface_flags,
            flesh_flags: payload.event_parm as u8,
        };
        presentation.present(
            FireFxRequest::event(
                &hit.0,
                FireFxOccurrence::Impact {
                    pellet: payload.pellet,
                    segment: payload.segment,
                },
                now,
            ),
            &record,
            u16::try_from(payload.other_entity_num)
                .ok()
                .filter(|&n| u32::from(n) < fx_iw4::FX_ENTITYNUM_WORLD),
            fx_world.view().as_ref().map(|s| s as &dyn FxScene),
        );
    }
}

fn drain_pellet_fx(
    mut pending: ResMut<PendingPelletFx>,
    mut presentation: PelletFxPresentation,
    fx_world: FxSceneAccess,
) {
    if pending.is_empty() {
        return;
    }
    for record in pending.take() {
        let request = FireFxRequest {
            cause: record.fire_cause,
            occurrence: FireFxOccurrence::Impact {
                pellet: record.pellet,
                segment: record.segment,
            },
            now: presentation.now(),
        };
        presentation.present(
            request,
            &record,
            None,
            fx_world.view().as_ref().map(|s| s as &dyn FxScene),
        );
    }
}

#[expect(
    clippy::too_many_arguments,
    reason = "Bevy system parameters keep scheduler resource access explicit."
)]
fn melee_blood(
    hit: On<net::EntityMeleeBlood>,
    identities: Query<&CEntity>,
    world_bolts: Query<&crate::adapters::anim::remote_body::RemoteFxBolts>,
    fpv_bolts: Res<crate::adapters::anim::fpv_present::FpvBoltTargets>,
    local: Res<LocalPresentClient>,
    presented: Res<PresentedSnapshot>,
    view_settings: (Res<ViewSubject>, Res<frame::GameSettings>),
    slots: Res<CEntitySlots>,
    runtime: FxRuntimeAccess,
    mut cursor: ResMut<FxJournalCursor>,
    fx_world: FxSceneAccess,
) {
    let FxRuntimeAccess {
        catalog,
        mut elem_infos,
        mut host,
    } = runtime;

    let (view, settings) = view_settings;
    let payload = hit.event.payload;
    let Some(catalog) = catalog else {
        return;
    };
    let eyes = match *view {
        ViewSubject::Seat {
            focus: Some(focus), ..
        } => i32::try_from(focus).unwrap_or(0),
        _ => i32::try_from(local.0.0).unwrap_or(0),
    };
    let gate = PlayerDrawGate {
        eyes_entity_num: eyes,
        other_flags: presented
            .player(local.0)
            .map(|ps| ps.other_flags)
            .unwrap_or(0),
        rendering_third_person: crate::adapters::anim::third_person::presented_is_third_person(
            &presented,
            local.0,
            view.in_killcam(),
            settings.third_person,
        ),
    };
    let player_view = identities
        .get(hit.entity)
        .ok()
        .is_some_and(|identity| gate.skip_self_fpv(identity.number()));
    let target = if player_view {
        fpv_bolts.knife[0].or(fpv_bolts.knife[1])
    } else {
        u16::try_from(payload.number)
            .ok()
            .and_then(|number| slots.entity_for_number(number))
            .and_then(|entity| world_bolts.get(entity).ok())
            .and_then(|bolts| bolts.knife)
    };
    let mut played = 0u32;
    try_play_weapon_fx_bolted(
        &mut host.0,
        &catalog.0,
        &mut elem_infos.0,
        Some(asset_game::FxName::engine("impacts/flesh_hit_knife")),
        target,
        &mut played,
        fx_world.view().as_ref().map(|s| s as &dyn FxScene),
    );
    cursor.impact_played = cursor.impact_played.saturating_add(played);
}

fn retire_entity_bolts(
    event: On<Remove<CEntity>>,
    identities: Query<&CEntity>,
    host: Option<ResMut<HostFxSystem>>,
) {
    if let Ok(identity) = identities.get(event.entity)
        && let Some(mut host) = host
    {
        host.0.kill_bolted(u32::from(identity.number()));
    }
}
