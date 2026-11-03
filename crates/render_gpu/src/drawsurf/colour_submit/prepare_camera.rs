use super::{
    ArenaPack, Buffer, MaterialGenerationId, PackDraw, PreparedExactDraw, RunPackCache, TableEpoch,
};
use bevy::tasks::{ComputeTaskPool, TaskPool};
use dpvs_iw4::GfxDrawSurf;
use render_backend::MaterialRunCensus;

use super::shadow_prepare::{ShadowLane, open_shadow_table_epoch, prepare_shadow_passes};
use super::{
    BTreeMap, COLOUR_PREPARE_LANES, Camera3d, CameraPrepareState, CameraWorldPretess,
    ColourPackPlan, ColourRowPlan, DrawRefusalCensus, ExactColourBindingCache, ExactColourGeometry,
    ExactColourPipeline, ExactColourSubmitCensus, ExactConstantArena, ExactFloatZResolve,
    ExactPipelineRegistry, ExactPrepare, ExactPrepareTarget, ExactShadowBindingCache,
    ExactTessBind, ExtractedColourRefs, ExtractedRenderFrameProducts, ExtractedView, FrameProduct,
    FrameProductKind, FrameProductStatus, GpuConstantArena, GpuSubmitRefusal, Mat4,
    MaterialExecView, MaterialRefusal, MaterialRunExecutor, Msaa, PipelineCache,
    PipelineWaitCensus, PrepareCost, PrepareTextureTables, PreparedColourRow, PublishedRenderFrame,
    Query, RenderDevice, RenderQueue, Res, ResMut, ResidentShadowStaticDraws, Resource,
    RetainedDrawKind, RuntimeShaderStage, RuntimeUploadedImageRegistry, SCENE_DEPTH_FORMAT,
    SamplerTable, SceneDepthTexture, SceneTextureState, SceneTextureTables, ShadowSubmitScratch,
    ShadowTextureTable, ShadowmapSpotArena, ShadowmapSpotGpu, ShadowmapSunArena, ShadowmapSunGpu,
    SmodelCacheGpu, SpecializedRenderPipelines, UnsupportedStateCensus, Vec3, Vec4, ViewTarget,
    With, WorldPretessLayout, as_u32, bind_world_packed_rows, bsp_draw_source, bsp_kind_index,
    build_colour_row_plan, colour_census_clock, colour_census_ms, colour_draw_at, colour_pack_key,
    colour_tech_at, draw_surf_list_work_colour, empty_world_run_gather, exec_tables, floatz,
    gather_world_run_indices, is_viewmodel_colour_draw, material_refusal_class,
    pack_sun_shadow_frontend, prepare_constant_bases, publish_this_frame_spot_shadow_views,
    publish_this_frame_sun_shadow_view, smodel_skinned, spot_rt_for_light,
    spot_shadow_view_missing, submit_refusal_class, submit_refusal_family, sun_shadow_view_missing,
    viewmodel_colour_submits_when_pipelines_ready, world_packed_row_meta, world_pretess_dest_ib,
    world_pretess_key,
};

fn run_colour_lanes(camera: CameraLane<'_>, shadow: ShadowLane<'_>) {
    let _prepare = perf::span!("colour_prepare", "iw4l.render");
    ComputeTaskPool::get_or_init(TaskPool::default).scope(|scope| {
        scope.spawn(async move { prepare_shadow_passes(shadow) });
        prepare_camera_colour(camera);
    });
}

pub(super) fn prepare_colour_lanes(
    views: Query<
        (
            &ViewTarget,
            &SceneDepthTexture,
            &ExtractedView,
            Option<&Msaa>,
        ),
        With<Camera3d>,
    >,
    (frame, installed, geometry, pipeline, registry, device, queue, uploaded): (
        Res<PublishedRenderFrame>,
        Res<InstalledColourPass>,
        Res<ExactColourGeometry>,
        Res<ExactColourPipeline>,
        Res<ExactPipelineRegistry>,
        Res<RenderDevice>,
        Res<RenderQueue>,
        Res<RuntimeUploadedImageRegistry>,
    ),
    (
        mut smodel_cache_gpu,
        mut colour_binding_cache,
        mut constant_arena,
        mut census,
        mut scratch,
        mut texture_table,
        mut cam,
        mut pretess,
    ): (
        ResMut<SmodelCacheGpu>,
        ResMut<ExactColourBindingCache>,
        ResMut<ExactConstantArena>,
        ResMut<ExactColourSubmitCensus>,
        ResMut<ColourSubmitScratch>,
        ResMut<SceneTextureTables>,
        ResMut<CameraPrepareState>,
        ResMut<CameraWorldPretess>,
    ),
    (
        mut shadow_binding_cache,
        mut shadow_table,
        mut shadow_arena,
        mut spot_arena,
        mut static_draws,
        mut shadowmap,
        mut spotmap,
        mut shadow_scratch,
    ): (
        ResMut<ExactShadowBindingCache>,
        ResMut<ShadowTextureTable>,
        ResMut<ShadowmapSunArena>,
        ResMut<ShadowmapSpotArena>,
        ResMut<ResidentShadowStaticDraws>,
        ResMut<ShadowmapSunGpu>,
        ResMut<ShadowmapSpotGpu>,
        ResMut<ShadowSubmitScratch>,
    ),
) {
    let mut cameras = views.iter();
    let view = cameras.next();
    let extra_views = cameras.count();
    run_colour_lanes(
        CameraLane {
            view,
            extra_views,
            frame: &frame,
            installed: &installed,
            geometry: &geometry,
            pipeline: &pipeline,
            registry: &registry,
            device: &device,
            queue: &queue,
            uploaded: &uploaded,
            smodel_cache_gpu: &mut smodel_cache_gpu,
            binding_cache: &mut colour_binding_cache,
            constant_arena: &mut constant_arena,
            census: &mut census,
            scratch: &mut scratch,
            texture_table: &mut texture_table,
            cam: &mut cam,
            pretess: &mut pretess,
        },
        ShadowLane {
            frame: &frame,
            installed_ready: installed.ready,
            geometry: &geometry,
            pipeline: &pipeline,
            registry: &registry,
            device: &device,
            queue: &queue,
            uploaded: &uploaded,
            binding_cache: &mut shadow_binding_cache,
            shadow_table: &mut shadow_table,
            shadow_arena: &mut shadow_arena,
            spot_arena: &mut spot_arena,
            static_draws: &mut static_draws,
            shadowmap: &mut shadowmap,
            spotmap: &mut spotmap,
            scratch: &mut shadow_scratch,
        },
    );
}

pub(super) struct CameraLane<'a> {
    pub view: Option<CameraTargetView<'a>>,
    pub extra_views: usize,
    pub frame: &'a PublishedRenderFrame,
    pub installed: &'a InstalledColourPass,
    pub geometry: &'a ExactColourGeometry,
    pub pipeline: &'a ExactColourPipeline,
    pub registry: &'a ExactPipelineRegistry,
    pub device: &'a RenderDevice,
    pub queue: &'a RenderQueue,
    pub uploaded: &'a RuntimeUploadedImageRegistry,
    pub smodel_cache_gpu: &'a mut SmodelCacheGpu,
    pub binding_cache: &'a mut ExactColourBindingCache,
    pub constant_arena: &'a mut ExactConstantArena,
    pub census: &'a mut ExactColourSubmitCensus,
    pub scratch: &'a mut ColourSubmitScratch,
    pub texture_table: &'a mut SceneTextureTables,
    pub cam: &'a mut CameraPrepareState,
    pub pretess: &'a mut CameraWorldPretess,
}

