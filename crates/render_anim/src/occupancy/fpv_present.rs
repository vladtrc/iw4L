use std::collections::HashMap;

use anim_iw4::{DOBJ_RADIUS_PARENT_ROOT, dobj_compute_bounds_radius};
use assets::{
    AssetEdge, AssetNamespace, FpvHands, MaterialSpace, PreparedFpvMeshes, PreparedWeapons,
    PreparedXAnims, TS_COLOR_MAP, WeaponAnimations, WeaponRegistry,
};
use bevy::ecs::system::SystemParam;
use bevy::prelude::*;
use frame::{LifeFrontPublished, PresentedPublished, ViewSubject, WorkerCmdSet};
use math_iw4::vec3_length;
use net::{CgFrameClock, CgViewweaponAim, ClientSet, LocalPresentClient, PresentedSnapshot};
use render_scene::{SCENE_VIEWMODEL_ENTNUM, SCENE_VIEWMODEL_FX_FLAGS, SCENE_VIEWMODEL_LEFT_ENTNUM};

use crate::anim::fpv::{
    AuthorityFpvCues, EquippedFpv, FpvAuthoritySample, is_predicted_fire_weap_anim,
    local_shot_identity,
};
use crate::anim::fpv_host::{FpvGenerateArgs, FpvPoseKind, FpvPoseRefuse, generate_fpv_pose};
use crate::anim::fpv_pose::{PosedModelSurface, pose_eye};
use crate::anim::scene_submission::{AnimDObjSceneSubmission, AnimSceneSubmit};
use crate::anim::viewmodel_controller::ViewmodelController;
use crate::gaps::{RenderGap, RenderGapCause, RenderPresentationGaps};
use crate::model_vertex_diag::{ModelVertexColorMode, apply_model_vertex_color_diag};
use crate::occupancy::remote_body::RemotePlayer;
use crate::occupancy::third_person::presented_is_third_person;
use crate::occupancy::view_kick::{
    CgGunOffset, PendingViewHurt, SessionViewKick, apply_cg_gun_offset_view,
    apply_viewweapon_land_view, iw_view_placement_to_bevy_camera_local,
    reset_view_kick_on_life_started, sync_camera_from_presented, tick_session_view_kick,
};
use crate::{fpv_dobj_skel_radii, viewmodel_lighting_origin};
use hud_iw4::{
    WeaponAdsOverlayFacts, cg_calc_crosshair_position, cg_get_weap_reticle_zoom, cg_tan_half_fov,
    cg_viewweapon_drawgun, cg_viewweapon_drawgun_skip,
};
use math_iw4::angle_vectors;
use playerstate_iw4::PlayerState;
use render_material::{RuntimeMaterialCatalog, RuntimeSortedMaterialTable};
use render_scene::WorldScriptModelInstance;
use render_scene::{FlyCamera, FpvLens};
use render_scene::{HostGfxScene, scene_quat_from_viewmodel_axes};
use weapon_iw4::{
    GunKickSpring, GunRecoilPlacementState, PLACEMENT_ASSEMBLE_STEP_COUNT,
    StanceTransitionFadeGlobals, WeaponBobInputs, WeaponBobWaveformInputs,
    WeaponMovementKinematics, WeaponPlacementAssembleStep, WeaponPlacementPsInputs,
    WeaponPlacementState, WeaponStanceStaticOfsInputs, bg_calculate_weapon_movement_bob_waveform,
    bg_clip_table_key, bg_get_clip_for_hand, bg_get_viewmodel_weapon_index,
    dual_wield_view_model_origin_add, viewmodel_rocket_should_be_attached,
    viewweapon_iron_ads_saves_composed_axis, viewweapon_save_gun_pitch_yaw,
    viewweapon_view_to_world_delta, weapon_placement_assemble,
};

#[derive(SystemSet, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct FpvPlacementSet;

#[derive(Component)]
pub struct FpvViewmodel;

#[derive(Component)]
pub struct FpvPlacementRoot;

pub use crate::anim::fpv_host::{
    FpvBoltTargets, FpvHeldLife, FpvHeldSettled, FpvPoseProduct, FpvPresentCursor,
    PendingFpvNotetracks, PendingFpvSpawn, PendingFpvSpawnRequest,
};

#[derive(Resource, Default)]
pub struct SessionViewmodel(pub Option<SessionFpvMeshesHandles>);

pub struct SessionFpvMeshesHandles {
    pub weapon_id: u32,
    pub fpv: EquippedFpv,
    pub(crate) materials: Vec<render_scene::SmodelPassMaterial>,
    pub(crate) material_by_authored: HashMap<usize, u32>,
}

#[derive(Resource, Default)]
pub struct LocalSpawnArmed(pub bool);

#[derive(Resource, Default, Clone, Debug, PartialEq)]
pub struct FpvStatusGap(pub Option<FpvState>);

#[derive(Clone, Debug, PartialEq)]
pub enum FpvState {
    ClearedNotAlive,

    ClearedNoWeapon,

    Queued { from_gun_xmodel: bool },

    Drawn { idle_sampled: bool },

    Blocked(RenderGapCause),
}

impl FpvState {
    pub fn label(&self) -> &'static str {
        match self {
            FpvState::ClearedNotAlive => "not Alive — FPV cleared",
            FpvState::ClearedNoWeapon => "held weapon 0 — FPV cleared",
            FpvState::Queued {
                from_gun_xmodel: true,
            } => "held weapon changed; FPV queued (gunXModel[0])",
            FpvState::Queued {
                from_gun_xmodel: false,
            } => "held weapon changed; FPV queued via idle→gun candidate (Diagnostic)",
            FpvState::Drawn { idle_sampled: true } => {
                "spawned after Equip; idle-sampled eye-posed hands+gun; retained FPV + ModelLightingCache"
            }
            FpvState::Drawn {
                idle_sampled: false,
            } => {
                "spawned after Equip; bind-pose eye-posed hands+gun; idle sample gap; retained FPV"
            }
            FpvState::Blocked(cause) => cause.label(),
        }
    }

    pub fn cause(&self) -> Option<&RenderGapCause> {
        match self {
            FpvState::Blocked(cause) => Some(cause),
            _ => None,
        }
    }
}

impl core::fmt::Display for FpvState {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            FpvState::Blocked(cause) => write!(f, "{cause}"),
            other => f.write_str(other.label()),
        }
    }
}

pub fn resolve_fpv_gun_name(
    weapons: &assets::WeaponRegistry,
    fpv: &assets::FpvMeshCatalog,
    weapon_id: u32,
) -> Option<(String, bool)> {
    let ns = weapons
        .namespace_of(weapon_id)
        .unwrap_or(AssetNamespace::Iw4);
    if let Some(order) = weapons
        .gun_xmodel_edge_of(weapon_id)
        .and_then(|edge| edge.bound_index())
    {
        if let Some(entry) = fpv.get_at(order) {
            return Some((entry.skel.name.clone(), true));
        }
    }
    if let Some(gun) = weapons.gun_xmodel_of(weapon_id) {
        if fpv.contains(ns, gun) {
            return Some((gun.to_owned(), true));
        }

        return Some((gun.to_owned(), true));
    }
    let weapon_name = weapons.name_of(weapon_id);
    if weapon_name.is_empty() {
        return None;
    }
    if let Some(idle) = weapons.idle_anim_of(weapon_id) {
        for candidate in assets::gun_candidates_from_idle(idle) {
            if fpv.contains(ns, &candidate) {
                return Some((candidate, false));
            }
        }
    }

    let _ = fpv;
    None
}

