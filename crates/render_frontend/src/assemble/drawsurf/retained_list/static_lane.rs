use super::bmodel_owner_for_surf;
use super::common::{
    push_direct_lane_item, retained_draw_order_tie, with_catalog, world_surface_rank,
};
use super::smodel::{
    SmodelBucketEmitQueues, SmodelBucketEmitSource, SmodelDestinationPass, SmodelDestinationWriter,
    SmodelPretessBuilder, packed_lighting_dword_nonzero, push_smodel_surf_bucket,
    smodel_cache_index_u16, smodel_lod_is_rigid, smodel_lod_smc_flag, smodel_vis_skips_slot,
};
use crate::assemble::drawsurf::list::DrawSurfList;
use crate::assemble::drawsurf::tess::smodel::{
    LodRampArgs, SmodelGpuPlan, SmodelPlacement, smodel_camera_lod,
};
use crate::prepare::scene::cull::{DpvsFrameStats, WorldEyes, smodel_cull_dist_skips_slot};
use crate::prepare::scene::smodel_geom_cache::{
    LodRampDvar, PretessDvar, SmcEnableDvar, WorldStaticModelCache,
};
use crate::prepare::scene::smodel_lighting::WorldSmodelLighting;
use crate::prepare::scene::view_parms::PreparedSceneView;
use crate::prepare::scene::world::WorldScene;
use bevy::prelude::*;
use render_frame::{BspCameraLane, RetainedDrawItem, RetainedDrawKind};
use std::sync::Arc;
use std::time::Instant;

type StaticDrawSlices<'a> = (
    &'a [RetainedDrawItem],
    &'a [RetainedDrawItem],
    &'a [RetainedDrawItem],
    &'a [RetainedDrawItem],
    &'a [u16],
);

const fn bsp_lane(kind: asset_world::CameraRangeKind) -> BspCameraLane {
    match kind {
        asset_world::CameraRangeKind::LitOpaque => BspCameraLane::LitOpaque,
        asset_world::CameraRangeKind::LitTrans => BspCameraLane::LitTrans,
        asset_world::CameraRangeKind::Decal => BspCameraLane::Decal,
        asset_world::CameraRangeKind::Emissive => BspCameraLane::Emissive,
    }
}

#[derive(Clone, Copy, Debug, Default)]
pub struct RetainedRebuildCensus {
    pub world_n: u32,
    pub smodel_n: u32,

    pub smodel_hidden_n: u32,

    pub smodel_query_n: u32,

    pub smodel_vis_n: u32,

    pub smodel_vis_ready: u8,
    pub sort_us: u32,

    pub smodel_probe59_n: u32,

    pub smodel_miss59_n: u32,

    pub rebuild_skip: u8,

    pub lod_hold: u8,

    pub world_run_n: u32,

    pub smodel_bucket_flush_n: u32,
    pub smodel_bucket_rigid_n: u32,
    pub smodel_bucket_skinned_n: u32,
    pub smodel_bucket_cached_n: u32,
    pub smodel_bucket_unread_n: u32,

    pub smodel_bucket_consume_n: u32,

    pub smodel_bucket_context_refused_n: u32,
}

impl RetainedRebuildCensus {
    fn note_smodel_bucket(&mut self, bucket: Option<i32>, full: bool) {
        let Some(bucket) = bucket else {
            self.smodel_bucket_unread_n = self.smodel_bucket_unread_n.saturating_add(1);
            return;
        };
        match bucket & 3 {
            lighting_iw4::SMODEL_BUCKET_CACHED => {
                self.smodel_bucket_cached_n = self.smodel_bucket_cached_n.saturating_add(1);
            }
            lighting_iw4::SMODEL_BUCKET_SKINNED => {
                self.smodel_bucket_skinned_n = self.smodel_bucket_skinned_n.saturating_add(1);
            }
            _ => {
                self.smodel_bucket_rigid_n = self.smodel_bucket_rigid_n.saturating_add(1);
            }
        }
        if full {
            self.smodel_bucket_flush_n = self.smodel_bucket_flush_n.saturating_add(1);
        }
    }
}

#[derive(Resource, Clone, Debug, Default)]
pub struct StaticDrawLane {
    pub(crate) generation_id: crate::assemble::drawsurf::MaterialGenerationId,

    logged_buckets: Option<[u32; 5]>,