pub(super) fn prepare_camera_colour(lane: CameraLane<'_>) {
    let CameraLane {
        view,
        extra_views,
        frame,
        installed,
        geometry,
        pipeline,
        registry,
        device,
        queue,
        uploaded,
        smodel_cache_gpu,
        binding_cache,
        constant_arena,
        census,
        scratch,
        texture_table,
        cam,
        pretess,
    } = lane;
    let _span = perf::span!("prepare_camera", "iw4l.render");
    let extracted = ExtractedColourRefs::new(frame);
    let products = &extracted.frame.frame_products;
    *cam = CameraPrepareState::default();
    let census_on = perf::recording();
    census.begin_prepare(census_on);
    if !installed.ready {
        return;
    }
    let Some(sampler_table) = extracted.world.sampler_table.as_ref() else {
        return;
    };
    let sun_shadow_view_ready = installed.sun_shadow_view_ready;
    let colour = products.0.product(FrameProductKind::Colour);
    let light = products.0.product(FrameProductKind::Light);
    let emissive = products.0.product(FrameProductKind::Emissive);
    if colour.ordered_draws.is_empty()
        && light.ordered_draws.is_empty()
        && emissive.ordered_draws.is_empty()
    {
        return;
    }

    let Some((target, _depth, _extracted_view, msaa)) = view else {
        return;
    };
    let extra = extra_views;
    if extra > 0 {
        cam.last_refusal = Some(GpuSubmitRefusal::MultipleCameraViews {
            views: as_u32(extra.saturating_add(1)),
        });
        return;
    }
    let samples = msaa.map_or(1, Msaa::samples);
    let needs_floatz = installed.camera.needs_floatz;
    let needs_resolved_scene = installed.camera.needs_resolved_scene;

    let focused_object_id = products.0.focus().and_then(|focus| focus.object_id);

    let world_run_surfs = products.0.world_run_surfs.as_slice();
    let pretess_key = world_pretess_key(
        colour,
        light,
        emissive,
        world_run_surfs,
        geometry.static_identity.world_products,
        geometry.generation,
        products.0.world_run_revision,
    );
    let pack_key = colour_pack_key(
        colour,
        light,
        emissive,
        pretess_key,
        geometry.static_identity.smodel_index_count,
        geometry.xmodel.uploaded_topology,
        geometry.xmodel.index.len(),
    );
    let pack_plan_rebuilt = scratch
        .pack_plan
        .as_ref()
        .is_none_or(|plan| plan.key != pack_key);
    if pack_plan_rebuilt {
        let packed = pack_sun_shadow_frontend(
            colour
                .ordered_draws
                .iter()
                .chain(light.ordered_draws.iter())
                .chain(emissive.ordered_draws.iter()),
            world_run_surfs,
            &geometry.world_surface_ranges,
            geometry.static_identity.world_vertex_count as u32,
            &geometry.smodel_surface_ranges,
            &geometry.xmodel_surface_ranges,
            &mut scratch.pack_draws,
        );
        let work = draw_surf_list_work_colour(&packed);
        let skinned = (
            packed.smodel_skinned.len(),
            work.smodel_skinned_unconsumed as usize,
        );
        if scratch.logged_skinned != Some(skinned) {
            scratch.logged_skinned = Some(skinned);
            diag::info!(
                World,
                "smodel skinned: packed={} unconsumed={}",
                skinned.0,
                skinned.1,
            );
        }
        let world_rows = world_packed_row_meta(colour, light, emissive, &packed, world_run_surfs);
        scratch.pack_plan = Some(ColourPackPlan {
            key: pack_key,
            packed,
            work,
            world_rows,

            row_plan: Vec::new(),
        });
    }
    let mut pack_plan = scratch
        .pack_plan
        .take()
        .expect("colour pack plan is filled");
    if census_on {
        census.frame.end_depth_restore_n = Some(pack_plan.work.end_restore_n);
        census.frame.end_depth_range_type = Some(pack_plan.work.end_depth_range_type);
    }
    let gather_started = colour_census_clock(census_on);
    let world_ib_skip = pretess.layout.as_ref().is_some_and(|layout| {
        layout.key == pretess_key && (layout.index.is_some() || layout.logical_index_count == 0)
    });
    let gathered = if world_ib_skip {
        empty_world_run_gather(Vec::new(), {
            pretess
                .layout
                .as_ref()
                .map(|layout| layout.index_gaps)
                .unwrap_or(0)
        })
    } else {
        let gathered = gather_world_run_indices(
            bind_world_packed_rows(colour, light, emissive, &pack_plan.world_rows).filter_map(
                |row| {
                    let row = row?;
                    let surf = row.world_surf()?;
                    Some((surf, row.item.key, row.item.surface_samplers))
                },
            ),
            &geometry.world_cpu_indices,
            &geometry.world_surface_ranges,
        );
        let reuse = pretess.layout.take().and_then(|layout| layout.index);
        let index = world_pretess_dest_ib(device, queue, reuse, gathered.indices.as_slice());
        pretess.epoch = pretess.epoch.wrapping_add(1);
        pretess.layout = Some(WorldPretessLayout {
            key: pretess_key,
            index,
            ranges: gathered.ranges.clone(),
            index_gaps: gathered.index_gaps,
            logical_index_count: as_u32(gathered.indices.len()),
            epoch: pretess.epoch,
        });
        gathered
    };

    if pack_plan_rebuilt || !world_ib_skip {
        pack_plan.row_plan = build_colour_row_plan(
            colour,
            light,
            emissive,
            &pack_plan.packed,
            &pack_plan.work,
            world_run_surfs,
            pretess,
        );
    }
    if census_on {
        census.frame.world_index_gaps = Some(gathered.index_gaps);
        census.frame.world_run_indices_n = Some(pretess.logical_index_count());
        census.frame.world_ib_skip = Some(u32::from(world_ib_skip));
    }
    if census_on {
        census.frame.world_gathered = Some(u32::from(pretess.index().is_some()));
    }

    if census_on {
        census.frame.smodel_pretess_skip = Some(0);
        census.frame.smodel_pretess_runs = Some(0);
        census.frame.smodel_pretess_hits = Some(0);
        census.frame.smodel_pretess_verts = Some(0);
        census.frame.smodel_pretess_indices = Some(0);
        census.frame.smodel_cached_lighting = Some(0);
        census.frame.smodel_pretess_local = Some(0);
        census.frame.smodel_pretess_length1 = Some(0);
        census.frame.submit_gather_ms = colour_census_ms(gather_started);
    }
    let prepare_started = colour_census_clock(census_on);
    let exec_frame = &extracted.frame.exec_frame;
    let exec_view = exec_tables(extracted)
        .map(|(catalog, prepared)| MaterialExecView::camera(catalog, prepared, exec_frame));
    let prepare_target = ExactPrepareTarget {
        color: target.main_texture_format(),
        samples,
        depth: SCENE_DEPTH_FORMAT,
        forward_z: false,
        use_world_pretess: true,
    };
    let textures = std::sync::Mutex::new(SceneTextureState {
        slots: std::mem::take(&mut binding_cache.textures),
        tables: std::mem::take(&mut texture_table.0),
    });
    let skinned = std::sync::Mutex::new(std::mem::take(&mut scratch.skinned_tess));
    let input = CameraRowsInput {
        extracted,
        products,
        colour,
        light,
        emissive,
        geometry,
        pretess: &*pretess,
        pipeline,
        registry,
        device,
        queue,
        uploaded,
        sampler_table,
        textures: &textures,
        skinned: &skinned,
        exec_view,
        prepare_target,
        glass_cull: view_cull_planes(exec_frame.clip_from_world),
        census_on,
        sun_shadow_view_ready,
    };
    let rows = pack_plan.row_plan.as_slice();
    let share = rows.len().div_ceil(COLOUR_PREPARE_LANES).max(1);
    let mut tallies: [Option<CameraRowTally>; COLOUR_PREPARE_LANES] = Default::default();
    ComputeTaskPool::get_or_init(TaskPool::default).scope(|scope| {
        let mut work = scratch
            .lanes
            .iter_mut()
            .zip(constant_arena.gpu.iter_mut())
            .zip(tallies.iter_mut())
            .enumerate();
        let first = work.next();
        for (index, ((lane, gpu), tally)) in work {
            let input = &input;
            let rows = lane_rows(rows, share, index);
            scope.spawn(async move {
                *tally = Some(prepare_camera_rows(input, rows, lane, gpu, index as u8));
            });
        }
        if let Some((index, ((lane, gpu), tally))) = first {
            let rows = lane_rows(rows, share, index);
            *tally = Some(prepare_camera_rows(&input, rows, lane, gpu, index as u8));
        }
    });
    let textures = textures
        .into_inner()
        .expect("scene texture tables are never poisoned");
    binding_cache.textures = textures.slots;
    texture_table.0 = textures.tables;
    scratch.skinned_tess = skinned
        .into_inner()
        .expect("skinned static model cache is never poisoned");

    // Draw order is row order only while lanes take contiguous shares and are
    // concatenated in lane order.
    let mut tally = CameraRowTally::new(census_on);
    for lane_tally in tallies.into_iter().flatten() {
        tally.absorb(lane_tally);
    }
    let mut prepared = std::mem::take(&mut scratch.prepared);
    let mut submitted_keys = std::mem::take(&mut scratch.submitted_keys);
    let mut world_exec_ready_keys = std::mem::take(&mut scratch.world_exec_ready_keys);
    let mut pending_viewmodel_prepared = std::mem::take(&mut scratch.pending_viewmodel_prepared);
    let mut pending_viewmodel_keys = std::mem::take(&mut scratch.pending_viewmodel_keys);
    prepared.clear();
    submitted_keys.clear();
    world_exec_ready_keys.clear();
    pending_viewmodel_prepared.clear();
    pending_viewmodel_keys.clear();
    for lane in &mut scratch.lanes {
        prepared.append(&mut lane.prepared);
        submitted_keys.append(&mut lane.submitted_keys);
        world_exec_ready_keys.append(&mut lane.world_exec_ready_keys);
        pending_viewmodel_prepared.append(&mut lane.pending_viewmodel_prepared);
        pending_viewmodel_keys.append(&mut lane.pending_viewmodel_keys);
    }
    let CameraRowTally {
        mut ready_draws,
        mut refused_draws,
        mut pipeline_not_ready,
        mut last_refusal,
        mut submit_refusals,
        exec_refused,
        unsupported_state,
        bsp_submit_refused_surfaces,
        pipeline_wait,
        bind_smodel_mats,
        last_markmesh_refusal,
        last_markmesh_exec_skip,
        markmesh_missing_58,
        last_glassmesh_exec_skip,
        last_glassmesh_refusal,
        last_mark_packed_custom,
        last_mark_packed_scene_light,
        last_mark_lmap_sampler,
        last_glass_packed_probe,
        last_glass_probe_sampler,
        markmesh_hits,
        glassmesh_hits,
        prepared_hits,
        viewmodel_pipeline_gap,
        cost: prepare_cost,
        run_census: colour_run_census,
        arena_ms,
        arena_share,
        arena_vertex_n,
        arena_pixel_n,
    } = tally;
    let viewmodel_held = if viewmodel_pipeline_gap {
        pending_viewmodel_keys.len()
    } else {
        0
    };
    if viewmodel_colour_submits_when_pipelines_ready(viewmodel_pipeline_gap) {
        ready_draws = ready_draws
            .saturating_add(u32::try_from(pending_viewmodel_keys.len()).unwrap_or(u32::MAX));
        submitted_keys.append(&mut pending_viewmodel_keys);
        prepared.append(&mut pending_viewmodel_prepared);
    } else {
        refused_draws = refused_draws.saturating_add(u32::try_from(viewmodel_held).unwrap_or(0));
        pipeline_not_ready =
            pipeline_not_ready.saturating_add(u32::try_from(viewmodel_held).unwrap_or(0));
        submit_refusals.note_submit_class(
            "xmodel/fpv",
            "PipelineNotReady",
            u32::try_from(viewmodel_held).unwrap_or(0),
        );
        last_refusal = Some(GpuSubmitRefusal::PipelineNotReady);
    }
    if census_on {
        census.frame.submit_prepare_ms = colour_census_ms(prepare_started);
    }
    if !prepared.is_empty() {
        if census_on {
            census.frame.submit_arena_ms = Some(arena_ms);
            census.frame.pack_arena_share_n = Some(arena_share);
            census.frame.pack_arena_vertex_n = Some(as_u32(arena_vertex_n));
            census.frame.pack_arena_pixel_n = Some(as_u32(arena_pixel_n));
        }
        let smodel_ib_skip = smodel_cache_gpu.write_dynamic_indices(
            queue,
            extracted.world.smodel_pretess_indices.as_slice(),
            extracted.world.smodel_index_layout_revision,
        );
        perf::counter!(
            "smodel_ib_skip",
            "iw4l.render",
            f64::from(u8::from(smodel_ib_skip))
        );
    }
    perf::counter!(
        "world_pretess_skip",
        "iw4l.render",
        f64::from(u8::from(world_ib_skip))
    );

    cam.active = true;
    cam.samples = samples;
    cam.needs_floatz = needs_floatz;
    cam.needs_resolved_scene = needs_resolved_scene;
    cam.world_ib_skip = world_ib_skip;
    cam.ready_draws = ready_draws;
    cam.refused_draws = refused_draws;
    cam.pipeline_not_ready = pipeline_not_ready;
    cam.last_refusal = last_refusal;
    cam.submit_refusals = submit_refusals.submit;
    cam.exec_refused = exec_refused;
    cam.exec_refusals = submit_refusals.exec;
    cam.unsupported_state = unsupported_state;
    cam.bsp_submit_refused_surfaces = bsp_submit_refused_surfaces;
    cam.pipeline_wait = pipeline_wait;
    cam.bind_smodel_mats = bind_smodel_mats;
    cam.last_markmesh_refusal = last_markmesh_refusal;
    cam.last_markmesh_exec_skip = last_markmesh_exec_skip;
    cam.markmesh_missing_58 = markmesh_missing_58;
    cam.last_glassmesh_exec_skip = last_glassmesh_exec_skip;
    cam.last_glassmesh_refusal = last_glassmesh_refusal;
    cam.last_mark_packed_custom = last_mark_packed_custom;
    cam.last_mark_packed_scene_light = last_mark_packed_scene_light;
    cam.last_mark_lmap_sampler = last_mark_lmap_sampler;
    cam.last_glass_packed_probe = last_glass_packed_probe;
    cam.last_glass_probe_sampler = last_glass_probe_sampler;
    cam.markmesh_hits = markmesh_hits;
    cam.glassmesh_hits = glassmesh_hits;
    cam.prepared_hits = prepared_hits;
    perf::counter!(
        "draw_source_prepared",
        "iw4l.render",
        f64::from(prepare_cost.source_build_n)
    );
    perf::counter!(
        "draw_source_reused",
        "iw4l.render",
        f64::from(prepare_cost.source_reuse_n)
    );
    cam.prepare_cost = prepare_cost;
    cam.colour_run_census = colour_run_census;
    cam.focused_object_id = focused_object_id;
    cam.viewmodel_held = viewmodel_held;
    scratch.prepared = prepared;
    scratch.submitted_keys = submitted_keys;
    scratch.world_exec_ready_keys = world_exec_ready_keys;
    scratch.pending_viewmodel_prepared = pending_viewmodel_prepared;
    scratch.pending_viewmodel_keys = pending_viewmodel_keys;
    scratch.pack_plan = Some(pack_plan);
}