fn fpv_rocket_model_name<'a>(
    weapons: &'a WeaponRegistry,
    weapon_id: u32,
    ps: Option<&PlayerState>,
) -> Option<&'a str> {
    let name = weapons.rocket_model_of(weapon_id)?;
    let Some(ps) = ps else {
        return Some(name);
    };
    let facts = weapons.facts_of(weapon_id)?;
    let viewmodel = bg_get_viewmodel_weapon_index(ps);
    let clip_key = bg_clip_table_key(facts.clip_index, viewmodel);
    let clip = bg_get_clip_for_hand(&ps.ammoclip, clip_key, 0);
    viewmodel_rocket_should_be_attached(
        clip,
        ps.weaponstate_primary,
        ps.weapon_time,
        facts.reload_time_ms,
        facts.reload_show_rocket_time_ms,
    )
    .then_some(name)
}

#[derive(SystemParam)]
pub struct SpawnPendingFpvInputs<'w> {
    weapons: Option<Res<'w, PreparedWeapons>>,
    fpv_meshes: Option<Res<'w, PreparedFpvMeshes>>,
    bodies: Option<Res<'w, assets::PreparedBodies>>,
    xanims: Option<Res<'w, PreparedXAnims>>,
    lighting: Option<Res<'w, render_scene::WorldModelLightingAtlas>>,
    tess: Res<'w, render_scene::TessMaterials>,
    presented: Res<'w, PresentedSnapshot>,
    local: Res<'w, LocalPresentClient>,
}

