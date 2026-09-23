use bevy::tasks::{ComputeTaskPool, TaskPool};
use dpvs_iw4::GfxDrawSurf;

use super::*;

fn run_colour_lanes(camera: CameraLane<'_>, shadow: ShadowLane<'_>) {
    let _prepare = perf::Span::RenderColourPrepareMs.enter();
    ComputeTaskPool::get_or_init(TaskPool::default).scope(|scope| {
        scope.spawn(async move { prepare_shadow_passes(shadow) });
        prepare_camera_colour(camera);
    });
}

pub(super) fn prepare_colour_lanes(
    views: Query<
        (
            &ViewTarget,
            &ViewDepthTexture,
            &ExtractedView,
            Option<&Msaa>,
        ),
        With<Camera3d>,
    >,
    (world, frame, installed, geometry, pipeline, registry, device, queue, uploaded): (
        Res<InstalledRenderWorld>,
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
            world: &world,
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
            world: &world,
            frame: &frame,
            installed: &installed,
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
    pub world: &'a InstalledRenderWorld,
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
        world,
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
    let _span = perf::Span::RenderPrepareCameraMs.enter();
    let extracted = ExtractedColourRefs::new(world, frame);
    let products = &extracted.frame.frame_products;
    *cam = CameraPrepareState::default();
    let census_on = perf::recording();
    if census_on {
        reset_exact_colour_census(census);
    }
    census.submitted_keys.clear();
    scratch.skinned_tess.begin_frame();
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
    let mut ready_draws = 0u32;
    let mut refused_draws = 0u32;

    let mut pipeline_not_ready = 0u32;
    let mut last_refusal: Option<GpuSubmitRefusal> = None;

    let mut submit_refusals = DrawRefusalCensus::new(census_on);

    let mut exec_refused = 0u32;
    let mut unsupported_state = UnsupportedStateCensus::default();
    let mut bsp_submit_refused_surfaces = [0u32; 4];
    let mut pnr_smodel_mats = BTreeMap::<String, u32>::new();
    let mut pnr_world_mats = BTreeMap::<String, u32>::new();
    let mut pnr_smodel_ps = BTreeMap::<String, u32>::new();
    let mut pnr_world_ps = BTreeMap::<String, u32>::new();
    let mut pnr_smodel_keys = HashSet::<u64>::new();
    let mut pnr_world_keys = HashSet::<u64>::new();
    let mut pnr_ports = HashSet::<PortId>::new();
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

    scratch.clear();
    let mut prepared = std::mem::take(&mut scratch.prepared);
    let mut submitted_keys = std::mem::take(&mut scratch.submitted_keys);
    let mut pending_viewmodel_prepared = std::mem::take(&mut scratch.pending_viewmodel_prepared);
    let mut pending_viewmodel_keys = std::mem::take(&mut scratch.pending_viewmodel_keys);
    let focused_object_id = products.0.focus().and_then(|focus| focus.object_id);

    let world_run_surfs = products.0.world_run_surfs.as_slice();
    let pretess_key = world_pretess_key(
        colour,
        light,
        emissive,
        world_run_surfs,
        geometry.world_generation,
        geometry.generation,
        products.0.world_run_revision,
    );
    let pack_key = colour_pack_key(
        colour,
        light,
        emissive,
        pretess_key,
        geometry.smodel_index_count,
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
            geometry.world_vertex_count as u32,
            &geometry.smodel_surface_ranges,
            &geometry.xmodel_surface_ranges,
            &mut scratch.pack_draws,
        );
        let work = r_draw_surf_list_work_colour(&packed);
        diag::info!(
            World,
            "smodel skinned: packed={} unconsumed={}",
            packed.smodel_skinned.len(),
            work.smodel_skinned_unconsumed,
        );
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
        census.end_depth_restore_n = Some(pack_plan.work.end_restore_n);
        census.end_depth_range_type = Some(pack_plan.work.end_depth_range_type);
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
        census.world_index_gaps = Some(gathered.index_gaps);
        census.world_run_indices_n = Some(pretess.logical_index_count());
        census.world_ib_skip = Some(u32::from(world_ib_skip));
    }
    if census_on {
        census.world_gathered = Some(u32::from(pretess.index().is_some()));
    }
    let mut viewmodel_pipeline_gap = false;
    let mut prepared_hits = 0u32;

    if census_on {
        census.smodel_pretess_skip = Some(0);
        census.smodel_pretess_runs = Some(0);
        census.smodel_pretess_hits = Some(0);
        census.smodel_pretess_verts = Some(0);
        census.smodel_pretess_indices = Some(0);
        census.smodel_cached_lighting = Some(0);
        census.smodel_pretess_local = Some(0);
        census.smodel_pretess_length1 = Some(0);
        census.submit_gather_ms = colour_census_ms(gather_started);
    }
    let mut arena_pack = std::mem::take(&mut scratch.arena_pack);
    arena_pack.begin_list();
    let prepare_started = colour_census_clock(census_on);

    let mut executor = std::mem::take(&mut scratch.executor);
    let mut world_exec_ready_keys = std::mem::take(&mut scratch.world_exec_ready_keys);
    executor.begin_list();
    let exec_frame = &extracted.frame.exec_frame;
    let exec_view = exec_tables(extracted)
        .map(|(catalog, prepared)| MaterialExecView::camera(catalog, prepared, exec_frame));
    let run_pack = std::mem::take(&mut scratch.run_pack);
    let mut exact_prepare = ExactPrepare {
        extracted,
        geometry,
        pretess: Some(pretess),
        pipeline_res: pipeline,
        registry,
        device,
        uploaded,
        spot_shadow_select: None,
        sampler_table,
        textures: PrepareTextureTables::Scene {
            slots: &mut binding_cache.textures,
            tables: &mut texture_table.0,
        },
        arena: Some(&mut arena_pack),
        run_pack,
        cost: PrepareCost::default(),
        skinned_tess: Some(&mut scratch.skinned_tess),
    };
    exact_prepare.run_pack.begin_pack_intern_frame();
    let prepare_target = ExactPrepareTarget {
        color: target.main_texture_format(),
        samples,
        depth: CORE_3D_DEPTH_FORMAT,
        forward_z: false,
        use_world_pretess: true,
    };
    for planned in &pack_plan.row_plan {
        let Some(live) = colour_draw_at(colour, light, emissive, planned.draw_index as usize)
        else {
            continue;
        };
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
        let executed = exec_view.map_or(
            Err(MaterialRefusal::StaleMaterialGeneration {
                retained: colour.generation_id,
                current: extracted.world.generation,
            }),
            |view| {
                let vertex_type = MaterialRunExecutor::vertex_type(view, item, tech);
                executor.execute(view, item, tech, vertex_type, true)
            },
        );
        let execution = match executed {
            Ok(()) => executor.execution(),
            Err(ref cause) => {
                exec_refused = exec_refused.saturating_add(1);
                if census_on && let (Some(kind), _, _, _) = bsp_draw_source(&item.kind) {
                    let lane = bsp_kind_index(kind);
                    bsp_submit_refused_surfaces[lane] =
                        bsp_submit_refused_surfaces[lane].saturating_add(1);
                }
                submit_refusals.note_exec(&item.kind, item.key, cause);
                if census_on && let MaterialRefusal::UnsupportedState { fields, .. } = cause {
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
        let place = executor.place();
        let run_serial = executor.run_serial();
        if matches!(item.kind, RetainedDrawKind::World { .. }) {
            world_exec_ready_keys.push(item.key);
        }
        let viewmodel = is_viewmodel_colour_draw(&item.kind, item.key);
        let binds_sun_shadow =
            execution_binds_code_texture(execution, super::CODE_TEXTURE_SHADOWMAP_SUN);
        let binds_spot_shadow =
            execution_binds_code_texture(execution, super::CODE_TEXTURE_SHADOWMAP_SPOT);
        let spot_select = if binds_spot_shadow {
            let light = GfxDrawSurf { packed: item.key }.scene_light_index();
            spot_rt_for_light(products, light)
        } else {
            None
        };
        exact_prepare.spot_shadow_select = spot_select;
        if sun_shadow_view_missing(sun_shadow_view_ready, binds_sun_shadow) {
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
        if spot_shadow_view_missing(uploaded, binds_spot_shadow, spot_select) {
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
        match exact_prepare.prepare_ready_hit(
            item,
            execution,
            run_serial,
            place,
            item.surface_samplers,
            prepare_target,
            binds_sun_shadow,
            binds_spot_shadow,
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
                                last_refusal = Some(GpuSubmitRefusal::WorldPretessEpochMismatch {
                                    span_epoch: planned.layout_epoch,
                                    layout_epoch: 0,
                                });
                                break;
                            };
                            if planned.layout_epoch != layout.epoch {
                                last_refusal = Some(GpuSubmitRefusal::WorldPretessEpochMismatch {
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
                    record_pipeline_not_ready(
                        &item.kind,
                        viewmodel,
                        item.key,
                        execution,
                        extracted,
                        &mut pnr_smodel_mats,
                        &mut pnr_world_mats,
                        &mut pnr_smodel_ps,
                        &mut pnr_world_ps,
                        &mut pnr_smodel_keys,
                        &mut pnr_world_keys,
                        &mut pnr_ports,
                    );
                }
                if census_on
                    && matches!(item.kind, RetainedDrawKind::Smodel { .. })
                    && matches!(cause, GpuSubmitRefusal::TextureBind(_))
                {
                    let ordinal = world_material_sorted(item.key);
                    let name = extracted
                        .world
                        .sorted_material_names
                        .get(usize::from(ordinal))
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
    exact_prepare.run_pack.sweep_pack_intern();
    let prepare_cost = std::mem::take(&mut exact_prepare.cost);
    let run_pack = std::mem::take(&mut exact_prepare.run_pack);
    drop(exact_prepare);
    scratch.run_pack = run_pack;
    let colour_run_census = executor.census();
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
        census.submit_prepare_ms = colour_census_ms(prepare_started);
    }
    if !prepared.is_empty() {
        let arena_started = colour_census_clock(census_on);
        let (pack_arena_share, arena_vertex_n, arena_pixel_n) = upload_constant_arena(
            &mut prepared,
            &mut arena_pack,
            &mut constant_arena.gpu,
            pipeline,
            registry,
            device,
            queue,
            "iw4_exact_vs_constant_arena",
            "iw4_exact_ps_constant_arena",
        );
        if census_on {
            census.submit_arena_ms = colour_census_ms(arena_started);
            census.pack_arena_share_n = Some(pack_arena_share);
            census.pack_arena_vertex_n = Some(as_u32(arena_vertex_n));
            census.pack_arena_pixel_n = Some(as_u32(arena_pixel_n));
        }
        let smodel_ib_skip = smodel_cache_gpu.write_dynamic_indices(
            queue,
            extracted.world.smodel_pretess_indices.as_slice(),
            extracted.world.smodel_index_layout_revision,
        );
        perf::Counter::SmodelIbSkip.emit(f64::from(u8::from(smodel_ib_skip)));
    }
    perf::Counter::WorldPretessSkip.emit(f64::from(u8::from(world_ib_skip)));

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
    cam.pnr_smodel_mats = pnr_smodel_mats;
    cam.pnr_world_mats = pnr_world_mats;
    cam.pnr_smodel_ps = pnr_smodel_ps;
    cam.pnr_world_ps = pnr_world_ps;
    cam.pnr_smodel_keys = pnr_smodel_keys;
    cam.pnr_world_keys = pnr_world_keys;
    cam.pnr_ports = pnr_ports;
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
    cam.prepare_cost = prepare_cost;
    cam.colour_run_census = colour_run_census;
    cam.focused_object_id = focused_object_id;
    cam.viewmodel_held = viewmodel_held;
    scratch.prepared = prepared;
    scratch.arena_pack = arena_pack;
    scratch.submitted_keys = submitted_keys;
    scratch.world_exec_ready_keys = world_exec_ready_keys;
    scratch.executor = executor;
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

pub(super) struct ShadowLane<'a> {
    pub world: &'a InstalledRenderWorld,
    pub frame: &'a PublishedRenderFrame,
    pub installed: &'a InstalledColourPass,
    pub geometry: &'a ExactColourGeometry,
    pub pipeline: &'a ExactColourPipeline,
    pub registry: &'a ExactPipelineRegistry,
    pub device: &'a RenderDevice,
    pub queue: &'a RenderQueue,
    pub uploaded: &'a RuntimeUploadedImageRegistry,
    pub binding_cache: &'a mut ExactShadowBindingCache,
    pub shadow_table: &'a mut ShadowTextureTable,
    pub shadow_arena: &'a mut ShadowmapSunArena,
    pub spot_arena: &'a mut ShadowmapSpotArena,
    pub static_draws: &'a mut ResidentShadowStaticDraws,
    pub shadowmap: &'a mut ShadowmapSunGpu,
    pub spotmap: &'a mut ShadowmapSpotGpu,
    pub scratch: &'a mut ShadowSubmitScratch,
}

pub(super) fn prepare_shadow_passes(lane: ShadowLane<'_>) {
    let ShadowLane {
        world,
        frame,
        installed,
        geometry,
        pipeline,
        registry,
        device,
        queue,
        uploaded,
        binding_cache,
        shadow_table,
        shadow_arena,
        spot_arena,
        static_draws,
        shadowmap,
        spotmap,
        scratch,
    } = lane;
    let _span = perf::Span::RenderPrepareShadowMs.enter();
    if !installed.ready {
        return;
    }
    let Some(sampler_table) = world.sampler_table.as_ref() else {
        return;
    };
    let extracted = ExtractedColourRefs::new(world, frame);
    let products = &extracted.frame.frame_products;
    let mut sun_exec = std::mem::take(&mut scratch.sun_exec);
    let mut spot_exec = std::mem::take(&mut scratch.spot_exec);
    scratch.skinned_tess.begin_frame();
    let sun_prepared = prepare_shadowmap_sun(
        products,
        extracted,
        geometry,
        pipeline,
        registry,
        device,
        queue,
        uploaded,
        binding_cache,
        shadow_table,
        shadow_arena,
        shadowmap,
        static_draws,
        sampler_table,
        &mut sun_exec,
        &mut scratch.skinned_tess,
    );
    let spot_prepared = prepare_shadowmap_spot(
        products,
        extracted,
        geometry,
        pipeline,
        registry,
        device,
        queue,
        uploaded,
        binding_cache,
        shadow_table,
        spot_arena,
        spotmap,
        sampler_table,
        &mut spot_exec,
        &mut scratch.skinned_tess,
    );
    scratch.sun_prepared = sun_prepared;
    scratch.spot_prepared = spot_prepared;
    scratch.sun_exec = sun_exec;
    scratch.spot_exec = spot_exec;
}

pub(super) fn install_shared_colour_pass(
    views: Query<
        (
            &ViewTarget,
            &ViewDepthTexture,
            &ExtractedView,
            Option<&Msaa>,
        ),
        With<Camera3d>,
    >,
    world: Res<InstalledRenderWorld>,
    frame: Res<PublishedRenderFrame>,
    pipeline: Res<ExactColourPipeline>,
    pipeline_cache: Res<PipelineCache>,
    device: Res<RenderDevice>,
    queue: Res<RenderQueue>,
    dof: Res<super::super::postfx::ExtractedPostFx>,
    (
        mut uploaded,
        mut binding_cache,
        mut shadow_binding,
        mut constant_arena,
        mut texture_table,
        mut shadow_table,
        (mut scratch, mut shadow_scratch),
    ): (
        ResMut<RuntimeUploadedImageRegistry>,
        ResMut<ExactColourBindingCache>,
        ResMut<ExactShadowBindingCache>,
        ResMut<ExactConstantArena>,
        ResMut<SceneTextureTables>,
        ResMut<ShadowTextureTable>,
        (ResMut<ColourSubmitScratch>, ResMut<ShadowSubmitScratch>),
    ),
    (
        mut shadow_arena,
        mut spot_arena,
        mut shadowmap,
        mut spotmap,
        mut floatz,
        mut floatz_pipelines,
        mut resolved_scene,
        mut installed,
    ): (
        ResMut<ShadowmapSunArena>,
        ResMut<ShadowmapSpotArena>,
        ResMut<ShadowmapSunGpu>,
        ResMut<ShadowmapSpotGpu>,
        ResMut<ExactFloatZResolve>,
        ResMut<SpecializedRenderPipelines<ExactFloatZResolve>>,
        ResMut<super::super::resolved_scene::ResolvedScene>,
        ResMut<InstalledColourPass>,
    ),
) {
    *installed = InstalledColourPass::default();
    let extracted = ExtractedColourRefs::new(&world, &frame);
    let products = &extracted.frame.frame_products;
    let Some(shared) = install_shared_colour_resources(SharedColourInstall {
        products,
        extracted,
        pipeline: &pipeline,
        device: &device,
        uploaded: &mut uploaded,
        constant_arena: &mut constant_arena,
        shadow_arena: &mut shadow_arena,
        spot_arena: &mut spot_arena,
        shadowmap: &mut shadowmap,
        spotmap: &mut spotmap,
    }) else {
        return;
    };
    let mut cameras = views.iter();
    let camera_view = match (cameras.next(), cameras.count()) {
        (view, 0) => view,
        _ => None,
    };
    let camera = install_camera_frame_targets(CameraTargetInstall {
        view: camera_view,
        extracted,
        dof: &dof,
        pipeline_cache: &pipeline_cache,
        device: &device,
        queue: &queue,
        uploaded: &mut uploaded,
        floatz: &mut floatz,
        floatz_pipelines: &mut floatz_pipelines,
        resolved_scene: &mut resolved_scene,
    });
    let generation = extracted.world.generation;
    open_scene_table_epoch(
        &mut binding_cache,
        &mut texture_table,
        &mut scratch,
        &uploaded,
        generation,
    );
    open_shadow_table_epoch(
        &mut shadow_binding,
        &mut shadow_table,
        &mut shadow_scratch,
        &uploaded,
        generation,
    );
    *installed = InstalledColourPass {
        ready: true,
        sun_shadow_view_ready: shared.sun_shadow_view_ready,
        camera,
    };
}

type CameraTargetView<'a> = (
    &'a ViewTarget,
    &'a ViewDepthTexture,
    &'a ExtractedView,
    Option<&'a Msaa>,
);

struct CameraTargetInstall<'a, 'r> {
    view: Option<CameraTargetView<'a>>,
    extracted: ExtractedColourRefs<'r>,
    dof: &'a super::super::postfx::ExtractedPostFx,
    pipeline_cache: &'a PipelineCache,
    device: &'a RenderDevice,
    queue: &'a RenderQueue,
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
    let Some((target, depth, extracted_view, msaa)) = install.view else {
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
    if needs_floatz {
        let size = depth.texture.size();
        let (view, _resized) =
            floatz::ensure_target(install.floatz, install.device, size.width, size.height);
        if view.is_some() {
            install
                .uploaded
                .publish_frame_target(|registry| &mut registry.float_z, view);
        }

        floatz::prepare_blit(
            install.floatz,
            install.pipeline_cache,
            install.device,
            install.queue,
            install.floatz_pipelines,
            depth.view(),
            samples,
            floatz::znear_from_clip_from_view(extracted_view.clip_from_view).unwrap_or(0.0),
            extracted.frame.exec_frame.viewmodel_near,
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
        install.constant_arena.gpu.bind_group = None;
    }
    if install.pipeline.ports.is_empty() {
        return None;
    }
    let _sampler_table = install.extracted.world.sampler_table.as_ref()?;
    if install.shadow_arena.generation != generation {
        install.shadow_arena.generation = generation;
        for gpu in &mut install.shadow_arena.gpu {
            gpu.bind_group = None;
        }
    }
    if install.spot_arena.generation != generation {
        install.spot_arena.generation = generation;
        install.spot_arena.gpu.bind_group = None;
    }
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