#[derive(Resource, Default)]
pub(super) struct InstalledColourPass {
    ready: bool,
    sun_shadow_view_ready: bool,
    camera: CameraFrameTargets,
}

#[derive(Clone, Copy, Default)]
struct CameraFrameTargets {
    needs_floatz: bool,
    needs_resolved_scene: bool,
}

#[derive(bevy::ecs::system::SystemParam)]
pub(super) struct SharedColourSetup<'w> {
    pipeline: Res<'w, ExactColourPipeline>,
    constant_arena: ResMut<'w, ExactConstantArena>,
    shadow_arena: ResMut<'w, ShadowmapSunArena>,
    spot_arena: ResMut<'w, ShadowmapSpotArena>,
    shadowmap: ResMut<'w, ShadowmapSunGpu>,
    spotmap: ResMut<'w, ShadowmapSpotGpu>,
}

impl SharedColourSetup<'_> {
    fn install(
        &mut self,
        extracted: ExtractedColourRefs<'_>,
        device: &RenderDevice,
        uploaded: &mut ResMut<RuntimeUploadedImageRegistry>,
    ) -> Option<SharedColourResources> {
        let products = &extracted.frame.frame_products;
        install_shared_colour_resources(SharedColourInstall {
            products,
            extracted,
            pipeline: &self.pipeline,
            device,
            uploaded,
            constant_arena: &mut self.constant_arena,
            shadow_arena: &mut self.shadow_arena,
            spot_arena: &mut self.spot_arena,
            shadowmap: &mut self.shadowmap,
            spotmap: &mut self.spotmap,
        })
    }
}