    pub(crate) colour: Vec<RetainedDrawItem>,
    pub(crate) emissive: Vec<RetainedDrawItem>,
    pub(crate) distortion: Vec<RetainedDrawItem>,
    pub(crate) census: RetainedRebuildCensus,

    last_static: Option<(u64, u64, render_scene::LodViewKey)>,

    static_items: Vec<RetainedDrawItem>,

    world_items: Vec<RetainedDrawItem>,

    smodel_items: Vec<RetainedDrawItem>,

    last_smodel_picks: Vec<SmodelLodPick>,

    pub(crate) world_run_surfs: Vec<u16>,

    pub(crate) membership_revision: u64,

    smodel_surf_lists: lighting_iw4::SmodelSurfBucketLists,

    pub(crate) smodel_pretess_indices: Arc<Vec<u16>>,

    pub(crate) smodel_index_layout_revision: u64,
}

impl StaticDrawLane {
    pub(crate) fn draws(&self) -> StaticDrawSlices<'_> {
        (
            &self.static_items,
            &self.colour,
            &self.emissive,
            &self.distortion,
            &self.world_run_surfs,
        )
    }
}

fn static_list_reusable(
    last: Option<&(u64, u64, render_scene::LodViewKey)>,
    world_id: u64,
    vis_id: u64,
    eye_key: &render_scene::LodViewKey,
    generation_hold: bool,
    has_static: bool,
) -> bool {
    generation_hold
        && has_static
        && last
            .is_some_and(|(world, vis, eye)| *world == world_id && *vis == vis_id && eye == eye_key)
}

fn world_static_reusable(last_world_id: Option<u64>, world_id: u64, generation_hold: bool) -> bool {
    generation_hold && last_world_id == Some(world_id)
}