pub fn spawn_pending_fpv(
    mut commands: Commands,
    mut pending: ResMut<PendingFpvSpawn>,
    inputs: SpawnPendingFpvInputs,
    mut session_vm: ResMut<SessionViewmodel>,
    mut cursor: ResMut<FpvPresentCursor>,
    mut fpv_plan: ResMut<crate::FpvDrawPlan>,
    cameras: Query<Entity, With<FlyCamera>>,
    mut lenses: Query<
        &mut Transform,
        (
            With<FpvLens>,
            Without<RemotePlayer>,
            Without<WorldScriptModelInstance>,
        ),
    >,
    existing_fpv: Query<Entity, With<FpvPlacementRoot>>,
    mut status: ResMut<FpvStatusGap>,
    gaps: Res<RenderPresentationGaps>,
) {
    let SpawnPendingFpvInputs {
        weapons,
        fpv_meshes,
        bodies,
        xanims,
        lighting,
        tess,
        presented,
        local,
    } = inputs;
    let Some(request) = pending.0.take() else {
        return;
    };
    cursor.0.forget_weap_anim();
    for entity in &existing_fpv {
        commands.entity(entity).try_despawn();
    }
    session_vm.0 = None;

    let idle_clip = weapons
        .as_ref()
        .zip(xanims.as_ref())
        .and_then(|(reg, cat)| {
            let row = reg.0.sz_xanim_edges_of(request.weapon_id)?;
            let order = row[assets::weap_anim::IDLE].bound_index()?;
            cat.0.clip_at(order)
        });
    let idle_from_table = idle_clip.is_some();
    let idle_name = idle_clip.as_ref().map(|clip| clip.name.clone());
    let idle_ns = weapons
        .as_ref()
        .and_then(|reg| reg.0.namespace_of(request.weapon_id))
        .unwrap_or(AssetNamespace::Iw4);
    let Some(fpv_cat) = fpv_meshes.as_ref() else {
        gaps.raise(RenderGapCause::FpvCatalogMissing);
        status.0 = Some(FpvState::Blocked(RenderGapCause::FpvCatalogMissing));
        return;
    };
    let hide_tags: Vec<String> = weapons
        .as_ref()
        .map(|reg| assets::effective_hide_tags(&reg.0, request.weapon_id))
        .unwrap_or_default();
    let scope_name = weapons
        .as_ref()
        .and_then(|reg| reg.0.scope_viewmodel_of(request.weapon_id));
    let rocket_name = weapons.as_ref().and_then(|reg| {
        fpv_rocket_model_name(&reg.0, request.weapon_id, presented.player(local.0))
    });
    let map_ns = fpv_cat.0.map_namespace.unwrap_or(idle_ns);
    let meta = presented
        .snapshot()
        .and_then(|snap| snap.meta.for_client(local.0));
    let ffa_team = meta.and_then(|m| m.ffa_team);
    let client_state_team = meta.map(|m| m.client_state_team).unwrap_or(0);
    let kit = bodies.as_ref().and_then(|b| {
        b.0.kits()
            .kit(assets::kit_assignment_is_axis(client_state_team, ffa_team))
            .cloned()
    });
    let weapon_hand_owned = weapons.as_ref().and_then(|reg| {
        reg.0
            .hand_xmodel_edge_of(request.weapon_id)
            .and_then(|edge| edge.bound_index())
            .and_then(|order| fpv_cat.0.get_at(order))
            .map(|entry| entry.skel.name.clone())
            .or_else(|| reg.0.hand_xmodel_of(request.weapon_id).map(str::to_owned))
    });
    let hands = FpvHands::resolve(
        &fpv_cat.0,
        map_ns,
        kit.as_ref(),
        weapon_hand_owned.as_deref(),
        idle_ns,
    );
    let Some(posed) = pose_eye(
        &fpv_cat.0,
        idle_ns,
        &request.gun_xmodel,
        &hands,
        idle_clip.as_deref(),
        0.0,
        &hide_tags,
        scope_name,
        rocket_name,
    ) else {
        diag::info!(
            Fpv,
            "fpv: cannot eye-pose `{}` (need viewhands+gun skels with tag_view)",
            request.gun_xmodel
        );
        let cause = RenderGapCause::FpvEyePoseFailed {
            gun_xmodel: request.gun_xmodel.clone(),
        };
        gaps.raise(cause.clone());
        status.0 = Some(FpvState::Blocked(cause));
        return;
    };
    let (hands_surfaces, gun_surfaces, stats, lens) =
        (posed.hands, posed.gun, posed.stats, posed.lens);
    let Ok(host) = cameras.single() else {
        diag::info!(
            Fpv,
            "fpv: no FlyCamera to parent viewmodel under — re-queue"
        );
        pending.0 = Some(request);
        gaps.raise(RenderGapCause::FpvNoCamera);
        status.0 = Some(FpvState::Blocked(RenderGapCause::FpvNoCamera));
        return;
    };

    let Some(lighting) = lighting.as_ref() else {
        diag::info!(
            Fpv,
            "fpv: no WorldSmodelLighting atlas — refuse Bevy-unlit glow (ModelLightingCache gap)"
        );
        gaps.raise(RenderGapCause::FpvNoLightingAtlas);
        status.0 = Some(FpvState::Blocked(RenderGapCause::FpvNoLightingAtlas));
        return;
    };

    if !matches!(
        tess.catalog.sorted_materials,
        RuntimeSortedMaterialTable::Ready { .. }
    ) {
        diag::info!(
            Fpv,
            "fpv: global sorted material table not READY — re-queue (clone-index is not an ordinal)"
        );
        pending.0 = Some(request);
        return;
    }

    let mut image_cache: HashMap<u32, Handle<Image>> = HashMap::new();
    let mut materials: Vec<render_scene::SmodelPassMaterial> = Vec::new();
    let mut material_by_authored: HashMap<usize, u32> = HashMap::new();
    let mut ordinal_admitted = 0u32;
    let mut ordinal_refused: Vec<String> = Vec::new();
    let mut gun_skip = FpvGunSkipCensus::default();
    let hands_entry = fpv_cat.0.get_hands(&hands);
    let gun_entry = fpv_cat.0.get(idle_ns, &request.gun_xmodel);
    let scope_entry = scope_name.and_then(|name| fpv_cat.0.get(idle_ns, name));
    let rocket_entry = rocket_name.and_then(|name| fpv_cat.0.get(idle_ns, name));
    let mut plan_mat_hints: Vec<String> = Vec::new();

    let mut pack_surfaces = |surfaces: Vec<PosedModelSurface>| -> Vec<crate::FpvPlanSurface> {
        use crate::anim::fpv_pose::FpvSurfOwner;
        let mut out = Vec::new();
        let color_mode = ModelVertexColorMode::from_env();
        for surface in surfaces {
            let hidden = surface.mesh.indices().is_none_or(|ix| ix.len() == 0);
            let (label, entry) = match surface.owner {
                FpvSurfOwner::Hands => ("fpv hands", hands_entry),
                FpvSurfOwner::Gun => ("fpv gun", gun_entry),
                FpvSurfOwner::Scope => ("fpv gun", scope_entry),
                FpvSurfOwner::Rocket => ("fpv gun", rocket_entry),
            };
            let edge = entry
                .and_then(|e| e.material_edges.get(surface.surface_index).copied())
                .unwrap_or(AssetEdge::Absent);
            let present_name = entry.and_then(|e| e.material_present_name(surface.surface_index));
            let leftover_hint = entry.and_then(|e| {
                e.material_names
                    .get(surface.surface_index)
                    .and_then(|n| n.as_deref())
            });
            let Some(mat) = fpv_pass_material(
                &tess.catalog,
                tess.material_images.as_ref(),
                surface.material,
                edge,
                present_name,
                leftover_hint,
                &mut image_cache,
                &mut material_by_authored,
                &mut materials,
                lighting,
                label,
                surface.surface_index,
                &mut ordinal_admitted,
                &mut ordinal_refused,
                &mut gun_skip,
            ) else {
                continue;
            };
            if let Some(hint) = leftover_hint.or(present_name) {
                if plan_mat_hints.len() < 12 && !plan_mat_hints.iter().any(|seen| seen == hint) {
                    plan_mat_hints.push(hint.to_owned());
                }
            }
            if hidden {
                continue;
            }
            let mut mesh = surface.mesh;
            apply_model_vertex_color_diag(&mut mesh, color_mode, surface.surface_index);
            out.push(crate::FpvPlanSurface {
                mesh,
                authored: surface.material,
                material: mat,
                packed_vertices: surface.packed_vertices,
                is_scope: surface.owner == FpvSurfOwner::Scope,
                is_lens: leftover_hint.is_some_and(leftover_scope_surf_is_lens),
            });
        }
        out
    };

    let hands_n = hands_surfaces.len();
    let posed_gun_n = gun_surfaces.len();
    let hands_packed = pack_surfaces(hands_surfaces);
    let gun_packed = pack_surfaces(gun_surfaces);
    drop(pack_surfaces);
    let hands_plan_n = hands_packed.len();
    let gun_plan_n = gun_packed.len();
    let mut all_packed = hands_packed;
    all_packed.extend(gun_packed);

    for mut lens_tf in &mut lenses {
        *lens_tf = Transform::from_matrix(lens);
    }

    commands.entity(host).with_children(|parent| {
        parent.spawn((
            FpvViewmodel,
            FpvPlacementRoot,
            Transform::IDENTITY,
            Visibility::Visible,
        ));
    });

    crate::rebuild_fpv_draw_plan(&mut fpv_plan, &all_packed, 0, false);
    fpv_plan.hands_plan_n = Some(hands_plan_n as u32);
    fpv_plan.gun_plan_n = Some(gun_plan_n as u32);
    fpv_plan.gun_colormap_skip_n = Some(gun_skip.colormap_n);
    fpv_plan.gun_ordinal_skip_n = Some(gun_skip.ordinal_n);
    fpv_plan.gun_colormap_skip_names =
        (!gun_skip.colormap_names.is_empty()).then(|| gun_skip.colormap_names.join(","));
    fpv_plan.plan_mat_hints = (!plan_mat_hints.is_empty()).then(|| plan_mat_hints.join(","));
    fpv_plan.plan_skip_n = Some(
        (hands_n
            .saturating_add(posed_gun_n)
            .saturating_sub(all_packed.len())) as u32,
    );
    match &fpv_plan.packed_vertices {
        assets::RetailPackedVertexPayload::Iw4(vertices) => diag::info!(
            Fpv,
            "fpv: packed admission model=`{}` verts={}",
            request.gun_xmodel,
            vertices.len()
        ),
        assets::RetailPackedVertexPayload::Unavailable { source_layout } => diag::info!(
            Fpv,
            "fpv: packed unavailable model=`{}` layout={source_layout}",
            request.gun_xmodel
        ),
    }
    let (controller, left) = match (weapons.as_ref(), xanims.as_ref()) {
        (Some(reg), Some(cat)) => {
            let ns = reg
                .0
                .namespace_of(request.weapon_id)
                .unwrap_or(AssetNamespace::Iw4);
            let right_idle = reg.0.idle_anim_right_of(request.weapon_id);
            let no_dual = reg
                .0
                .facts_of(request.weapon_id)
                .map(|f| u8::from(f.no_dual_wield))
                .unwrap_or(1);
            let create_lr = weapon_iw4::bg_create_akimbo_viewmodel_trees(no_dual, right_idle);
            let animations = if create_lr {
                let mut resolve = |name: &str| cat.0.clip(ns, name);
                WeaponAnimations::from_registry_table(
                    &reg.0,
                    request.weapon_id,
                    reg.0.sz_xanims_right_of(request.weapon_id),
                    &mut resolve,
                )
            } else {
                WeaponAnimations::from_registry(&reg.0, request.weapon_id, &cat.0)
            };
            let resolved = animations.resolved_count();
            let (fire_ms, raise_ms) = (animations.fire_time_ms, animations.raise_time_ms);
            let switch_ms = format!(
                "dropTime={}ms quickDropTime={}ms quickRaiseTime={}ms",
                animations.drop_time_ms,
                animations.quick_drop_time_ms,
                animations.quick_raise_time_ms
            );
            let idle_meta = animations
                .clip(assets::WeaponAnimSlot::Idle)
                .map(|clip| {
                    format!(
                        "idle=`{}` frames={} loop={} dur={:.2}s",
                        clip.name,
                        clip.numframes,
                        clip.looping,
                        clip.duration()
                    )
                })
                .unwrap_or_else(|| "idle=missing".into());
            let left = if create_lr
                && weapon_iw4::bg_has_akimbo_viewmodel_anims(
                    reg.0.idle_anim_left_of(request.weapon_id),
                ) {
                let mut resolve = |name: &str| cat.0.clip(ns, name);
                let left_anims = WeaponAnimations::from_registry_table(
                    &reg.0,
                    request.weapon_id,
                    reg.0.sz_xanims_left_of(request.weapon_id),
                    &mut resolve,
                );
                let left_n = left_anims.resolved_count();
                diag::info!(
                    Fpv,
                    "fpv: viewmodel controller `{}` — {resolved} clips R, {left_n} clips L, fireTime={fire_ms}ms raiseTime={raise_ms}ms {switch_ms} {idle_meta} (dual DObj; action from weapAnim)",
                    animations.name
                );
                Some(ViewmodelController::new(left_anims))
            } else {
                diag::info!(
                    Fpv,
                    "fpv: viewmodel controller `{}` — {resolved} clips, fireTime={fire_ms}ms raiseTime={raise_ms}ms {switch_ms} {idle_meta} (action from weapAnim; Fire predicted)",
                    animations.name
                );
                None
            };
            (ViewmodelController::new(animations), left)
        }
        _ => {
            diag::warn!(
                Fpv,
                "fpv: weapon/xanim catalog missing — static idle pose only"
            );
            (
                ViewmodelController::new(WeaponAnimations::resolve(
                    "",
                    &[const { None }; assets::WEAPON_ANIM_COUNT],
                    0,
                    0,
                    |_| None,
                )),
                None,
            )
        }
    };
    session_vm.0 = Some(SessionFpvMeshesHandles {
        weapon_id: request.weapon_id,
        fpv: EquippedFpv::new(request.gun_xmodel.clone(), idle_ns, hands, controller, left),
        materials,
        material_by_authored,
    });

    let pose_kind = if stats.idle_sampled {
        let name = idle_name.as_deref().unwrap_or("?");
        format!("idle sample `{name}` (szXAnims[IDLE])")
    } else if idle_from_table {
        "bind-pose (szXAnims[IDLE] present; DObj pose failed)".into()
    } else {
        "bind-pose (no szXAnims[IDLE] — guess forbidden)".into()
    };
    let refused_names = if ordinal_refused.is_empty() {
        "none".to_owned()
    } else {
        ordinal_refused
            .iter()
            .take(8)
            .cloned()
            .collect::<Vec<_>>()
            .join(",")
    };
    diag::info!(
        Fpv,
        "fpv: ordinal admission admitted={ordinal_admitted} refused={} names=[{refused_names}] (whole model incl. hideTags-hidden surfaces; global catalog, clone-index unused)",
        ordinal_refused.len(),
    );
    diag::info!(
        Fpv,
        "fpv: eye-posed `{}` ({}; {}; hands surfaces={} rigid/blend={}/{} gun surfaces={} rigid/blend={}/{}; retained path, no Mesh3d)",
        request.gun_xmodel,
        if request.from_gun_xmodel {
            "gunXModel[0]"
        } else {
            "idle→gun candidate (Diagnostic)"
        },
        pose_kind,
        hands_n,
        stats.hands_rigid,
        stats.hands_blend,
        posed_gun_n,
        stats.gun_rigid,
        stats.gun_blend,
    );

    gaps.clear(RenderGap::FpvViewmodel);
    status.0 = Some(FpvState::Drawn {
        idle_sampled: stats.idle_sampled,
    });
}