#[derive(bevy::ecs::system::SystemParam)]
pub(super) struct CameraTargetSetup<'w, 's> {
    views: Query<
        'w,
        's,
        (
            &'static ViewTarget,
            &'static SceneDepthTexture,
            &'static ExtractedView,
            Option<&'static Msaa>,
        ),
        With<Camera3d>,
    >,
    pipeline_cache: Res<'w, PipelineCache>,
    dof: Res<'w, super::super::postfx::ExtractedPostFx>,
    floatz: ResMut<'w, ExactFloatZResolve>,
    floatz_pipelines: ResMut<'w, SpecializedRenderPipelines<ExactFloatZResolve>>,
    resolved_scene: ResMut<'w, super::super::resolved_scene::ResolvedScene>,
}

impl CameraTargetSetup<'_, '_> {
    fn install(
        &mut self,
        extracted: ExtractedColourRefs<'_>,
        device: &RenderDevice,
        uploaded: &mut ResMut<RuntimeUploadedImageRegistry>,
    ) -> CameraFrameTargets {
        let mut cameras = self.views.iter();
        let camera_view = match (cameras.next(), cameras.count()) {
            (view, 0) => view,
            _ => None,
        };
        install_camera_frame_targets(CameraTargetInstall {
            view: camera_view,
            extracted,
            dof: &self.dof,
            pipeline_cache: &self.pipeline_cache,
            device,
            uploaded,
            floatz: &mut self.floatz,
            floatz_pipelines: &mut self.floatz_pipelines,
            resolved_scene: &mut self.resolved_scene,
        })
    }
}

#[derive(bevy::ecs::system::SystemParam)]
pub(super) struct ColourBindingEpochs<'w> {
    binding_cache: ResMut<'w, ExactColourBindingCache>,
    texture_table: ResMut<'w, SceneTextureTables>,
    scratch: ResMut<'w, ColourSubmitScratch>,
    shadow_binding: ResMut<'w, ExactShadowBindingCache>,
    shadow_table: ResMut<'w, ShadowTextureTable>,
    shadow_scratch: ResMut<'w, ShadowSubmitScratch>,
}

impl ColourBindingEpochs<'_> {
    fn open(&mut self, uploaded: &RuntimeUploadedImageRegistry, generation: MaterialGenerationId) {
        open_scene_table_epoch(
            &mut self.binding_cache,
            &mut self.texture_table,
            &mut self.scratch,
            uploaded,
            generation,
        );
        open_shadow_table_epoch(
            &mut self.shadow_binding,
            &mut self.shadow_table,
            &mut self.shadow_scratch,
            uploaded,
            generation,
        );
    }
}

pub(super) fn install_shared_colour_pass(
    frame: Res<PublishedRenderFrame>,
    device: Res<RenderDevice>,
    mut uploaded: ResMut<RuntimeUploadedImageRegistry>,
    mut installed: ResMut<InstalledColourPass>,
    mut shared_setup: SharedColourSetup,
    mut camera_setup: CameraTargetSetup,
    mut epochs: ColourBindingEpochs,
) {
    *installed = InstalledColourPass::default();
    let extracted = ExtractedColourRefs::new(&frame);
    let Some(shared) = shared_setup.install(extracted, &device, &mut uploaded) else {
        return;
    };
    let camera = camera_setup.install(extracted, &device, &mut uploaded);
    let generation = extracted.world.generation;
    epochs.open(&uploaded, generation);
    *installed = InstalledColourPass {
        ready: true,
        sun_shadow_view_ready: shared.sun_shadow_view_ready,
        camera,
    };
}

type CameraTargetView<'a> = (
    &'a ViewTarget,
    &'a SceneDepthTexture,
    &'a ExtractedView,
    Option<&'a Msaa>,
);

struct CameraTargetInstall<'a, 'r> {
    view: Option<CameraTargetView<'a>>,
    extracted: ExtractedColourRefs<'r>,
    dof: &'a super::super::postfx::ExtractedPostFx,
    pipeline_cache: &'a PipelineCache,
    device: &'a RenderDevice,
    uploaded: &'a mut RuntimeUploadedImageRegistry,
    floatz: &'a mut ExactFloatZResolve,
    floatz_pipelines: &'a mut SpecializedRenderPipelines<ExactFloatZResolve>,
    resolved_scene: &'a mut super::super::resolved_scene::ResolvedScene,
}

fn install_camera_frame_targets(install: CameraTargetInstall<'_, '_>) -> CameraFrameTargets {
    let extracted = install.extracted;
    let products = &extracted.frame.frame_products;
    let colour = products.0.product(FrameProductKind::Colour);
    let light = products.0.product(FrameProductKind::Light);
    let emissive = products.0.product(FrameProductKind::Emissive);
    if colour.ordered_draws.is_empty()
        && light.ordered_draws.is_empty()
        && emissive.ordered_draws.is_empty()
    {
        return CameraFrameTargets::default();
    }
    let Some((target, depth, _extracted_view, msaa)) = install.view else {
        return CameraFrameTargets::default();
    };
    let samples = msaa.map_or(1, Msaa::samples);

    install.floatz.resolved_frame = None;
    let needs_floatz = install.dof.frame.dof.active()
        || colour.has_codemesh
        || light.has_codemesh
        || emissive.has_codemesh
        || colour.binds_code_texture(super::CODE_TEXTURE_FLOATZ)
        || light.binds_code_texture(super::CODE_TEXTURE_FLOATZ)
        || emissive.binds_code_texture(super::CODE_TEXTURE_FLOATZ);
    let distortion = products.0.product(FrameProductKind::Distortion);
    let needs_resolved_scene =
        (matches!(distortion.status, FrameProductStatus::ResolveReady { .. })
            && !distortion.ordered_draws.is_empty())
            || colour.binds_code_texture(super::CODE_TEXTURE_RESOLVED_POST_SUN)
            || light.binds_code_texture(super::CODE_TEXTURE_RESOLVED_POST_SUN)
            || emissive.binds_code_texture(super::CODE_TEXTURE_RESOLVED_POST_SUN);
    if needs_resolved_scene {
        let (view, _resized) = install
            .resolved_scene
            .ensure(install.device, target.main_texture());
        install
            .uploaded
            .publish_frame_target(|registry| &mut registry.resolved_post_sun, Some(view));
    }
    let size = depth.texture.size();
    let view = floatz::ensure_target(
        install.floatz,
        install.device,
        size.width,
        size.height,
        samples,
    );
    if needs_floatz {
        install
            .uploaded
            .publish_frame_target(|registry| &mut registry.float_z, view);
        floatz::prepare_blit(
            install.floatz,
            install.pipeline_cache,
            install.device,
            install.floatz_pipelines,
        );
    } else {
        floatz::forget_blit(install.floatz);
    }

    CameraFrameTargets {
        needs_floatz,
        needs_resolved_scene,
    }
}

struct SharedColourInstall<'a, 'r> {
    products: &'r ExtractedRenderFrameProducts,
    extracted: ExtractedColourRefs<'r>,
    pipeline: &'r ExactColourPipeline,
    device: &'r RenderDevice,
    uploaded: &'a mut RuntimeUploadedImageRegistry,
    constant_arena: &'a mut ExactConstantArena,
    shadow_arena: &'a mut ShadowmapSunArena,
    spot_arena: &'a mut ShadowmapSpotArena,
    shadowmap: &'a mut ShadowmapSunGpu,
    spotmap: &'a mut ShadowmapSpotGpu,
}