fn smodel_descriptors_reusable(last: Option<&[SmodelLodPick]>, next: &[SmodelLodPick]) -> bool {
    last.is_some_and(|last| last == next)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct SmodelLodPick {
    placement: u32,
    mesh: u32,
    lod: u8,
    cache_index: u16,
    lighting_handle: u32,
    packed_lighting: Option<[u8; 4]>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
struct SmodelLodWalk {
    picks: Vec<SmodelLodPick>,
    hidden_n: u32,
    query_n: u32,
}

fn note_smodel_vis(census: &mut RetainedRebuildCensus, smodel_vis: &[u8]) {
    if !smodel_vis.is_empty() {
        census.smodel_vis_ready = 1;
        census.smodel_vis_n = smodel_vis.iter().filter(|byte| **byte != 0).count() as u32;
    }
}

fn collect_smodel_lod_picks(
    plan: Option<&SmodelGpuPlan>,
    smodel_vis: &[u8],
    cull_dists: &[u16],
    eyes: &WorldEyes,
    lod_args: &LodRampArgs,
    lighting: Option<&WorldSmodelLighting>,
    smc_cache: Option<&WorldStaticModelCache>,
) -> SmodelLodWalk {
    let Some(plan) = plan else {
        return SmodelLodWalk::default();
    };
    let mut walk = SmodelLodWalk::default();
    for (placement_i, placement) in plan.placements.iter().enumerate() {
        let vis_slot = placement.lighting_slot.unwrap_or(placement_i);
        if smodel_vis_skips_slot(smodel_vis, vis_slot) {
            walk.hidden_n = walk.hidden_n.saturating_add(1);
            continue;
        }
        if smodel_cull_dist_skips_slot(
            cull_dists,
            vis_slot,
            placement.origin,
            eyes.at(placement.origin),
            lod_args.scale_last,
        ) {
            walk.hidden_n = walk.hidden_n.saturating_add(1);
            continue;
        }
        walk.query_n = walk.query_n.saturating_add(1);
        if let Some(pick) = smodel_descriptor_pick(
            placement_i,
            placement,
            plan,
            lighting,
            smc_cache,
            eyes.at(placement.origin),
            lod_args,
        ) {
            walk.picks.push(pick);
        }
    }
    walk
}

fn smodel_descriptor_pick(
    placement_i: usize,
    placement: &SmodelPlacement,
    plan: &SmodelGpuPlan,
    lighting: Option<&WorldSmodelLighting>,
    smc_cache: Option<&WorldStaticModelCache>,
    eye: Option<Vec3>,
    lod_args: &LodRampArgs,
) -> Option<SmodelLodPick> {
    let handle = if placement.lit {
        let slot = placement.lighting_slot?;
        let h = lighting
            .and_then(|l| l.handles.get(slot).copied())
            .unwrap_or(0);
        if h == 0 {
            return None;
        }
        u32::from(h)
    } else {
        0
    };
    let packed_lighting = placement.packed_lighting.or_else(|| {
        placement
            .lighting_slot
            .and_then(|slot| lighting.and_then(|l| l.packed_lighting_for_slot(slot)))
    });
    let mesh = plan.meshes.get(placement.mesh)?;
    let lod = smodel_camera_lod(mesh.lod, placement.origin, placement.scale, eye, lod_args)?;
    let lod_surfs = mesh
        .surfaces_by_lod
        .get(usize::from(lod))
        .map(|v| v.as_slice())
        .unwrap_or(&[]);
    if lod_surfs.is_empty() {
        return None;
    }
    let vis_slot = placement.lighting_slot.unwrap_or(placement_i);
    let _smodel_index = u16::try_from(vis_slot).ok()?;
    let lod_i = usize::from(lod);
    Some(SmodelLodPick {
        placement: placement_i as u32,
        mesh: placement.mesh as u32,
        lod,
        cache_index: smodel_cache_index_u16(smc_cache, placement.lighting_slot, lod_i),
        lighting_handle: handle,
        packed_lighting,
    })
}

fn materialize_world_runs(items: &mut Vec<RetainedDrawItem>, table: &mut Vec<u16>) -> u32 {
    table.clear();
    let mut run_n = 0u32;
    for item in items {
        let RetainedDrawKind::World {
            surf,
            run,
            world_from_local,
            bsp_kind,
            bsp_run_first,
            setup_key_changed,
            brush_model,
            ..
        } = item.kind
        else {
            continue;
        };
        let run = run.max(1);
        let run_off = table.len() as u32;
        for offset in 0..run {
            table.push(
                surf.checked_add(offset)
                    .unwrap_or_else(|| panic!("world draw run exceeds the u16 surface domain")),
            );
        }
        item.kind = RetainedDrawKind::World {
            surf,
            run,
            run_off,
            bsp_kind,
            bsp_run_first,
            setup_key_changed,
            world_from_local,
            brush_model,
        };
        run_n = run_n.saturating_add(1);
    }
    run_n
}

fn emit_world_static_lane(
    list: &mut StaticDrawLane,
    world_list: &DrawSurfList,
    world_plan: Option<&crate::assemble::drawsurf::WorldDrawGpuPlan>,
    scene: Option<&WorldScene>,
    catalog: &crate::assemble::drawsurf::material_runtime::RuntimeMaterialCatalog,
) {
    for item in &world_list.items {
        let surface_samplers = world_plan
            .and_then(|plan| plan.surface_sampler_inputs.get(usize::from(item.surf)))
            .copied()
            .unwrap_or_else(render_frame::SurfaceSamplerInputs::default);
        let brush_owner = scene
            .and_then(|scene| scene.cull.as_ref())
            .and_then(|cull| {
                bmodel_owner_for_surf(item.surf, &cull.brush_models, &cull.bmodel_world_from_local)
            });
        let kind = match item.kind {
            crate::prepare::scene::world::WorldDrawItemKind::Bsp(kind) => {
                RetainedDrawKind::bsp_world(
                    item.surf,
                    item.run,
                    bsp_lane(kind),
                    item.surf,
                    item.setup_key_changed,
                )
            }
            crate::prepare::scene::world::WorldDrawItemKind::BModel => {
                let Some((model, pose)) = brush_owner else {
                    continue;
                };
                RetainedDrawKind::brush_world(item.surf, model, pose)
            }
        };

        let world_rank = world_surface_rank(
            scene.and_then(|scene| scene.cull.as_ref()),
            usize::from(item.surf),
            item.key,
            catalog,
        );
        list.world_items.push(with_catalog(
            item.key,
            world_rank,
            kind,
            surface_samplers,
            catalog,
        ));
        list.census.world_n = list.census.world_n.saturating_add(u32::from(item.run));
    }
}

fn emit_smodel_static_lane(
    list: &mut StaticDrawLane,
    plan: &SmodelGpuPlan,
    walk: &SmodelLodWalk,
    smc_cache: Option<&WorldStaticModelCache>,
    smc_on: bool,
    pretess_enabled: bool,
    catalog: &crate::assemble::drawsurf::material_runtime::RuntimeMaterialCatalog,
) {
    let mut queues = SmodelBucketEmitQueues::default();
    let mut destinations = Vec::new();
    let mut pretess = SmodelPretessBuilder::new(pretess_enabled);
    list.census.smodel_hidden_n = walk.hidden_n;
    list.census.smodel_query_n = walk.query_n;
    list.last_smodel_picks.clone_from(&walk.picks);
    for pick in &walk.picks {
        let placement_i = pick.placement as usize;
        let Some(placement) = plan.placements.get(placement_i) else {
            continue;
        };
        let vis_slot = placement.lighting_slot.unwrap_or(placement_i);
        let world_from_local = placement.world_from_local;
        let handle = pick.lighting_handle;
        let packed_lighting = pick.packed_lighting;
        let Some(mesh) = plan.meshes.get(placement.mesh) else {
            continue;
        };
        let lod = pick.lod;
        let Some(smodel_index) = u16::try_from(vis_slot).ok() else {
            list.census.smodel_bucket_context_refused_n = list
                .census
                .smodel_bucket_context_refused_n
                .saturating_add(1);
            continue;
        };
        let lod_i = usize::from(lod);
        let cache_index = pick.cache_index;
        let source = SmodelBucketEmitSource {
            placement: placement_i as u32,
            lod,
            payload: 0,
            world_from_local,
            lighting_handle: handle,
            packed_lighting,
            pass: SmodelDestinationPass::Colour,
        };
        let lighting_nonzero = packed_lighting_dword_nonzero(packed_lighting);
        let lod_smc_flag = smodel_lod_smc_flag(mesh, lod_i);
        let selected_bucket = smodel_lod_is_rigid(mesh, lod_i).map(|lod_is_rigid| {
            lighting_iw4::add_static_model_surf_to_bucket(
                i32::from(lod),
                smc_on,
                lighting_nonzero,
                lod_smc_flag,
                cache_index,
                lod_is_rigid,
            )
        });
        let (bucket, full) = push_smodel_surf_bucket(
            &mut list.smodel_surf_lists,
            &mut queues,
            selected_bucket,
            cache_index,
            smodel_index,
            source,
        );
        list.census.note_smodel_bucket(bucket, full);
        if bucket.is_none() {
            SmodelDestinationWriter {
                smodel_plan: plan,
                catalog,
                out: &mut destinations,
            }
            .expand(source, None, None);
        } else if full
            && let Some(mask) = bucket
                .and_then(|bucket| u8::try_from(bucket).ok())
                .and_then(lighting_iw4::smodel_bucket_mask)
        {
            let consumed = SmodelDestinationWriter {
                smodel_plan: plan,
                catalog,
                out: &mut destinations,
            }
            .consume_buckets(
                &mut list.smodel_surf_lists,
                &mut queues,
                mask,
                smc_cache,
                &mut pretess,
                None,
            );
            list.census.smodel_bucket_consume_n = list
                .census
                .smodel_bucket_consume_n
                .saturating_add(consumed.buckets);
            list.census.smodel_bucket_context_refused_n = list
                .census
                .smodel_bucket_context_refused_n
                .saturating_add(consumed.context_refused);
        }
    }
    if list.census.smodel_bucket_unread_n > 0 {
        diag::warn!(
            World,
            "smodel arm unknown: {} of {} admitted placements carry no XSurface+1 byte (rigid/skinned undecidable) — dropped, not drawn",
            list.census.smodel_bucket_unread_n,
            list.census.smodel_query_n
        );
    }
    let consumed = SmodelDestinationWriter {
        smodel_plan: plan,
        catalog,
        out: &mut destinations,
    }
    .consume_tail(
        &mut list.smodel_surf_lists,
        &mut queues,
        smc_cache,
        &mut pretess,
        None,
    );
    list.census.smodel_bucket_consume_n = list
        .census
        .smodel_bucket_consume_n
        .saturating_add(consumed.buckets);
    list.census.smodel_bucket_context_refused_n = list
        .census
        .smodel_bucket_context_refused_n
        .saturating_add(consumed.context_refused);
    let buckets = [
        list.census.smodel_bucket_rigid_n,
        list.census.smodel_bucket_skinned_n,
        list.census.smodel_bucket_cached_n,
        list.census.smodel_bucket_unread_n,
        list.census.smodel_bucket_consume_n,
    ];
    if list.logged_buckets != Some(buckets) {
        list.logged_buckets = Some(buckets);
        let [rigid, skinned, cached, unread, consume] = buckets;
        diag::info!(
            World,
            "smodel buckets: rigid={rigid} skinned={skinned} cached={cached} unread={unread} consume={consume}",
        );
    }
    if list.smodel_pretess_indices.as_slice() != pretess.indices() {
        list.smodel_index_layout_revision = list.smodel_index_layout_revision.wrapping_add(1);
        list.smodel_pretess_indices = Arc::new(pretess.finish());
    }
    for destination in destinations {
        list.census.smodel_n = list.census.smodel_n.saturating_add(1);
        if destination.packed_lighting.is_some() {
            list.census.smodel_probe59_n = list.census.smodel_probe59_n.saturating_add(1);
        } else {
            list.census.smodel_miss59_n = list.census.smodel_miss59_n.saturating_add(1);
        }
        list.smodel_items.push(destination.into_item(catalog));
    }
}

fn compose_static_lanes(list: &mut StaticDrawLane) {
    list.static_items.clear();
    list.static_items.extend_from_slice(&list.world_items);
    list.static_items.extend_from_slice(&list.smodel_items);
    let sort_started = Instant::now();
    list.static_items
        .sort_unstable_by_key(|i| (i.host_sort_key(), retained_draw_order_tie(&i.kind)));
    list.census.world_run_n =
        materialize_world_runs(&mut list.static_items, &mut list.world_run_surfs);
    let mut colour = std::mem::take(&mut list.colour);
    let mut emissive = std::mem::take(&mut list.emissive);
    let mut distortion = std::mem::take(&mut list.distortion);
    colour.clear();
    emissive.clear();
    distortion.clear();
    for &item in &list.static_items {
        let distortion_out =
            matches!(item.kind, RetainedDrawKind::World { .. }).then_some(&mut distortion);
        push_direct_lane_item(&mut colour, &mut emissive, distortion_out, item);
    }
    list.colour = colour;
    list.emissive = emissive;
    list.distortion = distortion;
    list.census.sort_us = sort_started.elapsed().as_micros() as u32;
    list.membership_revision = list.membership_revision.wrapping_add(1);
}

fn apply_lod_hold_census(list: &mut StaticDrawLane, walk: SmodelLodWalk, smodel_vis: &[u8]) {
    list.census.rebuild_skip = 0;
    list.census.lod_hold = 1;
    list.census.smodel_query_n = walk.query_n;
    list.census.smodel_hidden_n = walk.hidden_n;
    list.census.sort_us = 0;
    note_smodel_vis(&mut list.census, smodel_vis);
    list.last_smodel_picks = walk.picks;
}

fn restore_held_smodel_census(dst: &mut RetainedRebuildCensus, src: &RetainedRebuildCensus) {
    dst.smodel_n = src.smodel_n;
    dst.smodel_probe59_n = src.smodel_probe59_n;
    dst.smodel_miss59_n = src.smodel_miss59_n;
    dst.smodel_bucket_flush_n = src.smodel_bucket_flush_n;
    dst.smodel_bucket_rigid_n = src.smodel_bucket_rigid_n;
    dst.smodel_bucket_skinned_n = src.smodel_bucket_skinned_n;
    dst.smodel_bucket_cached_n = src.smodel_bucket_cached_n;
    dst.smodel_bucket_unread_n = src.smodel_bucket_unread_n;
    dst.smodel_bucket_consume_n = src.smodel_bucket_consume_n;
    dst.smodel_bucket_context_refused_n = src.smodel_bucket_context_refused_n;
}

#[expect(
    clippy::type_complexity,
    reason = "Bevy system parameters keep scheduler resource access explicit."
)]
pub(crate) fn rebuild_static_draw_lane(
    mut list: ResMut<StaticDrawLane>,
    world_list: Res<DrawSurfList>,
    world_geom: (
        Option<Res<crate::assemble::drawsurf::WorldDrawGpuPlan>>,
        Option<Res<WorldScene>>,
    ),
    smodel: (
        Option<Res<SmodelGpuPlan>>,
        Option<Res<WorldSmodelLighting>>,
        Option<Res<DpvsFrameStats>>,
        Option<Res<WorldStaticModelCache>>,
        Res<SmcEnableDvar>,
        Res<PretessDvar>,
    ),
    prepared: Option<Res<PreparedSceneView>>,
    lod_ramp: Res<LodRampDvar>,
    runtime: Res<crate::assemble::drawsurf::MaterialGeneration>,
) {
    let _post_rebuild = perf::span!("post_rebuild", "iw4l.render");
    let (world_plan, scene) = world_geom;
    let (smodel_plan, lighting, dpvs, smc_cache, smc_enable, pretess_dvar) = smodel;
    let smc_on = smc_enable.enabled != Some(false);
    let smodel_vis = dpvs
        .as_ref()
        .map(|stats| stats.smodel_vis.as_slice())
        .unwrap_or(&[]);
    let world_id = world_list.draw_items_id;
    let vis_id = dpvs.as_ref().map(|stats| stats.smodel_vis_id).unwrap_or(0);
    let eye = prepared.as_ref().filter(|v| v.ready).map(|v| v.eye);
    let lod_args = lod_ramp.args();
    let eye_key = lod_args.view_key(eye.map(|eye| eye.to_array()));
    let generation_hold = list.generation_id == runtime.catalog.generation_id();
    let reuse = static_list_reusable(
        list.last_static.as_ref(),
        world_id,
        vis_id,
        &eye_key,
        generation_hold,
        !list.static_items.is_empty(),
    );
    list.generation_id = runtime.catalog.generation_id();
    if reuse {
        list.census.rebuild_skip = 1;
        list.census.lod_hold = 0;
        list.census.smodel_query_n = 0;
        list.census.sort_us = 0;
        note_smodel_vis(&mut list.census, smodel_vis);
    } else {
        let world_hold = world_static_reusable(
            list.last_static.as_ref().map(|(world, _, _)| *world),
            world_id,
            generation_hold,
        );
        let cull_dists = scene
            .as_ref()
            .and_then(|scene| scene.cull.as_ref())
            .map(|cull| cull.static_model_cull_dists.as_slice())
            .unwrap_or(&[]);
        let smodel_walk = collect_smodel_lod_picks(
            smodel_plan.as_deref(),
            smodel_vis,
            cull_dists,
            &WorldEyes::new(scene.as_deref(), eye),
            &lod_args,
            lighting.as_deref(),
            smc_cache.as_deref(),
        );
        let smodel_hold = generation_hold
            && list.last_static.is_some()
            && smodel_descriptors_reusable(
                Some(list.last_smodel_picks.as_slice()),
                &smodel_walk.picks,
            );
        if world_hold && smodel_hold {
            apply_lod_hold_census(list.as_mut(), smodel_walk, smodel_vis);
        } else {
            let prev = list.census;
            if !world_hold {
                list.world_items.clear();
                list.world_run_surfs.clear();
            }
            if !smodel_hold {
                list.smodel_items.clear();
                list.smodel_surf_lists = lighting_iw4::SmodelSurfBucketLists::default();
                list.smodel_pretess_indices = Arc::new(Vec::new());
                list.smodel_index_layout_revision =
                    list.smodel_index_layout_revision.wrapping_add(1);
                list.last_smodel_picks.clear();
            }
            list.static_items.clear();
            list.colour.clear();
            list.emissive.clear();
            list.distortion.clear();
            list.census = RetainedRebuildCensus::default();
            note_smodel_vis(&mut list.census, smodel_vis);
            if world_hold {
                list.census.world_n = prev.world_n;
            }
            if smodel_hold {
                restore_held_smodel_census(&mut list.census, &prev);
                list.census.smodel_query_n = smodel_walk.query_n;
                list.census.smodel_hidden_n = smodel_walk.hidden_n;
                list.last_smodel_picks.clone_from(&smodel_walk.picks);
            }
            if !world_hold {
                emit_world_static_lane(
                    list.as_mut(),
                    &world_list,
                    world_plan.as_deref(),
                    scene.as_deref(),
                    &runtime.catalog,
                );
            }
            if !smodel_hold && let Some(plan) = smodel_plan.as_deref() {
                emit_smodel_static_lane(
                    list.as_mut(),
                    plan,
                    &smodel_walk,
                    smc_cache.as_deref(),
                    smc_on,
                    pretess_dvar.enabled,
                    &runtime.catalog,
                );
            }
            compose_static_lanes(list.as_mut());
        }
    }
    list.last_static = Some((world_id, vis_id, eye_key));
}

impl StaticDrawLane {
    pub fn smodel_pretess_indices(&self) -> &Arc<Vec<u16>> {
        &self.smodel_pretess_indices
    }

    pub fn smodel_index_layout_revision(&self) -> u64 {
        self.smodel_index_layout_revision
    }
}