#[derive(Default)]
struct FpvGunSkipCensus {
    colormap_n: u32,
    ordinal_n: u32,
    colormap_names: Vec<String>,
}

impl FpvGunSkipCensus {
    fn note_colormap(&mut self, is_gun: bool, name: Option<&str>) {
        if !is_gun {
            return;
        }
        self.colormap_n = self.colormap_n.saturating_add(1);
        let Some(name) = name else {
            return;
        };
        if self.colormap_names.len() < 8 && !self.colormap_names.iter().any(|seen| seen == name) {
            self.colormap_names.push(name.to_owned());
        }
    }

    fn note_ordinal(&mut self, is_gun: bool) {
        if is_gun {
            self.ordinal_n = self.ordinal_n.saturating_add(1);
        }
    }
}

fn fpv_pass_material(
    global: &RuntimeMaterialCatalog,
    images: &[Option<Handle<Image>>],
    material_index: Option<usize>,
    edge: AssetEdge<MaterialSpace>,
    present_name: Option<&str>,
    leftover_hint: Option<&str>,
    image_cache: &mut HashMap<u32, Handle<Image>>,
    material_by_authored: &mut HashMap<usize, u32>,
    materials: &mut Vec<render_scene::SmodelPassMaterial>,
    lighting: &render_scene::WorldModelLightingAtlas,
    label: &str,
    surface_index: usize,
    ordinal_admitted: &mut u32,
    ordinal_refused: &mut Vec<String>,
    gun_skip: &mut FpvGunSkipCensus,
) -> Option<render_scene::SmodelPassMaterial> {
    use lighting_iw4::{
        MODEL_LIGHTING_INV_ATLAS_WIDTH, MODEL_LIGHTING_VOLUME_W, model_lighting_inv_image_height,
        model_lighting_lookup_scale,
    };
    use render_scene::SmodelPassMaterial;

    let is_gun = label == "fpv gun";
    if !edge.is_bound() {
        diag::info!(
            Fpv,
            "{label} surface {surface_index}: materialHandles {} leftover=`{}` — skip (leftover name is not identity)",
            edge.edge_kind(),
            leftover_hint.unwrap_or("-")
        );
        return None;
    }
    let Some(present_name) = present_name else {
        diag::info!(
            Fpv,
            "{label} surface {surface_index}: Bound materialHandles has no present name — skip"
        );
        return None;
    };

    let mat_i = material_index?;
    if let Some(&idx) = material_by_authored.get(&mat_i) {
        return materials.get(idx as usize).cloned();
    }
    let bound = edge.bound_index()?;
    let authored = global.materials.get(bound)?;
    let Some(ordinal) = global.ordinal_for_material_name(present_name) else {
        diag::info!(
            Fpv,
            "{label} surface {surface_index}: Bound `{present_name}` global-index={bound} has no sorted ordinal — skip (not packed as 0)"
        );
        if !ordinal_refused.iter().any(|name| name == present_name) {
            ordinal_refused.push(present_name.to_owned());
        }
        gun_skip.note_ordinal(is_gun);
        return None;
    };
    *ordinal_admitted = ordinal_admitted.saturating_add(1);
    let color_texture = {
        let image_index = authored
            .textures
            .iter()
            .find_map(|(_, texture)| texture.filter(|binding| binding.semantic == TS_COLOR_MAP))?
            .image
            .0;
        if let Some(cached) = image_cache.get(&image_index) {
            Some(cached.clone())
        } else {
            let Some(handle) = images.get(image_index as usize).cloned().flatten() else {
                diag::info!(
                    Fpv,
                    "{label} surface {surface_index}: no uploaded color map — skip (no unlit stand-in)"
                );
                gun_skip.note_colormap(is_gun, Some(present_name).or(leftover_hint));
                return None;
            };
            image_cache.insert(image_index, handle.clone());
            Some(handle)
        }
    };
    let specular_texture = authored
        .textures
        .iter()
        .find_map(|(_, texture)| {
            texture.filter(|binding| binding.semantic == assets::TS_SPECULAR_MAP)
        })
        .and_then(|binding| {
            let image_index = binding.image.0;
            if let Some(cached) = image_cache.get(&image_index) {
                return Some(cached.clone());
            }
            let handle = images.get(image_index as usize).cloned().flatten()?;
            image_cache.insert(image_index, handle.clone());
            Some(handle)
        });

    const ENV_MAP_PARMS: u32 = 1_033_475_292;
    let env_map_parms = authored
        .constants
        .iter()
        .find(|(hash, _)| *hash == ENV_MAP_PARMS)
        .map(|(_, words)| words.map(f32::from_bits))
        .unwrap_or([0.0; 4]);
    let alpha_mode = AlphaMode::Opaque;
    let inv_h = model_lighting_inv_image_height(lighting.dims.image_height)?;
    let scale = model_lighting_lookup_scale(inv_h);
    let cull_mode =
        authored
            .state_bits_table
            .first()
            .and_then(|bits| match assets::cull_face_from_state_bits(*bits) {
                assets::MaterialCullFace::Back => Some(bevy::render::render_resource::Face::Back),
                assets::MaterialCullFace::Front => Some(bevy::render::render_resource::Face::Front),
                assets::MaterialCullFace::None => None,
            });
    let mat = SmodelPassMaterial {
        model_lighting_required: true,
        color: color_texture,
        specular: specular_texture,
        probe: None,
        atlas: Some(lighting.image.clone()),
        alpha_mode,
        draw_mode: None,
        cull_mode,
        env_map_parms,
        lighting_lookup_scale: [scale.u, scale.v, scale.w, scale.q],
        atlas_lookup: [
            MODEL_LIGHTING_INV_ATLAS_WIDTH as f32,
            inv_h,
            MODEL_LIGHTING_VOLUME_W,
            0.0,
        ],
        sort_key: authored.sort_key,
        material_sorted_index: Some(ordinal.get()),
    };
    let idx = materials.len() as u32;
    materials.push(mat.clone());
    material_by_authored.insert(mat_i, idx);
    Some(mat)
}