#[derive(Clone, Copy)]
struct SharedColourResources {
    sun_shadow_view_ready: bool,
}

fn install_shared_colour_resources(
    install: SharedColourInstall<'_, '_>,
) -> Option<SharedColourResources> {
    let generation = install.extracted.world.generation;
    if install.constant_arena.generation != generation {
        install.constant_arena.generation = generation;
        for gpu in &mut install.constant_arena.gpu {
            gpu.bind_group = None;
        }
    }
    if install.pipeline.ports.is_empty() {
        return None;
    }
    let _sampler_table = install.extracted.world.sampler_table.as_ref()?;
    install.shadow_arena.open_generation(generation);
    install.spot_arena.open_generation(generation);
    let sun_shadow_view_ready = publish_this_frame_sun_shadow_view(
        install.products,
        install.uploaded,
        install.shadowmap,
        install.device,
    );
    let _spot_shadow_view_ready = publish_this_frame_spot_shadow_views(
        install.products,
        install.uploaded,
        install.spotmap,
        install.device,
    );
    Some(SharedColourResources {
        sun_shadow_view_ready,
    })
}

fn view_cull_planes(clip_from_world: Option<Mat4>) -> Option<[Vec4; 5]> {
    let m = clip_from_world?;
    let (x, y, w) = (m.row(0), m.row(1), m.row(3));
    let mut planes = [w + x, w - x, w + y, w - y, w];
    for plane in &mut planes {
        let len = plane.truncate().length();
        if !len.is_finite() || len <= f32::EPSILON {
            return None;
        }
        *plane /= len;
    }
    Some(planes)
}

fn sphere_outside(planes: &[Vec4; 5], centre: Vec3, radius: f32) -> bool {
    planes
        .iter()
        .any(|plane| plane.truncate().dot(centre) + plane.w < -radius)
}

struct CameraRowsInput<'a> {
    extracted: ExtractedColourRefs<'a>,
    products: &'a ExtractedRenderFrameProducts,
    colour: &'a FrameProduct,
    light: &'a FrameProduct,
    emissive: &'a FrameProduct,
    geometry: &'a ExactColourGeometry,
    pretess: &'a CameraWorldPretess,
    pipeline: &'a ExactColourPipeline,
    registry: &'a ExactPipelineRegistry,
    device: &'a RenderDevice,
    queue: &'a RenderQueue,
    uploaded: &'a RuntimeUploadedImageRegistry,
    sampler_table: &'a SamplerTable,
    textures: &'a std::sync::Mutex<SceneTextureState>,
    skinned: &'a std::sync::Mutex<smodel_skinned::SmodelSkinnedTess>,
    exec_view: Option<MaterialExecView<'a>>,
    prepare_target: ExactPrepareTarget,
    glass_cull: Option<[Vec4; 5]>,
    census_on: bool,
    sun_shadow_view_ready: bool,
}

fn lane_rows(rows: &[ColourRowPlan], share: usize, index: usize) -> &[ColourRowPlan] {
    let start = index.saturating_mul(share).min(rows.len());
    let end = start.saturating_add(share).min(rows.len());
    &rows[start..end]
}

struct CameraRowTally {
    ready_draws: u32,
    refused_draws: u32,
    pipeline_not_ready: u32,
    last_refusal: Option<GpuSubmitRefusal>,
    submit_refusals: DrawRefusalCensus,
    exec_refused: u32,
    unsupported_state: UnsupportedStateCensus,
    bsp_submit_refused_surfaces: [u32; 4],
    pipeline_wait: PipelineWaitCensus,
    bind_smodel_mats: BTreeMap<String, u32>,
    last_markmesh_refusal: Option<GpuSubmitRefusal>,
    last_markmesh_exec_skip: Option<&'static str>,
    markmesh_missing_58: u32,
    last_glassmesh_exec_skip: Option<&'static str>,
    last_glassmesh_refusal: Option<GpuSubmitRefusal>,
    last_mark_packed_custom: Option<u8>,
    last_mark_packed_scene_light: Option<u8>,
    last_mark_lmap_sampler: Option<u32>,
    last_glass_packed_probe: Option<u8>,
    last_glass_probe_sampler: Option<u32>,
    markmesh_hits: usize,
    glassmesh_hits: usize,
    prepared_hits: u32,
    viewmodel_pipeline_gap: bool,
    cost: PrepareCost,
    run_census: MaterialRunCensus,
    arena_ms: f32,
    arena_share: u32,
    arena_vertex_n: usize,
    arena_pixel_n: usize,
}

impl CameraRowTally {
    fn new(census_on: bool) -> Self {
        Self {
            ready_draws: 0,
            refused_draws: 0,
            pipeline_not_ready: 0,
            last_refusal: None,
            submit_refusals: DrawRefusalCensus::new(census_on),
            exec_refused: 0,
            unsupported_state: UnsupportedStateCensus::default(),
            bsp_submit_refused_surfaces: [0; 4],
            pipeline_wait: PipelineWaitCensus::default(),
            bind_smodel_mats: BTreeMap::new(),
            last_markmesh_refusal: None,
            last_markmesh_exec_skip: None,
            markmesh_missing_58: 0,
            last_glassmesh_exec_skip: None,
            last_glassmesh_refusal: None,
            last_mark_packed_custom: None,
            last_mark_packed_scene_light: None,
            last_mark_lmap_sampler: None,
            last_glass_packed_probe: None,
            last_glass_probe_sampler: None,
            markmesh_hits: 0,
            glassmesh_hits: 0,
            prepared_hits: 0,
            viewmodel_pipeline_gap: false,
            cost: PrepareCost::default(),
            run_census: MaterialRunCensus::default(),
            arena_ms: 0.0,
            arena_share: 0,
            arena_vertex_n: 0,
            arena_pixel_n: 0,
        }
    }

    fn absorb(&mut self, next: Self) {
        fn add_counts<K: Ord>(into: &mut BTreeMap<K, u32>, from: BTreeMap<K, u32>) {
            for (key, n) in from {
                *into.entry(key).or_default() += n;
            }
        }
        self.ready_draws = self.ready_draws.saturating_add(next.ready_draws);
        self.refused_draws = self.refused_draws.saturating_add(next.refused_draws);
        self.pipeline_not_ready = self
            .pipeline_not_ready
            .saturating_add(next.pipeline_not_ready);
        self.last_refusal = next.last_refusal.or(self.last_refusal.take());
        add_counts(
            &mut self.submit_refusals.submit,
            next.submit_refusals.submit,
        );
        add_counts(&mut self.submit_refusals.exec, next.submit_refusals.exec);
        self.exec_refused = self.exec_refused.saturating_add(next.exec_refused);
        self.unsupported_state.absorb(next.unsupported_state);
        for (into, from) in self
            .bsp_submit_refused_surfaces
            .iter_mut()
            .zip(next.bsp_submit_refused_surfaces)
        {
            *into = into.saturating_add(from);
        }
        self.pipeline_wait.absorb(next.pipeline_wait);
        add_counts(&mut self.bind_smodel_mats, next.bind_smodel_mats);
        self.last_markmesh_refusal = next
            .last_markmesh_refusal
            .or(self.last_markmesh_refusal.take());
        self.last_markmesh_exec_skip = next
            .last_markmesh_exec_skip
            .or(self.last_markmesh_exec_skip);
        self.markmesh_missing_58 = self
            .markmesh_missing_58
            .saturating_add(next.markmesh_missing_58);
        self.last_glassmesh_exec_skip = next
            .last_glassmesh_exec_skip
            .or(self.last_glassmesh_exec_skip);
        self.last_glassmesh_refusal = next
            .last_glassmesh_refusal
            .or(self.last_glassmesh_refusal.take());
        self.last_mark_packed_custom = self
            .last_mark_packed_custom
            .or(next.last_mark_packed_custom);
        self.last_mark_packed_scene_light = self
            .last_mark_packed_scene_light
            .or(next.last_mark_packed_scene_light);
        self.last_mark_lmap_sampler = self.last_mark_lmap_sampler.or(next.last_mark_lmap_sampler);
        self.last_glass_packed_probe = self
            .last_glass_packed_probe
            .or(next.last_glass_packed_probe);
        self.last_glass_probe_sampler = self
            .last_glass_probe_sampler
            .or(next.last_glass_probe_sampler);
        self.markmesh_hits = self.markmesh_hits.saturating_add(next.markmesh_hits);
        self.glassmesh_hits = self.glassmesh_hits.saturating_add(next.glassmesh_hits);
        self.prepared_hits = self.prepared_hits.saturating_add(next.prepared_hits);
        self.viewmodel_pipeline_gap |= next.viewmodel_pipeline_gap;
        self.cost.absorb(next.cost);
        self.run_census = add_run_census(self.run_census, next.run_census);
        self.arena_ms += next.arena_ms;
        self.arena_share = self.arena_share.saturating_add(next.arena_share);
        self.arena_vertex_n = self.arena_vertex_n.saturating_add(next.arena_vertex_n);
        self.arena_pixel_n = self.arena_pixel_n.saturating_add(next.arena_pixel_n);
    }
}

fn add_run_census(a: MaterialRunCensus, b: MaterialRunCensus) -> MaterialRunCensus {
    MaterialRunCensus {
        draws: a.draws.saturating_add(b.draws),
        material_runs: a.material_runs.saturating_add(b.material_runs),
        pass_setups: a.pass_setups.saturating_add(b.pass_setups),
        obj_binds: a.obj_binds.saturating_add(b.obj_binds),
        refused: a.refused.saturating_add(b.refused),
        shell_hits: a.shell_hits.saturating_add(b.shell_hits),
        shell_misses: a.shell_misses.saturating_add(b.shell_misses),
        overlay_const_writes: a
            .overlay_const_writes
            .saturating_add(b.overlay_const_writes),
        overlay_need_known: a.overlay_need_known.saturating_add(b.overlay_need_known),
    }
}

fn prepare_camera_rows(
    input: &CameraRowsInput<'_>,
    rows: &[ColourRowPlan],
    lane: &mut ColourPrepareLane,
    gpu_arena: &mut GpuConstantArena,
    lane_index: u8,
) -> CameraRowTally {
    let CameraRowsInput {
        extracted,
        products,
        colour,
        light,
        emissive,
        geometry,
        pretess,
        pipeline,
        registry,
        device,
        queue,
        uploaded,
        sampler_table,
        textures,
        skinned,
        exec_view,
        prepare_target,
        glass_cull,
        census_on,
        sun_shadow_view_ready,
    } = *input;
    let mut ready_draws = 0u32;
    let mut refused_draws = 0u32;
    let mut pipeline_not_ready = 0u32;
    let mut last_refusal: Option<GpuSubmitRefusal> = None;
    let mut submit_refusals = DrawRefusalCensus::new(census_on);
    let mut exec_refused = 0u32;
    let mut unsupported_state = UnsupportedStateCensus::default();
    let mut bsp_submit_refused_surfaces = [0u32; 4];
    let mut pipeline_wait = PipelineWaitCensus::default();
    let mut bind_smodel_mats = BTreeMap::<String, u32>::new();
    let mut last_markmesh_refusal: Option<GpuSubmitRefusal> = None;
    let mut last_markmesh_exec_skip: Option<&'static str> = None;
    let mut markmesh_missing_58 = 0u32;
    let mut last_glassmesh_exec_skip: Option<&'static str> = None;
    let mut last_glassmesh_refusal: Option<GpuSubmitRefusal> = None;
    let mut last_mark_packed_custom: Option<u8> = None;
    let mut last_mark_packed_scene_light: Option<u8> = None;
    let mut last_mark_lmap_sampler: Option<u32> = None;
    let mut last_glass_packed_probe: Option<u8> = None;
    let mut last_glass_probe_sampler: Option<u32> = None;
    let mut markmesh_hits = 0usize;
    let mut glassmesh_hits = 0usize;
    let mut viewmodel_pipeline_gap = false;
    let mut prepared_hits = 0u32;
    let mut prepared = std::mem::take(&mut lane.prepared);
    let mut submitted_keys = std::mem::take(&mut lane.submitted_keys);
    let mut pending_viewmodel_prepared = std::mem::take(&mut lane.pending_viewmodel_prepared);
    let mut pending_viewmodel_keys = std::mem::take(&mut lane.pending_viewmodel_keys);
    let mut world_exec_ready_keys = std::mem::take(&mut lane.world_exec_ready_keys);
    prepared.clear();
    submitted_keys.clear();
    pending_viewmodel_prepared.clear();
    pending_viewmodel_keys.clear();
    world_exec_ready_keys.clear();
    lane.arena_pack.begin_list();
    let mut executor = std::mem::take(&mut lane.executor);
    executor.begin_list();
    let run_pack = std::mem::take(&mut lane.run_pack);
    let mut exact_prepare = ExactPrepare {
        portal_view: None,
        slice_plane: None,
        instance: 0,
        slice_groups: Default::default(),
        extracted,
        geometry,
        pretess: Some(pretess),
        pipeline_res: pipeline,
        registry,
        device,
        uploaded,
        spot_shadow_select: None,
        sampler_table,
        textures: PrepareTextureTables::SceneShared(textures),
        arena: Some(&mut lane.arena_pack),
        run_pack,
        cost: PrepareCost::default(),
        skinned_tess: None,
        skinned_shared: Some(skinned),
    };
    exact_prepare.run_pack.begin_pack_intern_frame();
    for planned in rows {
        let Some(live) = colour_draw_at(colour, light, emissive, planned.draw_index as usize)
        else {
            continue;
        };
        if let RetainedDrawKind::Glass { draw, .. } = live.kind
            && let Some(planes) = glass_cull.as_ref()
            && let Some(&[x, y, z, radius]) = extracted.frame.glass_mesh_bounds.get(draw as usize)
            && sphere_outside(planes, Vec3::new(x, y, z), radius)
        {
            continue;
        }
        let tech = colour_tech_at(colour, light, emissive, planned.draw_index as usize)
            .unwrap_or(planned.technique);
        let row = PreparedColourRow {
            item: live,
            world_surf: planned.world_surface_override,
        };
        let expanded;
        let item = match row.expanded_item() {
            Some(owned) => {
                expanded = owned;
                &expanded
            }
            None => live,
        };
        let index_span = planned.index_span;
        match item.kind {
            RetainedDrawKind::MarkMesh { .. } => {
                markmesh_hits = markmesh_hits.saturating_add(1);
            }
            RetainedDrawKind::Glass { .. } => {
                glassmesh_hits = glassmesh_hits.saturating_add(1);
            }
            _ => {}
        }
        if matches!(item.kind, RetainedDrawKind::MarkMesh { .. })
            && last_mark_packed_custom.is_none()
        {
            let fields = dpvs_iw4::unpack(dpvs_iw4::GfxDrawSurf { packed: item.key });
            last_mark_packed_custom = Some(fields.custom_index);
            last_mark_packed_scene_light = Some(fields.scene_light_index);
            last_mark_lmap_sampler =
                Some(u32::from(item.surface_samplers.primary_lightmap.is_some()));
        }
        if matches!(item.kind, RetainedDrawKind::Glass { .. }) && last_glass_packed_probe.is_none()
        {
            let fields = dpvs_iw4::unpack(dpvs_iw4::GfxDrawSurf { packed: item.key });
            last_glass_packed_probe = Some(fields.reflection_probe_index);
            last_glass_probe_sampler =
                Some(u32::from(item.surface_samplers.reflection_probe.is_some()));
        }
        let mut instances = [Some((*item, None)), None];
        if !is_viewmodel_colour_draw(&item.kind, item.key)
            && let Some(portal) = extracted.frame.exec_frame.portal.as_ref()
            && matches!(item.kind, RetainedDrawKind::XModel { .. })
        {
            for (instance, placement) in portal.entity_instances(&item.kind).into_iter().enumerate()
            {
                let mut copy = *item;
                placement.apply_to(&mut copy.kind);
                instances[instance] = Some((copy, placement.slice_plane));
            }
        }
        for (instance, (instance_item, slice_plane)) in instances.into_iter().flatten().enumerate()
        {
            let item = &instance_item;
            exact_prepare.instance = instance as u16;
            exact_prepare.slice_plane = slice_plane;
            let mut source_cache = None;
            for portal_tag in super::portal_draw_tags(
                &extracted.frame.exec_frame,
                &item.kind,
                &products.0.world_run_surfs,
                item.key,
            ) {
                let foreign_world =
                    extracted
                        .frame
                        .exec_frame
                        .portal
                        .as_ref()
                        .is_some_and(|portal| {
                            portal_tag
                                .checked_sub(1)
                                .and_then(|view| portal.worlds.get(view as usize))
                                .copied()
                                .is_some_and(|world| world != portal.camera_world)
                        });
                let tech = if foreign_world {
                    render_material::TechType(lighting_iw4::without_sun_shadow(tech.0))
                } else {
                    tech
                };
                exact_prepare.portal_view = Some(portal_tag);
                if super::portal_scene::exit_clip_mode(device)
                    == d3d9_sm3::ExitClipMode::FragmentDiscard
                    && super::portal_scene::exit_class(extracted, portal_tag, &item.kind)
                        == super::portal_scene::ExitClass::Outside
                {
                    continue;
                }
                let mut view_item = *item;
                if let RetainedDrawKind::XModel {
                    object_id: u16::MAX,
                    ref mut world_from_local,
                    ..
                } = view_item.kind
                    && let Some(view) = exec_view
                {
                    // Sky geometry is shared, but its centre belongs to this
                    // unfolded camera, including multiple routes into one world.
                    world_from_local.w_axis = view.in_world(portal_tag).view_origin.extend(1.0);
                }
                let item = &view_item;
                let executed = exec_view.map_or(
                    Err(MaterialRefusal::StaleMaterialGeneration {
                        retained: colour.generation_id,
                        current: extracted.world.generation,
                    }),
                    |view| {
                        let view = view.in_world(portal_tag);
                        let vertex_type = MaterialRunExecutor::vertex_type(view, item, tech);
                        executor.execute(view, item, tech, vertex_type, true)
                    },
                );
                let execution = match executed {
                    Ok(()) => executor.execution(),
                    Err(ref cause) => {
                        if matches!(
                            item.kind,
                            RetainedDrawKind::XModel {
                                object_id: u16::MAX,
                                ..
                            }
                        ) {
                            diag::warn_once!(
                                World,
                                "sky draw execution refused: tag={portal_tag} key={} cause={cause:?}",
                                item.key
                            );
                        }
                        exec_refused = exec_refused.saturating_add(1);
                        if census_on && let (Some(kind), _, _, _) = bsp_draw_source(&item.kind) {
                            let lane = bsp_kind_index(kind);
                            bsp_submit_refused_surfaces[lane] =
                                bsp_submit_refused_surfaces[lane].saturating_add(1);
                        }
                        submit_refusals.note_exec(&item.kind, item.key, cause);
                        if census_on && let MaterialRefusal::UnsupportedState { fields, .. } = cause
                        {
                            unsupported_state.note(super::super::state::UnsupportedStateFields {
                                unknown_blend_factor: fields.unknown_blend_factor,
                                unknown_blend_operation: fields.unknown_blend_operation,
                                stencil: fields.stencil,
                            });
                        }
                        if matches!(item.kind, RetainedDrawKind::MarkMesh { .. }) {
                            last_markmesh_exec_skip = Some(material_refusal_class(cause));
                            if matches!(
                                cause,
                                MaterialRefusal::MissingCodeConstant {
                                    stage: RuntimeShaderStage::Vertex,
                                    index: super::super::CODE_BASE_LIGHTING_COORDS,
                                    ..
                                }
                            ) {
                                markmesh_missing_58 = markmesh_missing_58.saturating_add(1);
                            }
                        }
                        if matches!(item.kind, RetainedDrawKind::Glass { .. }) {
                            last_glassmesh_exec_skip = Some(material_refusal_class(cause));
                        }
                        continue;
                    }
                };
                if matches!(item.kind, RetainedDrawKind::World { .. }) {
                    world_exec_ready_keys.push(item.key);
                }
                let viewmodel = is_viewmodel_colour_draw(&item.kind, item.key);
                let dependencies = super::ShadowProductDependencies::from_execution(execution);
                let spot_select = if dependencies.spot {
                    let light = GfxDrawSurf { packed: item.key }.scene_light_index();
                    spot_rt_for_light(products, light)
                } else {
                    None
                };
                exact_prepare.spot_shadow_select = spot_select;
                if sun_shadow_view_missing(sun_shadow_view_ready, dependencies.sun) {
                    let cause = GpuSubmitRefusal::ProductDependencyNotReady {
                        product: FrameProductKind::SunShadow,
                    };
                    refused_draws = refused_draws.saturating_add(1);
                    if census_on && let (Some(kind), _, _, _) = bsp_draw_source(&item.kind) {
                        let lane = bsp_kind_index(kind);
                        bsp_submit_refused_surfaces[lane] =
                            bsp_submit_refused_surfaces[lane].saturating_add(1);
                    }
                    submit_refusals.note_submit(&item.kind, viewmodel, &cause);
                    last_refusal = Some(cause);
                    continue;
                }
                if spot_shadow_view_missing(uploaded, dependencies.spot, spot_select) {
                    let cause = GpuSubmitRefusal::ProductDependencyNotReady {
                        product: FrameProductKind::SpotShadow,
                    };
                    refused_draws = refused_draws.saturating_add(1);
                    if census_on && let (Some(kind), _, _, _) = bsp_draw_source(&item.kind) {
                        let lane = bsp_kind_index(kind);
                        bsp_submit_refused_surfaces[lane] =
                            bsp_submit_refused_surfaces[lane].saturating_add(1);
                    }
                    submit_refusals.note_submit(&item.kind, viewmodel, &cause);
                    last_refusal = Some(cause);
                    continue;
                }
                let dest = if viewmodel {
                    &mut pending_viewmodel_prepared
                } else {
                    &mut prepared
                };
                let dest_start = dest.len();
                // Sky follows the unfolded camera; other instances keep one
                // physical source for every view that draws them this frame.
                if matches!(
                    item.kind,
                    RetainedDrawKind::XModel {
                        object_id: u16::MAX,
                        ..
                    }
                ) {
                    source_cache = None;
                }
                match exact_prepare.prepare_ready_hit(
                    item,
                    &executor,
                    prepare_target,
                    &mut source_cache,
                    dependencies,
                    dest,
                ) {
                    Ok(_) => {
                        prepared_hits = prepared_hits.saturating_add(1);
                        if let Some((start, count)) = index_span {
                            let mut span_ok = true;
                            for draw in &mut dest[dest_start..] {
                                if matches!(draw.tess, ExactTessBind::World) {
                                    let Some(layout) = pretess.layout.as_ref() else {
                                        span_ok = false;
                                        last_refusal =
                                            Some(GpuSubmitRefusal::WorldPretessEpochMismatch {
                                                span_epoch: planned.layout_epoch,
                                                layout_epoch: 0,
                                            });
                                        break;
                                    };
                                    if planned.layout_epoch != layout.epoch {
                                        last_refusal =
                                            Some(GpuSubmitRefusal::WorldPretessEpochMismatch {
                                                span_epoch: planned.layout_epoch,
                                                layout_epoch: layout.epoch,
                                            });
                                        span_ok = false;
                                        break;
                                    }
                                    if start.saturating_add(count) > layout.logical_index_count {
                                        last_refusal =
                                            Some(GpuSubmitRefusal::WorldPretessSpanBeyondLimit {
                                                start,
                                                count,
                                                logical_len: layout.logical_index_count,
                                                epoch: layout.epoch,
                                            });
                                        span_ok = false;
                                        break;
                                    }
                                    draw.start = start;
                                    draw.count = count;
                                } else if matches!(
                                    draw.tess,
                                    ExactTessBind::Smodel
                                        | ExactTessBind::SmodelCached
                                        | ExactTessBind::XModel
                                ) {
                                    draw.start = start;
                                    draw.count = count;
                                }
                            }
                            if !span_ok {
                                dest.truncate(dest_start);
                                prepared_hits = prepared_hits.saturating_sub(1);
                                refused_draws = refused_draws.saturating_add(1);
                                submit_refusals.note_submit_class(
                                    submit_refusal_family(&item.kind, viewmodel),
                                    last_refusal
                                        .as_ref()
                                        .map(submit_refusal_class)
                                        .unwrap_or("WorldPretessSpanBeyondLimit"),
                                    1,
                                );
                                continue;
                            }
                        }
                        if viewmodel {
                            pending_viewmodel_keys.push(item.key);
                        } else {
                            ready_draws = ready_draws.saturating_add(1);
                            submitted_keys.push(item.key);
                        }
                    }
                    Err(cause) => {
                        if matches!(
                            item.kind,
                            RetainedDrawKind::XModel {
                                object_id: u16::MAX,
                                ..
                            }
                        ) {
                            diag::warn_once!(
                                World,
                                "sky draw submit refused: tag={portal_tag} key={} cause={cause:?}",
                                item.key
                            );
                        }
                        refused_draws = refused_draws.saturating_add(1);
                        if census_on && let (Some(kind), _, _, _) = bsp_draw_source(&item.kind) {
                            let lane = bsp_kind_index(kind);
                            bsp_submit_refused_surfaces[lane] =
                                bsp_submit_refused_surfaces[lane].saturating_add(1);
                        }
                        submit_refusals.note_submit(&item.kind, viewmodel, &cause);
                        if matches!(cause, GpuSubmitRefusal::PipelineNotReady) {
                            pipeline_not_ready = pipeline_not_ready.saturating_add(1);
                        }
                        if census_on && matches!(cause, GpuSubmitRefusal::PipelineNotReady) {
                            pipeline_wait
                                .record(&item.kind, viewmodel, item.key, execution, extracted);
                        }
                        if census_on
                            && matches!(item.kind, RetainedDrawKind::Smodel { .. })
                            && matches!(cause, GpuSubmitRefusal::TextureBind(_))
                        {
                            let ordinal = item.material_rank;
                            let name = extracted
                                .world
                                .sorted_material_names
                                .get(ordinal as usize)
                                .cloned()
                                .unwrap_or_else(|| format!("ord{ordinal}"));
                            *bind_smodel_mats.entry(name).or_default() += 1;
                        }
                        if viewmodel && matches!(cause, GpuSubmitRefusal::PipelineNotReady) {
                            viewmodel_pipeline_gap = true;
                        }
                        if matches!(item.kind, RetainedDrawKind::MarkMesh { .. }) {
                            last_markmesh_refusal = Some(cause.clone());
                        }
                        if matches!(item.kind, RetainedDrawKind::Glass { .. }) {
                            last_glassmesh_refusal = Some(cause.clone());
                        }
                        last_refusal = Some(cause);
                    }
                }
            }
        }
    }
    exact_prepare.run_pack.sweep_pack_intern();
    let cost = std::mem::take(&mut exact_prepare.cost);
    lane.run_pack = std::mem::take(&mut exact_prepare.run_pack);
    drop(exact_prepare);
    let run_census = executor.census();
    lane.executor = executor;

    let mut arena_ms = 0.0f32;
    let (mut arena_share, mut arena_vertex_n, mut arena_pixel_n) = (0u32, 0usize, 0usize);
    if !prepared.is_empty() || !pending_viewmodel_prepared.is_empty() {
        let arena_started = std::time::Instant::now();
        let split = prepared.len();
        prepared.append(&mut pending_viewmodel_prepared);
        let layout = prepare_constant_bases(&mut prepared, &mut lane.arena_pack, pipeline);
        (arena_share, arena_vertex_n, arena_pixel_n) = lane.arena_pack.upload(
            gpu_arena,
            layout,
            registry,
            device,
            queue,
            "iw4_exact_vs_constant_arena",
        );
        pending_viewmodel_prepared.extend(prepared.drain(split..));
        arena_ms = arena_started.elapsed().as_secs_f32() * 1000.0;
    }
    for draw in prepared
        .iter_mut()
        .chain(pending_viewmodel_prepared.iter_mut())
    {
        draw.arena_lane = lane_index;
    }
    lane.prepared = prepared;
    lane.submitted_keys = submitted_keys;
    lane.pending_viewmodel_prepared = pending_viewmodel_prepared;
    lane.pending_viewmodel_keys = pending_viewmodel_keys;
    lane.world_exec_ready_keys = world_exec_ready_keys;
    CameraRowTally {
        ready_draws,
        refused_draws,
        pipeline_not_ready,
        last_refusal,
        submit_refusals,
        exec_refused,
        unsupported_state,
        bsp_submit_refused_surfaces,
        pipeline_wait,
        bind_smodel_mats,
        last_markmesh_refusal,
        last_markmesh_exec_skip,
        markmesh_missing_58,
        last_glassmesh_exec_skip,
        last_glassmesh_refusal,
        last_mark_packed_custom,
        last_mark_packed_scene_light,
        last_mark_lmap_sampler,
        last_glass_packed_probe,
        last_glass_probe_sampler,
        markmesh_hits,
        glassmesh_hits,
        prepared_hits,
        viewmodel_pipeline_gap,
        cost,
        run_census,
        arena_ms,
        arena_share,
        arena_vertex_n,
        arena_pixel_n,
    }
}