fn viewweapon_drawgun_admit(
    ps: &PlayerState,
    weapons: Option<&WeaponRegistry>,
    b_position_to_ads: bool,
) -> Option<(bool, Option<&'static str>)> {
    let reg = weapons?;
    let viewmodel = bg_get_viewmodel_weapon_index(ps);
    let facts = reg.facts_of(viewmodel)?;

    let hud_iris = reg.overlay_is_hud_iris(viewmodel);
    let weap = WeaponAdsOverlayFacts {
        ads_zoom_in_frac: facts.ads_zoom_in_frac,
        ads_zoom_out_frac: facts.ads_zoom_out_frac,
        overlay_material: u32::from(hud_iris),
        overlay_reticle: if hud_iris {
            if facts.overlay_reticle != 0 {
                facts.overlay_reticle
            } else {
                1
            }
        } else {
            0
        },
        ads_overlay_width: facts.ads_overlay_width,
        ads_overlay_height: facts.ads_overlay_height,
        ..WeaponAdsOverlayFacts::default()
    };
    let iris = cg_get_weap_reticle_zoom(ps.f_weapon_pos_frac, b_position_to_ads, &weap);
    Some((
        cg_viewweapon_drawgun(false, true, iris),
        cg_viewweapon_drawgun_skip(false, true, iris),
    ))
}

fn viewweapon_drawgun_value(
    ps: &PlayerState,
    weapons: Option<&WeaponRegistry>,
    b_position_to_ads: bool,
) -> Option<i32> {
    let (admit, _skip) = viewweapon_drawgun_admit(ps, weapons, b_position_to_ads)?;
    Some(i32::from(admit))
}

fn fpv_occupy_submission(
    lighting: [f32; 3],
    radius: Option<f32>,
    entnum: u32,
    model_n: u8,
) -> AnimDObjSceneSubmission {
    AnimDObjSceneSubmission {
        render_fx_flags: SCENE_VIEWMODEL_FX_FLAGS,
        has_tree: true,
        origin: lighting,
        lighting_origin: lighting,
        radius,
        entnum,
        quat: None,
        occupy_model_n: model_n,
        models: Vec::new(),
        hide_part_bits: [0; 6],
        store_skin: false,
    }
}

pub fn occupy_fpv_scene(
    mut submissions: MessageWriter<AnimDObjSceneSubmission>,
    presented: Res<PresentedSnapshot>,
    local: Res<LocalPresentClient>,
    view: Res<ViewSubject>,
    weapons: Option<Res<PreparedWeapons>>,
    kick: Option<Res<SessionViewKick>>,
    session_vm: Option<Res<SessionViewmodel>>,
    fpv_meshes: Option<Res<PreparedFpvMeshes>>,
) {
    if presented_is_third_person(&presented, local.0, view.in_killcam()) {
        return;
    }
    let Some(ps) = presented.viewweapon_player(local.0) else {
        return;
    };
    let admit = viewweapon_drawgun_admit(
        ps,
        weapons.as_ref().map(|w| &w.0),
        kick.as_ref().map(|k| k.b_position_to_ads).unwrap_or(true),
    );
    if !admit.is_some_and(|(ok, _)| ok) {
        return;
    }
    let Some(session) = session_vm.as_ref().and_then(|s| s.0.as_ref()) else {
        return;
    };
    let lighting = viewmodel_lighting_origin(
        ps.origin,
        ps.view_height_current,
        ps.viewangles[1],
        ps.leanf,
    );
    let radius = fpv_meshes.as_ref().and_then(|cat| {
        let (hands, gun) =
            fpv_dobj_skel_radii(&cat.0, session.fpv.namespace, &session.fpv.gun_xmodel);
        match (hands, gun) {
            (Some(h), Some(g)) => Some(dobj_compute_bounds_radius(
                &[h, g],
                &[DOBJ_RADIUS_PARENT_ROOT, 0],
            )),
            (Some(r), None) | (None, Some(r)) => Some(r),
            (None, None) => None,
        }
    });
    let model_n: u8 = if session.fpv.gun_xmodel.is_empty() {
        1
    } else {
        2
    };
    submissions.write(fpv_occupy_submission(
        lighting,
        radius,
        SCENE_VIEWMODEL_ENTNUM,
        model_n,
    ));
    if ps.last_weapon_hand == 1 {
        submissions.write(fpv_occupy_submission(
            lighting,
            radius,
            SCENE_VIEWMODEL_LEFT_ENTNUM,
            model_n,
        ));
    }
}

fn refuse_gap_cause(refuse: FpvPoseRefuse) -> RenderGapCause {
    match refuse {
        FpvPoseRefuse::CatalogMissing => RenderGapCause::FpvCatalogMissing,
        FpvPoseRefuse::NoActiveClips => RenderGapCause::FpvNoActiveClips,
        FpvPoseRefuse::EyePoseFailed { gun_xmodel } => {
            RenderGapCause::FpvEyePoseFailed { gun_xmodel }
        }
    }
}