fn open_scene_table_epoch(
    binding_cache: &mut ExactColourBindingCache,
    scene_tables: &mut SceneTextureTables,
    scratch: &mut ColourSubmitScratch,
    uploaded: &RuntimeUploadedImageRegistry,
    generation: MaterialGenerationId,
) {
    let epoch = TableEpoch {
        generation,
        replaced_revision: uploaded.replaced_revision(),
    };
    for table in &mut scene_tables.0 {
        table.open_epoch(epoch);
    }
    scratch.prepared_scene_epoch = epoch;
    binding_cache.open_epoch(generation, uploaded.views_revision());
}

#[derive(Default)]
struct ColourPrepareLane {
    run_pack: RunPackCache,
    arena_pack: ArenaPack,
    executor: MaterialRunExecutor,
    prepared: Vec<PreparedExactDraw>,
    submitted_keys: Vec<u64>,
    pending_viewmodel_prepared: Vec<PreparedExactDraw>,
    pending_viewmodel_keys: Vec<u64>,
    world_exec_ready_keys: Vec<u64>,
}

#[derive(Resource, Default)]
pub(crate) struct ColourSubmitScratch {
    lanes: [ColourPrepareLane; COLOUR_PREPARE_LANES],

    pub(super) world_exec_ready_keys: Vec<u64>,
    pub(super) prepared: Vec<PreparedExactDraw>,
    pub(super) submitted_keys: Vec<u64>,
    pending_viewmodel_prepared: Vec<PreparedExactDraw>,
    pending_viewmodel_keys: Vec<u64>,
    pack_draws: Vec<PackDraw>,

    pack_plan: Option<ColourPackPlan>,

    skinned_tess: smodel_skinned::SmodelSkinnedTess,

    pub(super) prepared_scene_epoch: TableEpoch,

    logged_skinned: Option<(usize, usize)>,
}

impl ColourSubmitScratch {
    pub(super) fn upload_skinned(
        &mut self,
        device: &RenderDevice,
        queue: &RenderQueue,
    ) -> CameraSkinnedBuffers {
        self.skinned_tess.upload(device, queue);
        CameraSkinnedBuffers {
            vertex: self.skinned_tess.vertex_buffer().cloned(),
            index: self.skinned_tess.index_buffer().cloned(),
            lighting: self.skinned_tess.vertex_lighting_buffer().cloned(),
        }
    }
}

pub(super) struct CameraSkinnedBuffers {
    pub(super) vertex: Option<Buffer>,
    pub(super) index: Option<Buffer>,
    pub(super) lighting: Option<Buffer>,
}