pub fn tick_fpv_viewmodel(
    time: Res<Time>,
    presented: Res<PresentedSnapshot>,
    local: Res<LocalPresentClient>,
    mut cursor: ResMut<FpvPresentCursor>,
    weapons: Option<Res<PreparedWeapons>>,
    fpv_meshes: Option<Res<PreparedFpvMeshes>>,
    mut session_vm: ResMut<SessionViewmodel>,
    mut pending: ResMut<PendingFpvSpawn>,
    mut product: ResMut<FpvPoseProduct>,
    mut pending_notes: ResMut<PendingFpvNotetracks>,
    mut bolts: ResMut<FpvBoltTargets>,
    kick: Option<Res<SessionViewKick>>,
    view: Res<ViewSubject>,
) {
    pending_notes.weapon = 0;
    pending_notes.names.clear();
    bolts.clear();
    *product = FpvPoseProduct::default();
    if let Some(ps) = presented.viewweapon_player(local.0) {
        if !presented_is_third_person(&presented, local.0, view.in_killcam()) {
            product.drawgun = viewweapon_drawgun_value(
                ps,
                weapons.as_ref().map(|w| &w.0),
                kick.as_ref().map(|k| k.b_position_to_ads).unwrap_or(true),
            );
        }
    }
    let Some(session) = session_vm.0.as_mut() else {
        product.kind = FpvPoseKind::Hide;
        return;
    };
    if presented_is_third_person(&presented, local.0, view.in_killcam()) {
        product.kind = FpvPoseKind::Hide;
        return;
    }
    let Some(fpv) = fpv_meshes.as_ref() else {
        product.kind = FpvPoseKind::Refuse(FpvPoseRefuse::CatalogMissing);
        return;
    };

    if let Some(ps) = presented.viewweapon_player(local.0) {
        let weapon = bg_get_viewmodel_weapon_index(ps);
        if weapon != 0 && weapon != session.weapon_id {
            if let Some(reg) = weapons.as_ref() {
                match resolve_fpv_gun_name(&reg.0, &fpv.0, weapon) {
                    Some((gun, from_gun)) if gun == session.fpv.gun_xmodel => {
                        session.weapon_id = weapon;
                        let _ = from_gun;
                        diag::info!(
                            Fpv,
                            "fpv: weapon id → {weapon} (`{}`); hideTags refresh (same gunXModel)",
                            reg.0.name_of(weapon)
                        );
                    }
                    Some((gun, from_gun)) => {
                        let idle_anim = reg.0.idle_anim_of(weapon).map(str::to_owned);
                        pending.0 = Some(PendingFpvSpawnRequest {
                            gun_xmodel: gun,
                            weapon_id: weapon,
                            idle_anim,
                            from_gun_xmodel: from_gun,
                        });
                        diag::info!(
                            Fpv,
                            "fpv: weapon id → {weapon}; re-queue FPV for new gunXModel"
                        );
                        product.kind = FpvPoseKind::Hide;
                        return;
                    }
                    None => {
                        product.kind = FpvPoseKind::Hide;
                        return;
                    }
                }
            } else {
                session.weapon_id = weapon;
            }
        }
    }

    let hide_tags: Vec<String> = weapons
        .as_ref()
        .map(|reg| assets::effective_hide_tags(&reg.0, session.weapon_id))
        .unwrap_or_default();
    let scope_name = weapons
        .as_ref()
        .and_then(|reg| reg.0.scope_viewmodel_of(session.weapon_id));

    let dt = time.delta_secs();
    let (sample, predicted_fire) = match presented.snapshot() {
        Some(snap) => {
            let ps = presented.player(local.0);
            let ws = ps.map(|p| p.weaponstate_primary).unwrap_or(0);
            const PMF_SPRINTING: u32 = 0x4000;
            let sprinting = ps
                .map(|p| (p.pm_flags & PMF_SPRINTING) != 0)
                .unwrap_or(false);
            let ads_frac = ps.map(|p| p.f_weapon_pos_frac).unwrap_or(0.0);
            let weap_anim = ps.map(|p| p.weap_anim).unwrap_or(0);
            let clip_ammo = match (ps, weapons.as_ref()) {
                (Some(ps), Some(reg)) => {
                    let viewmodel = bg_get_viewmodel_weapon_index(ps);
                    reg.0.facts_of(viewmodel).map(|facts| {
                        let key = bg_clip_table_key(facts.clip_index, viewmodel);
                        bg_get_clip_for_hand(&ps.ammoclip, key, 0)
                    })
                }
                _ => None,
            };
            let cues = presented.fpv_cues(local.0);
            let predicted_fire = if let Some(ps) = ps {
                let life = snap
                    .meta
                    .for_client(local.0)
                    .map(|m| m.life_sequence.0)
                    .unwrap_or(0);
                let id = local_shot_identity(life, 0, ps.weapon_shot_count, ps.weap_anim);
                let edged = cursor.0.local_shot.observe(id);
                let masked = ps.weap_anim as u32 & weapon_iw4::WEAP_ANIM_EVENT_MASK;
                edged && is_predicted_fire_weap_anim(masked)
            } else {
                false
            };
            (
                Some(FpvAuthoritySample {
                    tick: snap.tick.0,
                    weaponstate: ws,
                    cues: AuthorityFpvCues {
                        shot_accepted: false,
                        attack_released: cues.attack_released,
                        spawned: cues.spawned,
                    },
                    sprinting,
                    ads_frac,
                    weap_anim,
                    weap_anim_secondary: ps.map(|p| p.weap_anim_secondary).unwrap_or(0),
                    last_weapon_hand: ps.map(|p| p.last_weapon_hand).unwrap_or(0),
                    perks0: ps.map(|p| p.perks[0]).unwrap_or(0),
                    clip_ammo,
                }),
                predicted_fire,
            )
        }
        None => (None, false),
    };
    let rocket_name = weapons.as_ref().and_then(|reg| {
        fpv_rocket_model_name(&reg.0, session.weapon_id, presented.player(local.0))
    });
    let dual = presented
        .viewweapon_player(local.0)
        .is_some_and(|ps| ps.last_weapon_hand == 1);
    let dual_offset = if dual {
        presented.viewweapon_player(local.0).and_then(|ps| {
            weapons
                .as_ref()?
                .0
                .facts_of(bg_get_viewmodel_weapon_index(ps))
                .map(|f| f.dual_wield_view_model_offset)
        })
    } else {
        None
    };
    let weapon_id = session.weapon_id;
    let kind = generate_fpv_pose(FpvGenerateArgs {
        dt,
        equipped: &mut session.fpv,
        cursor: &mut cursor.0,
        catalog: &fpv.0,
        hide_tags: &hide_tags,
        scope_name,
        rocket_name,
        sample,
        predicted_fire,
        dual,
        dual_offset,
    });
    if let FpvPoseKind::Posed(frame) = &kind {
        if weapon_id != 0 && !frame.notetracks.is_empty() {
            pending_notes.weapon = weapon_id;
            pending_notes.names.clone_from(&frame.notetracks);
        }
        for (hand, bolt) in frame.bolts.iter().enumerate() {
            if let Some(bolt) = bolt.clone() {
                bolts.set_pose(hand, bolt);
            }
        }
    }
    product.kind = kind;
}

fn commit_fpv_draw_plan(
    product: Res<FpvPoseProduct>,
    session_vm: Option<Res<SessionViewmodel>>,
    fpv_meshes: Option<Res<PreparedFpvMeshes>>,
    mut fpv_plan: ResMut<crate::FpvDrawPlan>,
    mut status: ResMut<FpvStatusGap>,
    gaps: Res<RenderPresentationGaps>,
    mut lenses: Query<
        &mut Transform,
        (
            With<FpvLens>,
            Without<RemotePlayer>,
            Without<WorldScriptModelInstance>,
        ),
    >,
) {
    fpv_plan.drawgun = product.drawgun;
    match &product.kind {
        FpvPoseKind::Hide => {
            let handle = fpv_plan.lighting_handle;
            crate::rebuild_fpv_draw_plan(&mut fpv_plan, &[], handle, false);
        }
        FpvPoseKind::Refuse(refuse) => {
            let cause = refuse_gap_cause(refuse.clone());
            let handle = fpv_plan.lighting_handle;
            crate::rebuild_fpv_draw_plan(&mut fpv_plan, &[], handle, false);
            gaps.raise(cause.clone());
            status.0 = Some(FpvState::Blocked(cause));
        }
        FpvPoseKind::Posed(frame) => {
            let Some(session) = session_vm.as_ref().and_then(|s| s.0.as_ref()) else {
                let handle = fpv_plan.lighting_handle;
                crate::rebuild_fpv_draw_plan(&mut fpv_plan, &[], handle, false);
                return;
            };
            let scope_entry = frame.scope_xmodel.as_deref().and_then(|n| {
                fpv_meshes
                    .as_ref()
                    .and_then(|cat| cat.0.get(session.fpv.namespace, n))
            });
            let color_mode = ModelVertexColorMode::from_env();
            let posed_n = frame.hands.len().saturating_add(frame.gun.len());
            let mut packed: Vec<crate::FpvPlanSurface> = Vec::new();
            for surface in frame.hands.iter().chain(frame.gun.iter()) {
                if surface.mesh.indices().is_none_or(|ix| ix.len() == 0) {
                    continue;
                }
                let Some(ai) = surface.material else {
                    continue;
                };
                let Some(&mat_idx) = session.material_by_authored.get(&ai) else {
                    continue;
                };
                let Some(mat) = session.materials.get(mat_idx as usize).cloned() else {
                    continue;
                };
                let leftover_name = match surface.owner {
                    crate::anim::fpv_pose::FpvSurfOwner::Scope => scope_entry.and_then(|e| {
                        e.material_names
                            .get(surface.surface_index)
                            .and_then(|n| n.as_deref())
                    }),
                    _ => None,
                };
                let is_lens = leftover_name.is_some_and(leftover_scope_surf_is_lens);
                let mut mesh = surface.mesh.clone();
                apply_model_vertex_color_diag(&mut mesh, color_mode, surface.surface_index);
                packed.push(crate::FpvPlanSurface {
                    mesh,
                    authored: surface.material,
                    material: mat,
                    packed_vertices: surface.packed_vertices.clone(),
                    is_scope: surface.owner == crate::anim::fpv_pose::FpvSurfOwner::Scope,
                    is_lens,
                });
            }
            let handle = fpv_plan.lighting_handle;
            let visible = !packed.is_empty();
            let admitted_n = packed.len();
            crate::rebuild_fpv_draw_plan(&mut fpv_plan, &packed, handle, visible);
            fpv_plan.plan_skip_n = Some(posed_n.saturating_sub(admitted_n) as u32);
            gaps.clear(RenderGap::FpvViewmodel);
            status.0 = Some(FpvState::Drawn {
                idle_sampled: frame.idle_sampled,
            });
            for mut lens_tf in &mut lenses {
                *lens_tf = Transform::from_matrix(frame.lens);
            }
        }
    }
}

pub fn stamp_fpv_placement_matrix(
    mut fpv_plan: ResMut<crate::FpvDrawPlan>,
    cameras: Query<&Transform, (With<FlyCamera>, Without<FpvPlacementRoot>)>,
    roots: Query<&Transform, With<FpvPlacementRoot>>,
) {
    let (Ok(cam), Ok(local)) = (cameras.single(), roots.single()) else {
        fpv_plan.visible = false;
        return;
    };
    fpv_plan.world_from_local = cam.to_matrix() * local.to_matrix();
}

pub fn publish_fpv_dobj_pose(
    fpv_plan: Res<crate::FpvDrawPlan>,
    roots: Query<(), With<FpvPlacementRoot>>,
    mut bolts: ResMut<FpvBoltTargets>,
    mut dobj_poses: ResMut<crate::anim::dobj_pose::HostDObjPoseFrame>,
) {
    if roots.single().is_err() {
        bolts.clear();
        return;
    }
    for hand in 0..2usize {
        let Some(frame) = bolts.pose[hand].take() else {
            continue;
        };
        let dobj = fx_iw4::FX_BOLT_VIEWMODEL_DOBJ_BASE + hand as u32;

        if dobj_poses
            .publish(dobj, true, 0, fpv_plan.world_from_local, &frame.bones)
            .is_err()
        {
            continue;
        }
        let target = |bone: Option<u16>| -> Option<fx::FxBoltTarget> {
            let bone = bone?;
            let orientation = dobj_poses.resolve(dobj, i32::from(bone)).ok()?;
            Some(fx::FxBoltTarget {
                dobj,
                bone,
                centity_teleport: false,
                orientation,
            })
        };

        bolts.flash[hand] = target(frame.tags.flash);
        bolts.brass[hand] = target(frame.tags.brass);
        bolts.knife[hand] = target(frame.tags.knife);
        bolts.laser[hand] = target(frame.tags.laser);
    }
}

#[allow(clippy::too_many_arguments)]
pub fn apply_fpv_placement(
    clock: Res<CgFrameClock>,
    presented: Res<PresentedSnapshot>,
    local: Res<LocalPresentClient>,
    weapons: Option<Res<PreparedWeapons>>,
    mut kick: ResMut<SessionViewKick>,
    cg_gun: Res<CgGunOffset>,
    mut aim: ResMut<CgViewweaponAim>,
    windows: Query<&Window, With<bevy::window::PrimaryWindow>>,
    mut roots: Query<
        &mut Transform,
        (
            With<FpvPlacementRoot>,
            Without<RemotePlayer>,
            Without<WorldScriptModelInstance>,
        ),
    >,
    view: Res<ViewSubject>,
    mut gfx_scene: ResMut<HostGfxScene>,
) {
    *aim = CgViewweaponAim::default();
    let Ok(mut transform) = roots.single_mut() else {
        return;
    };
    let Some(ps) = presented.player(local.0) else {
        return;
    };
    if presented_is_third_person(&presented, local.0, view.in_killcam()) {
        return;
    }
    let Some(reg) = weapons.as_ref() else {
        return;
    };
    let viewmodel = bg_get_viewmodel_weapon_index(ps);
    let Some(facts) = reg.0.facts_of(viewmodel) else {
        return;
    };

    let mut state = WeaponPlacementState {
        sway_springs: kick.sway.springs(),
        gun_recoil: GunRecoilPlacementState {
            pitch_offset: kick.state.gun_angles[0],
            pitch_speed: kick.state.gun_speed[0],
            yaw_offset: kick.state.gun_angles[1],
            yaw_speed: kick.state.gun_speed[1],
        },
        movement_origin: kick.placement_move_origin,
        movement_angles: kick.placement_move_angles,
        weap_idle_time: kick.weap_idle_time,
        last_idle_factor: kick.last_idle_factor,
        damage_kick_time: clock.time(),
        damage_time: kick.damage_time,
        v_dmg_pitch: kick.v_dmg_pitch,
        v_dmg_roll: kick.v_dmg_roll,
        ..Default::default()
    };

    let overlay_reticle = if facts.overlay_reticle != 0 {
        facts.overlay_reticle
    } else if reg.0.overlay_is_hud_iris(viewmodel) {
        1
    } else {
        0
    };
    let ps_in = WeaponPlacementPsInputs {
        e_flags: ps.e_flags,
        weapon_pos_frac: ps.f_weapon_pos_frac,
        weapon_time: ps.weapon_time,
        aim_down_sight: facts.aim_down_sight,
        overlay_reticle,
        weapon_transition_active: false,

        lean_fraction: 0.0,
    };
    let stance = WeaponStanceStaticOfsInputs {
        ofs_at_0x168: facts.stance_ofs_at_0x168,
        ofs_at_0x18c: facts.stance_ofs_at_0x18c,
        ads_aim_pitch: facts.ads_aim_pitch,
        night_vision_wear_time: facts.night_vision_wear_time,
    };
    let bob_inputs = WeaponBobInputs {
        ads_bob_factor_at_0x330: facts.ads_bob_factor_at_0x330,
    };
    let xyspeed = {
        let vx = ps.velocity[0];
        let vy = ps.velocity[1];
        vec3_length([vx, vy, 0.0])
    };
    let kinematics = WeaponMovementKinematics {
        xyspeed,
        speed: ps.speed as f32,
        velocity: ps.velocity,
        viewangles: ps.viewangles,
        weaponstate: ps.weaponstate_primary,
        weaponstate_secondary: ps.weaponstate_secondary,
        pm_flags: ps.pm_flags,
        frametime: clock.frametime_secs(),
    };
    let waveform = bg_calculate_weapon_movement_bob_waveform(WeaponBobWaveformInputs {
        bob_cycle: (ps.bob_cycle as u32 & 0xff) as u8,
        xyspeed,
        view_height_target: ps.view_height_target,
        pm_flags: ps.pm_flags,
        weapon_pos_frac: ps.f_weapon_pos_frac,
    });
    let hip = GunKickSpring {
        accel: facts.kick.hip_gun_kick_accel,
        speed_max: facts.kick.hip_gun_kick_speed_max,
        speed_decay: facts.kick.hip_gun_kick_speed_decay,
        static_decay: facts.kick.hip_gun_kick_static_decay,
    };
    let ads = GunKickSpring {
        accel: facts.kick.ads_gun_kick_accel,
        speed_max: facts.kick.ads_gun_kick_speed_max,
        speed_decay: facts.kick.ads_gun_kick_speed_decay,
        static_decay: facts.kick.ads_gun_kick_static_decay,
    };
    let mut steps = [WeaponPlacementAssembleStep::Sway; PLACEMENT_ASSEMBLE_STEP_COUNT];

    let contrib = weapon_placement_assemble(
        &mut state,
        ps_in,
        stance,
        StanceTransitionFadeGlobals::default(),
        facts.movement,
        kinematics,
        bob_inputs,
        facts.idle,
        Some(waveform),
        hip,
        ads,
        facts.kick.gun_max_pitch,
        facts.kick.gun_max_yaw,
        0.0,
        &mut steps,
    );
    kick.placement_move_origin = state.movement_origin;
    kick.placement_move_angles = state.movement_angles;
    kick.weap_idle_time = state.weap_idle_time;
    kick.last_idle_factor = state.last_idle_factor;
    let mut origin = apply_viewweapon_land_view(
        apply_cg_gun_offset_view(contrib.origin, cg_gun.xyz()),
        kick.viewweapon_land_view,
    );
    if ps.last_weapon_hand == 1 {
        let add = dual_wield_view_model_origin_add(
            0,
            [0.0, 1.0, 0.0],
            facts.dual_wield_view_model_offset,
        );
        origin[0] += add[0];
        origin[1] += add[1];
        origin[2] += add[2];
    }
    let from_axis = viewweapon_iron_ads_saves_composed_axis(
        facts.aim_down_sight,
        ps.f_weapon_pos_frac,
        overlay_reticle,
    );
    let [gun_pitch, gun_yaw] = viewweapon_save_gun_pitch_yaw(
        contrib.angles,
        kick.refdef_view_angles,
        facts.aim_down_sight,
        ps.f_weapon_pos_frac,
        overlay_reticle,
    );
    let xhair = if kick.horiz_fov_deg > 0.0 {
        if let Ok(window) = windows.single() {
            let height = window.height().max(1.0);
            let aspect = window.width() / height;
            let (tan_x, tan_y) = cg_tan_half_fov(kick.horiz_fov_deg, aspect);
            let (vf, vr, vu) = angle_vectors(kick.refdef_view_angles);
            cg_calc_crosshair_position(
                gun_pitch,
                gun_yaw,
                kick.refdef_view_angles[2],
                vf,
                vr,
                vu,
                tan_x,
                tan_y,
            )
        } else {
            [0.0, 0.0]
        }
    } else {
        [0.0, 0.0]
    };
    *aim = CgViewweaponAim {
        live: true,
        gun_pitch,
        gun_yaw,
        xhair_x: xhair[0],
        xhair_y: xhair[1],
        from_composed_axis: from_axis,
    };
    let placed = iw_view_placement_to_bevy_camera_local(origin, contrib.angles);
    let world_delta = viewweapon_view_to_world_delta(origin, kick.refdef_view_angles);
    let pose_origin = [
        kick.refdef_vieworg[0] + world_delta[0],
        kick.refdef_vieworg[1] + world_delta[1],
        kick.refdef_vieworg[2] + world_delta[2],
    ];
    let pose_quat = scene_quat_from_viewmodel_axes(contrib.angles, kick.refdef_view_angles);
    gfx_scene
        .scene
        .store_pose_origin_quat(SCENE_VIEWMODEL_ENTNUM, pose_origin, Some(pose_quat));
    if ps.last_weapon_hand == 1 {
        gfx_scene.scene.store_pose_origin_quat(
            SCENE_VIEWMODEL_LEFT_ENTNUM,
            pose_origin,
            Some(pose_quat),
        );
    }
    *transform = placed;
}

fn leftover_scope_surf_is_lens(name: &str) -> bool {
    name.contains("lens")
}

fn fpv_spawn_queued(pending: Res<PendingFpvSpawn>) -> bool {
    pending.0.is_some()
}

fn flush_fpv_spawn(world: &mut World) {
    world.flush();
}

fn publish_fpv_notetracks(
    pending: Res<PendingFpvNotetracks>,
    mut notes: MessageWriter<audio::ViewmodelNotetracks>,
) {
    if pending.weapon == 0 || pending.names.is_empty() {
        return;
    }
    notes.write(audio::ViewmodelNotetracks {
        weapon: pending.weapon,
        names: pending.names.clone(),
    });
}

pub fn register_fpv_present_systems(app: &mut App) {
    app.init_resource::<LocalSpawnArmed>()
        .init_resource::<SessionViewmodel>()
        .init_resource::<SessionViewKick>()
        .init_resource::<CgGunOffset>()
        .init_resource::<CgViewweaponAim>()
        .init_resource::<PendingViewHurt>()
        .init_resource::<FpvStatusGap>()
        .init_resource::<RenderPresentationGaps>()
        .add_systems(
            Update,
            reset_view_kick_on_life_started.in_set(LifeFrontPublished),
        )
        .add_systems(
            Update,
            occupy_fpv_scene
                .after(PresentedPublished)
                .in_set(render_scene::GfxSceneAdd)
                .in_set(AnimSceneSubmit),
        )
        .add_systems(
            Update,
            (
                tick_session_view_kick.after(reset_view_kick_on_life_started),
                sync_camera_from_presented.after(tick_session_view_kick),
                spawn_pending_fpv
                    .run_if(fpv_spawn_queued)
                    .after(sync_camera_from_presented),
                flush_fpv_spawn.after(spawn_pending_fpv),
                tick_fpv_viewmodel.after(flush_fpv_spawn),
                commit_fpv_draw_plan.after(tick_fpv_viewmodel),
                publish_fpv_notetracks.after(tick_fpv_viewmodel),
                apply_fpv_placement
                    .after(tick_fpv_viewmodel)
                    .before(WorkerCmdSet::CellSceneEnt),
                stamp_fpv_placement_matrix
                    .after(apply_fpv_placement)
                    .after(commit_fpv_draw_plan)
                    .in_set(FpvPlacementSet),
                publish_fpv_dobj_pose
                    .after(stamp_fpv_placement_matrix)
                    .after(crate::anim::dobj_pose::begin_dobj_pose_frame),
            )
                .in_set(ClientSet::Present),
        );
}
