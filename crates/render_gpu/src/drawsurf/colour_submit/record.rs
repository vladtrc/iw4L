use super::encode::{ColourPassInputs, RecordMesh};
use super::*;

#[derive(bevy::ecs::system::SystemParam)]
pub(super) struct ColourFrameSource<'w> {
    frame: Res<'w, PublishedRenderFrame>,
    support: Res<'w, crate::SkinSupport>,
    skin_failure: Res<'w, render_scene::SkinFailure>,
    geometry: Res<'w, ExactColourGeometry>,
    smodel_cache_gpu: Res<'w, SmodelCacheGpu>,
    pretess: Res<'w, CameraWorldPretess>,
}

#[derive(bevy::ecs::system::SystemParam)]
pub(super) struct ColourPassBindings<'w> {
    pipeline: Res<'w, ExactColourPipeline>,
    registry: Res<'w, ExactPipelineRegistry>,
    cache: Res<'w, PipelineCache>,
    device: Res<'w, RenderDevice>,
    queue: Res<'w, RenderQueue>,
    adapter: Res<'w, bevy::render::renderer::RenderAdapter>,
    binding_cache: Res<'w, ExactColourBindingCache>,
    constant_arena: Res<'w, ExactConstantArena>,
}

#[derive(bevy::ecs::system::SystemParam)]
pub(super) struct ColourShadowRecorder<'w> {
    shadow_binding: Res<'w, ExactShadowBindingCache>,
    shadow_arena: Res<'w, ShadowmapSunArena>,
    spot_arena: Res<'w, ShadowmapSpotArena>,
    static_draws: Res<'w, ResidentShadowStaticDraws>,
    shadow_scratch: ResMut<'w, ShadowSubmitScratch>,
    shadow_table: ResMut<'w, ShadowTextureTable>,
}

#[derive(bevy::ecs::system::SystemParam)]
pub(super) struct ColourSubmitAccounting<'w> {
    census: ResMut<'w, ExactColourSubmitCensus>,
    focus_submit: ResMut<'w, FocusedOwnerSubmitState>,
    working_set: ResMut<'w, crate::ColourWorkingSet>,
}

#[derive(bevy::ecs::system::SystemParam)]
pub(super) struct ColourPassRecorder<'w> {
    cam: ResMut<'w, CameraPrepareState>,
    scratch: ResMut<'w, ColourSubmitScratch>,
    indirect: ResMut<'w, super::indirect::ExactIndirectDraws>,
    floatz: ResMut<'w, ExactFloatZResolve>,
    resolved_scene: Res<'w, super::super::resolved_scene::ResolvedScene>,
    texture_table: ResMut<'w, SceneTextureTables>,
}

impl ColourFrameSource<'_> {
    fn skin_refused(&self) -> bool {
        self.support.resident_budget == 0
            || self.frame.skin_refused
            || self.skin_failure.reason.is_some()
    }
}

impl ColourPassBindings<'_> {
    fn has_ports(&self) -> bool {
        !self.pipeline.ports.is_empty()
    }
    fn texture_layout(&self) -> &BindGroupLayoutDescriptor {
        texture_table_layout(self.pipeline.as_ref()).expect("ports are non-empty above")
    }
}

impl ColourSubmitAccounting<'_> {
    fn begin(&mut self) {
        *self.working_set = crate::ColourWorkingSet::default();
    }
    fn publish_coverage(&mut self, submitted: &[u64], world_ready: &[u64]) {
        self.census.publish_coverage(submitted, world_ready);
    }
}

impl ColourShadowRecorder<'_> {
    fn record(
        &mut self,
        bindings: &ColourPassBindings,
        geometry: &ExactColourGeometry,
        context: &mut RenderContext,
        census_on: bool,
    ) -> shadow_prepare::ShadowSubmitResult {
        self.shadow_scratch.record(
            shadow_prepare::ShadowRecorder {
                pipeline: &bindings.pipeline,
                registry: &bindings.registry,
                geometry,
                device: &bindings.device,
                table: &mut self.shadow_table,
                context,
            },
            &bindings.queue,
            &self.shadow_arena,
            &self.spot_arena,
            &self.static_draws,
            census_on,
        )
    }
}

impl ColourPassRecorder<'_> {
    fn record(
        mut self,
        view: ViewQuery<(
            &ViewTarget,
            &SceneDepthTexture,
            &ExtractedView,
            Option<&Msaa>,
        )>,
        source: ColourFrameSource,
        bindings: ColourPassBindings,
        mut shadow: ColourShadowRecorder,
        mut accounting: ColourSubmitAccounting,
        mut context: RenderContext,
    ) {
        if source.skin_refused() {
            return;
        }
        let extracted = ExtractedColourRefs::new(&source.frame);
        let products = &extracted.frame.frame_products;
        let _colour_submit = perf::span!("colour_submit", "iw4l.render");
        let census_on = perf::recording();

        accounting.begin();
        if !bindings.has_ports() {
            accounting.focus_submit.emit(
                products,
                Some((
                    &extracted.world.sorted_material_names,
                    &extracted.world.image_handles,
                )),
                exec_tables(extracted),
                FocusedOwnerSubmitReport {
                    terminal: "no_exact_ports",
                    prepared_surfaces: 0,
                    prepared_passes: 0,
                    drawn_passes: 0,
                },
            );
            return;
        }
        if extracted.world.sampler_table.is_none() {
            accounting.focus_submit.emit(
                products,
                Some((
                    &extracted.world.sorted_material_names,
                    &extracted.world.image_handles,
                )),
                exec_tables(extracted),
                FocusedOwnerSubmitReport {
                    terminal: "sampler_table_missing",
                    prepared_surfaces: 0,
                    prepared_passes: 0,
                    drawn_passes: 0,
                },
            );
            return;
        }

        let scene_epoch = self.scratch.prepared_scene_epoch;
        let shadow_epoch = shadow.shadow_scratch.prepared_shadow_epoch;
        let table_epoch_ok = scene_epoch.generation == extracted.world.generation
            && bindings.binding_cache.generation == extracted.world.generation
            && shadow.shadow_binding.generation == extracted.world.generation
            && bindings.constant_arena.generation == extracted.world.generation
            && shadow.shadow_table.epoch() == shadow_epoch
            && self
                .texture_table
                .0
                .iter()
                .all(|table| table.epoch() == scene_epoch);
        if !table_epoch_ok {
            diag::error!(
                World,
                "drawsurf colour submit: prepared work is from another table epoch (extracted={:?} binding_cache={:?} shadow_binding={:?} arena={:?} prepared_scene={scene_epoch:?} prepared_shadow={shadow_epoch:?} scene={:?} shadow={:?}) — prepare did not run before record; refusing to record against a table this frame never numbered",
                extracted.world.generation,
                bindings.binding_cache.generation,
                shadow.shadow_binding.generation,
                bindings.constant_arena.generation,
                self.texture_table
                    .0
                    .each_ref()
                    .map(ExactTextureTable::epoch),
                shadow.shadow_table.epoch(),
            );
            accounting.focus_submit.emit(
                products,
                Some((
                    &extracted.world.sorted_material_names,
                    &extracted.world.image_handles,
                )),
                exec_tables(extracted),
                FocusedOwnerSubmitReport {
                    terminal: "table_epoch_mismatch",
                    prepared_surfaces: 0,
                    prepared_passes: 0,
                    drawn_passes: 0,
                },
            );
            return;
        }
        let colour = products.0.product(FrameProductKind::Colour);
        let light = products.0.product(FrameProductKind::Light);
        let emissive = products.0.product(FrameProductKind::Emissive);
        let tess = self
            .scratch
            .upload_skinned(&bindings.device, &bindings.queue);
        let smodel_skinned_vertex = tess.vertex.as_ref();
        let smodel_skinned_index = tess.index.as_ref();
        let smodel_skinned_vertex_lighting = tess.lighting.as_ref();
        let shadow_prepare::ShadowSubmitResult {
            sun: sun_submit,
            spot: spot_submit,
            submit_started: sun_submit_started,
        } = shadow.record(&bindings, &source.geometry, &mut context, census_on);
        let mut record_n = sun_submit.record_n;
        record_n.add(spot_submit.record_n);
        perf::counter!(
            "spot_shadow_cache_hit",
            "iw4l.render",
            f64::from(spot_submit.cache_hits)
        );
        perf::counter!(
            "spot_shadow_updates",
            "iw4l.render",
            f64::from(spot_submit.updates)
        );
        if spot_submit.miss > 0 {
            perf::instant!("spot_shadow_refusal", "iw4l.render", |ctx| {
                ctx.arg("count", i64::from(spot_submit.miss));
                ctx.arg("cause", spot_submit.cause.as_deref().unwrap_or("unknown"));
            });
        }
        if census_on {
            accounting.census.frame.sun_shadow_gpu = Some(sun_submit.gpu);
            accounting.census.frame.sun_shadow_gpu_miss = Some(sun_submit.miss);
            accounting.census.frame.sun_shadow_gpu_cause = sun_submit.cause;
            accounting.census.frame.sun_shadow_gpu_causes = sun_submit.causes;
            accounting.census.preparation.sun_shadow_static_hit = Some(sun_submit.static_hit);
            accounting.census.frame.sun_shadow_world_ib_n = Some(sun_submit.world_ib_n);
            accounting.census.frame.sun_shadow_submit_ms = colour_census_ms(sun_submit_started);
            accounting.census.preparation.sun_shadow_prepare_ms = Some(sun_submit.prepare_ms);
            accounting.census.preparation.sun_shadow_patch_ms = Some(sun_submit.patch_ms);
            accounting.census.preparation.sun_shadow_arena_ms = Some(sun_submit.arena_ms);
            accounting.census.preparation.sun_shadow_record_ms = Some(sun_submit.record_ms);
            accounting.census.frame.spot_shadow_gpu = Some(spot_submit.gpu);
            accounting.census.frame.spot_shadow_gpu_miss = Some(spot_submit.miss);
            accounting.census.frame.spot_shadow_gpu_cause = spot_submit.cause;
            accounting.census.frame.spot_shadow_slot_n = Some(spot_submit.slots);
            accounting.census.frame.set_bind_group_n = Some(record_n.total());
            accounting.census.frame.set_state_n = Some(record_n.state);
            accounting.census.frame.multi_draw_n = Some(record_n.multi_draws);
            accounting.census.frame.multi_draw_commands_n = Some(record_n.multi_draw_commands);
            accounting.census.preparation.set_bind_group0_n = Some(record_n.group0);
            accounting.census.preparation.set_bind_group1_n = Some(record_n.group1);
            accounting.census.preparation.sun_shadow_finish_ms = Some(sun_submit.finish_ms);
            accounting.census.preparation.sun_shadow_queue_ms = Some(sun_submit.emit_ms);
            let named = sun_submit.emit_ms
                + sun_submit.prepare_ms
                + sun_submit.patch_ms
                + sun_submit.arena_ms
                + sun_submit.record_ms
                + sun_submit.finish_ms;
            accounting.census.preparation.sun_shadow_unnamed_ms = accounting
                .census
                .frame
                .sun_shadow_submit_ms
                .filter(|envelope| *envelope >= named)
                .map(|envelope| envelope - named);
        }
        if colour.ordered_draws.is_empty()
            && light.ordered_draws.is_empty()
            && emissive.ordered_draws.is_empty()
        {
            accounting.focus_submit.emit(
                products,
                Some((
                    &extracted.world.sorted_material_names,
                    &extracted.world.image_handles,
                )),
                exec_tables(extracted),
                FocusedOwnerSubmitReport {
                    terminal: "product_empty",
                    prepared_surfaces: 0,
                    prepared_passes: 0,
                    drawn_passes: 0,
                },
            );
            return;
        }
        let (target, depth, extracted_view, _msaa) = view.into_inner();
        if !self.cam.active {
            accounting.focus_submit.emit(
                products,
                Some((
                    &extracted.world.sorted_material_names,
                    &extracted.world.image_handles,
                )),
                exec_tables(extracted),
                FocusedOwnerSubmitReport {
                    terminal: "camera_prepare_skipped",
                    prepared_surfaces: 0,
                    prepared_passes: 0,
                    drawn_passes: 0,
                },
            );
            return;
        }
        let ready_draws = self.cam.ready_draws;
        let mut refused_draws = self.cam.refused_draws;
        let mut pipeline_not_ready = self.cam.pipeline_not_ready;
        let mut last_refusal = self.cam.last_refusal.take();
        let mut refusals = DrawRefusalCensus::taking(
            census_on,
            std::mem::take(&mut self.cam.submit_refusals),
            std::mem::take(&mut self.cam.exec_refusals),
        );
        let exec_refused = self.cam.exec_refused;
        let unsupported_state = self.cam.unsupported_state;
        let bsp_submit_refused_surfaces = self.cam.bsp_submit_refused_surfaces;
        let pipeline_wait = std::mem::take(&mut self.cam.pipeline_wait);
        let bind_smodel_mats = std::mem::take(&mut self.cam.bind_smodel_mats);
        let last_markmesh_refusal = self.cam.last_markmesh_refusal.take();
        let last_markmesh_exec_skip = self.cam.last_markmesh_exec_skip;
        let markmesh_missing_58 = self.cam.markmesh_missing_58;
        let last_glassmesh_exec_skip = self.cam.last_glassmesh_exec_skip;
        let last_glassmesh_refusal = self.cam.last_glassmesh_refusal.take();
        let last_mark_packed_custom = self.cam.last_mark_packed_custom;
        let last_mark_packed_scene_light = self.cam.last_mark_packed_scene_light;
        let last_mark_lmap_sampler = self.cam.last_mark_lmap_sampler;
        let last_glass_packed_probe = self.cam.last_glass_packed_probe;
        let last_glass_probe_sampler = self.cam.last_glass_probe_sampler;
        let markmesh_hits = self.cam.markmesh_hits;
        let glassmesh_hits = self.cam.glassmesh_hits;
        let prepared_hits = self.cam.prepared_hits;
        let prepare_cost = std::mem::take(&mut self.cam.prepare_cost);
        let colour_run_census = self.cam.colour_run_census;
        let focused_object_id = self.cam.focused_object_id;
        let viewmodel_held = self.cam.viewmodel_held;
        let needs_floatz = self.cam.needs_floatz;
        let needs_resolved_scene = self.cam.needs_resolved_scene;
        let _world_ib_skip = self.cam.world_ib_skip;
        let mut prepared = std::mem::take(&mut self.scratch.prepared);
        let submitted_keys = &self.scratch.submitted_keys;
        let world_exec_ready_keys = &self.scratch.world_exec_ready_keys;
        let mut floatz_blit = 0u32;
        let mut resolved_scene_copy = 0u32;
        let mut focused_drawn_passes = 0u32;

        let sun_shadow_produced = sun_submit.gpu != 0;
        let spot_shadow_produced = spot_submit.slots != 0 || spot_submit.gpu != 0;
        let held = retain_camera_draws_with_sun_content(
            &mut prepared,
            sun_shadow_produced,
            spot_shadow_produced,
        );
        if held > 0 {
            refused_draws = refused_draws.saturating_add(held);
            refusals.note_submit_class("colour", "ProductDependencyNotReady", held);
            last_refusal = Some(GpuSubmitRefusal::ProductDependencyNotReady {
                product: FrameProductKind::SunShadow,
            });
        }
        if census_on {
            accounting.census.frame.pack_intern_hit_n = Some(prepare_cost.intern_hit_n);
            accounting.census.frame.pack_intern_miss_n = Some(prepare_cost.intern_miss_n);
            accounting.census.frame.gpu_exec_reuse_n = Some(0);
            accounting.census.frame.gpu_exec_unique_n = Some(0);
            accounting.census.frame.pack_overlay_n = Some(0);
            accounting.census.frame.pack_overlay_row_n = Some(0);
            accounting.census.frame.pack_overlay_pixel_share_n = Some(0);
            accounting.census.frame.pack_seed_n = Some(prepare_cost.pack_seed_n);
            accounting.census.frame.pack_walk_n = Some(prepare_cost.pack_walk_n);
            accounting.census.frame.tex_bind_hit_n = Some(prepare_cost.tex_bind_hit_n);
            accounting.census.frame.tex_bind_miss_n = Some(prepare_cost.tex_bind_miss_n);
            accounting.census.frame.markmesh_hits = Some(as_u32(markmesh_hits));
            accounting.census.frame.last_markmesh_refusal =
                last_markmesh_refusal.map(|cause| submit_refusal_class(&cause).to_owned());
            accounting.census.frame.last_markmesh_exec_skip =
                last_markmesh_exec_skip.map(str::to_owned);
            accounting.census.frame.markmesh_missing_58 = Some(markmesh_missing_58);
            accounting.census.frame.glassmesh_hits = Some(as_u32(glassmesh_hits));
            accounting.census.frame.last_glassmesh_exec_skip =
                last_glassmesh_exec_skip.map(str::to_owned);
            accounting.census.frame.last_glassmesh_refusal =
                last_glassmesh_refusal.map(|cause| submit_refusal_class(&cause).to_owned());
            accounting.census.frame.last_glass_packed_probe = last_glass_packed_probe;
            accounting.census.frame.last_glass_probe_sampler = last_glass_probe_sampler;
            accounting.census.frame.last_mark_packed_custom = last_mark_packed_custom;
            accounting.census.frame.last_mark_packed_scene_light = last_mark_packed_scene_light;
            accounting.census.frame.last_mark_lmap_sampler = last_mark_lmap_sampler;
        }

        let mut gpu_indexed = 0u32;
        let mut bsp_drawn_surfaces = [0u32; 4];
        let mut bsp_draw_refused_surfaces = [0u32; 4];
        if !prepared.is_empty() {
            coalesce_exact_draws(&mut prepared);

            self.indirect.publish(
                &mut prepared,
                &bindings.device,
                &bindings.adapter,
                &bindings.queue,
            );
            let exec_frame = &extracted.frame.exec_frame;
            let Some(effect_depth) = self.floatz.effect_view().cloned() else {
                return;
            };
            let colour_input = ColourPassInputs {
                effect_depth: &effect_depth,
                device: &bindings.device,
                depth,
                viewport: extracted_view.viewport.as_vec4().to_array(),
                geometry: geometry::record_geometry(
                    &source.geometry,
                    &source.smodel_cache_gpu,
                    RecordMesh {
                        vertex: smodel_skinned_vertex,
                        lighting: smodel_skinned_vertex_lighting,
                        index: smodel_skinned_index,
                    },
                    &source.pretess,
                ),
                indirect_args: self.indirect.buffer(),
                registry: &bindings.registry,
                constants: bindings
                    .constant_arena
                    .gpu
                    .each_ref()
                    .map(|arena| arena.bind_group.as_ref()),
                focused_object_id,
                portal_level: None,
                exit_clip: None,
                scissor: None,
            };
            for draw in &prepared {
                if draw.bsp_counted
                    && let Some(kind) = draw.bsp_kind
                {
                    let lane = bsp_kind_index(kind);
                    if census_on {
                        accounting.census.frame.bsp_submitted_surfaces[lane] =
                            accounting.census.frame.bsp_submitted_surfaces[lane]
                                .saturating_add(u32::from(draw.bsp_surf_count));
                    }
                }
            }
            if census_on {
                accounting.census.frame.gpu_prepared = Some(prepared_hits);
                accounting.census.frame.gpu_world_ready = Some(as_u32(
                    prepared
                        .iter()
                        .filter(|draw| draw.tess == ExactTessBind::World)
                        .count(),
                ));
                accounting.census.frame.gpu_smodel_ready = Some(as_u32(
                    prepared
                        .iter()
                        .filter(|draw| {
                            draw.tess == ExactTessBind::Smodel
                                || draw.tess == ExactTessBind::SmodelCached
                                || draw.tess == ExactTessBind::SmodelSkinned
                        })
                        .count(),
                ));
                accounting.census.frame.gpu_xmodel_ready = Some(as_u32(
                    prepared
                        .iter()
                        .filter(|draw| draw.tess == ExactTessBind::XModel)
                        .count(),
                ));
            }
            let table_layout = bindings.texture_layout();
            let table_binds = self.texture_table.0.each_mut().map(|table| {
                &table
                    .binds(&bindings.device, &bindings.registry, table_layout)
                    .scene
            });

            let diagnostics = context.diagnostic_recorder();
            let diagnostics = diagnostics.as_deref();
            let encoder = context.command_encoder();
            let record_started = colour_census_clock(census_on);
            let mut pass_end_ms = 0.0f32;
            let mut pass_end_n = 0u32;
            let mut encode_not_ready = 0u32;
            let gpu_span = diagnostics.time_span(encoder, GPU_SPAN_COLOUR);
            self.floatz.clear_effect_depth(encoder);
            let mut resolve_failures = 0u32;
            let mut resolve = |encoder: &mut bevy::render::render_resource::CommandEncoder| {
                if needs_floatz {
                    let span = diagnostics
                        .time_span(encoder, crate::diag::render_frame_diag::GPU_SPAN_FLOATZ);
                    let ready = floatz::resolve_depth(
                        encoder,
                        &bindings.cache,
                        &bindings.device,
                        &self.floatz,
                    );
                    span.end(encoder);
                    if !ready {
                        resolve_failures += 1;
                        return false;
                    }
                    floatz_blit = 1;
                    self.floatz.resolved_frame = Some(products.0.frame_id);
                }
                if needs_resolved_scene {
                    self.resolved_scene.copy(encoder, target.main_texture());
                    resolved_scene_copy = 1;
                }
                true
            };
            let output = super::portal_scene::PortalScene {
                input: &colour_input,
                target,
                binds: table_binds,
                draws: &prepared,
                samples: _msaa.map_or(1, |m| m.samples()),
                diagnostics,
            }
            .record(encoder, exec_frame, &mut resolve);
            if needs_floatz {
                let span =
                    diagnostics.time_span(encoder, crate::diag::render_frame_diag::GPU_SPAN_FLOATZ);
                if floatz::resolve_depth(encoder, &bindings.cache, &bindings.device, &self.floatz) {
                    floatz_blit = 1;
                    self.floatz.resolved_frame = Some(products.0.frame_id);
                } else {
                    resolve_failures += 1;
                }
                span.end(encoder);
            }
            gpu_span.end(encoder);
            if resolve_failures > 0 {
                refusals.note_submit_class("resolve", "FloatZBlitNotReady", resolve_failures);
                refused_draws = refused_draws.saturating_add(resolve_failures);
                last_refusal = Some(GpuSubmitRefusal::ProductDependencyNotReady {
                    product: FrameProductKind::FloatZ,
                });
            }
            gpu_indexed = gpu_indexed.saturating_add(output.indexed);
            refused_draws = refused_draws.saturating_add(output.refused_draws);
            encode_not_ready = encode_not_ready.saturating_add(output.encode_not_ready);
            last_refusal = output.last_refusal.or(last_refusal);
            focused_drawn_passes = focused_drawn_passes.saturating_add(output.focused_drawn);
            record_n.add(output.bindings);
            for lane in 0..3 {
                bsp_drawn_surfaces[lane] =
                    bsp_drawn_surfaces[lane].saturating_add(output.bsp_drawn_surfaces[lane]);
                bsp_draw_refused_surfaces[lane] = bsp_draw_refused_surfaces[lane]
                    .saturating_add(output.bsp_draw_refused_surfaces[lane]);
            }
            pass_end_ms += output.drop_ms;
            pass_end_n += 1;
            if census_on {
                if pass_end_n > 0 {
                    accounting.census.frame.pass_end_ms = Some(pass_end_ms);
                }
                accounting.census.frame.submit_record_ms = colour_census_ms(record_started);
                accounting.census.frame.encoder_finish_ms = Some(0.0);
            }
            if encode_not_ready > 0 {
                pipeline_not_ready = pipeline_not_ready.saturating_add(encode_not_ready);
                refusals.note_submit_class("encode", "PipelineNotReady", encode_not_ready);
            }
        }
        *accounting.working_set = crate::ColourWorkingSet {
            hits: ready_draws.saturating_add(refused_draws),
            pipeline_not_ready,
        };

        if census_on {
            accounting.census.preparation.ready_draws = ready_draws;
            accounting.census.frame.bsp_submit_refused_surfaces = bsp_submit_refused_surfaces;
            accounting.census.frame.bsp_drawn_surfaces = bsp_drawn_surfaces;
            accounting.census.frame.bsp_draw_refused_surfaces = bsp_draw_refused_surfaces;
            accounting.census.preparation.refused_draws = refused_draws;
            accounting.census.preparation.gpu_ready = Some(gpu_indexed);
            accounting.census.frame.set_bind_group_n = Some(record_n.total());
            accounting.census.frame.set_state_n = Some(record_n.state);
            accounting.census.frame.multi_draw_n = Some(record_n.multi_draws);
            accounting.census.frame.multi_draw_commands_n = Some(record_n.multi_draw_commands);
            accounting.census.preparation.set_bind_group0_n = Some(record_n.group0);
            accounting.census.preparation.set_bind_group1_n = Some(record_n.group1);
            accounting.census.preparation.material_runs_n = Some(colour_run_census.material_runs);
            accounting.census.preparation.pass_setups_n = Some(colour_run_census.pass_setups);
            accounting.census.preparation.obj_binds_n = Some(colour_run_census.obj_binds);
            accounting.census.preparation.shell_hits_n = Some(colour_run_census.shell_hits);
            accounting.census.preparation.shell_misses_n = Some(colour_run_census.shell_misses);
            accounting.census.preparation.overlay_const_writes_n =
                Some(colour_run_census.overlay_const_writes);
            accounting.census.preparation.overlay_need_known_n =
                Some(colour_run_census.overlay_need_known);
            accounting.census.frame.smodel_reuse_n = Some(0);
            accounting.census.frame.xmodel_reuse_n = Some(0);
            if accounting.census.frame.gpu_prepared.is_none() {
                accounting.census.frame.gpu_prepared = Some(prepared_hits);
                accounting.census.frame.gpu_world_ready = Some(0);
                accounting.census.frame.gpu_smodel_ready = Some(0);
                accounting.census.frame.gpu_xmodel_ready = Some(0);
            }
            if accounting.census.frame.pack_arena_share_n.is_none() {
                accounting.census.frame.pack_arena_share_n = Some(0);
                accounting.census.frame.pack_arena_vertex_n = Some(0);
                accounting.census.frame.pack_arena_pixel_n = Some(0);
            }
            if accounting.census.frame.encoder_finish_ms.is_none() {
                accounting.census.frame.encoder_finish_ms = Some(0.0);
            }
            if accounting.census.frame.submit_arena_ms.is_none() {
                accounting.census.frame.submit_arena_ms = Some(0.0);
            }
            if accounting.census.frame.submit_record_ms.is_none() {
                accounting.census.frame.submit_record_ms = Some(0.0);
            }
            let mut ranked: Vec<_> = refusals.submit.iter().collect();
            ranked.sort_by(|a, b| b.1.cmp(a.1).then(a.0.cmp(b.0)));
            accounting.census.frame.submit_cause = ranked
                .first()
                .map(|((family, cause), n)| format!("{family}:{cause}:{n}"));
            accounting.census.frame.submit_cause2 = ranked
                .get(1)
                .map(|((family, cause), n)| format!("{family}:{cause}:{n}"));
            accounting.census.frame.gpu_not_ready_n = Some(
                refusals
                    .submit
                    .iter()
                    .filter(|((_, cause), _)| *cause == "PipelineNotReady")
                    .map(|(_, n)| *n)
                    .sum(),
            );
            accounting.census.frame.gpu_no_port_n = Some(
                refusals
                    .submit
                    .iter()
                    .filter(|((_, cause), _)| *cause == "NoExactPort")
                    .map(|(_, n)| *n)
                    .sum(),
            );
            accounting.census.frame.pnr_smodel_mat = rank_count_map(&pipeline_wait.smodel_mats, 8);
            accounting.census.frame.pnr_world_mat = rank_count_map(&pipeline_wait.world_mats, 8);
            accounting.census.frame.pnr_smodel_ps = rank_count_map(&pipeline_wait.smodel_ps, 8);
            accounting.census.frame.pnr_world_ps = rank_count_map(&pipeline_wait.world_ps, 8);
            accounting.census.frame.pnr_smodel_key_n =
                Some(as_u32(pipeline_wait.smodel_keys.len()));
            accounting.census.frame.pnr_world_key_n = Some(as_u32(pipeline_wait.world_keys.len()));
            accounting.census.frame.pnr_port_n = Some(as_u32(pipeline_wait.ports.len()));
            accounting.census.frame.gpu_smodel_bind_mat = rank_count_map(&bind_smodel_mats, 8);
        }
        let focused_prepared_surfaces = focused_object_id.map_or(0, |object_id| {
            submitted_keys
                .iter()
                .filter(|key| drawsurf_object_id(**key) == object_id)
                .count() as u32
        });
        let focused_prepared_passes = focused_object_id.map_or(0, |object_id| {
            prepared
                .iter()
                .filter(|draw| draw.owner_object_id == Some(object_id))
                .count() as u32
        });
        accounting.focus_submit.emit(
            products,
            Some((
                &extracted.world.sorted_material_names,
                &extracted.world.image_handles,
            )),
            exec_tables(extracted),
            FocusedOwnerSubmitReport {
                terminal: "complete",
                prepared_surfaces: focused_prepared_surfaces,
                prepared_passes: focused_prepared_passes,
                drawn_passes: focused_drawn_passes,
            },
        );
        accounting.publish_coverage(submitted_keys, world_exec_ready_keys);

        let xmodel_draws = prepared
            .iter()
            .filter(|d| d.tess == ExactTessBind::XModel)
            .count();
        let codemesh_draws = prepared
            .iter()
            .filter(|d| d.tess == ExactTessBind::CodeMesh)
            .count();
        let markmesh_draws = prepared
            .iter()
            .filter(|d| d.tess == ExactTessBind::MarkMesh)
            .count();
        let glassmesh_draws = prepared
            .iter()
            .filter(|d| d.tess == ExactTessBind::Glass)
            .count();
        if census_on {
            accounting.census.frame.markmesh_prepared = Some(as_u32(markmesh_draws));
            accounting.census.frame.glassmesh_prepared = Some(as_u32(glassmesh_draws));
            accounting.census.preparation.log_frame =
                accounting.census.preparation.log_frame.wrapping_add(1);
            if accounting.census.preparation.log_frame == 1
                || accounting.census.preparation.log_frame.is_multiple_of(64)
            {
                let mut authored_state = AuthoredStateCensus::default();
                for draw in &prepared {
                    authored_state.note(draw.state.authored_host_fields());
                }
                let message = format!(
                    "drawsurf production gpu submit: material_runs={} pass_setups={} obj_binds={} shell_hits={} shell_misses={} overlay_const_writes={} overlay_need_known={} ready_draws={ready_draws} refused_draws={refused_draws} exec_refused={exec_refused} authored_state={authored_state:?} unsupported_state={unsupported_state:?} exec_causes={} submit_cause={} submit_cause2={} prepared={} xmodel_prepared={xmodel_draws} codemesh_prepared={codemesh_draws} markmesh_prepared={markmesh_draws} glassmesh_prepared={glassmesh_draws} floatz_blit={floatz_blit} resolved_scene_copy={resolved_scene_copy} viewmodel_held={viewmodel_held} scene_tables(before_linear,before_srgb,after_linear,after_srgb)(2d,cube,3d,samplers)={:?} texture_table_rebuilds={} shadow_table={:?} cached_slot_words={} tex_bind(hit,miss)=({},{}) constant_bind_groups={} indirect(folded,batches,uploaded_words)=({},{},{}) last={last_refusal:?}",
                    colour_run_census.material_runs,
                    colour_run_census.pass_setups,
                    colour_run_census.obj_binds,
                    colour_run_census.shell_hits,
                    colour_run_census.shell_misses,
                    colour_run_census.overlay_const_writes,
                    colour_run_census.overlay_need_known,
                    rank_pair_map(&refusals.exec, 2)
                        .as_deref()
                        .unwrap_or("none"),
                    accounting
                        .census
                        .frame
                        .submit_cause
                        .as_deref()
                        .unwrap_or("none"),
                    accounting
                        .census
                        .frame
                        .submit_cause2
                        .as_deref()
                        .unwrap_or("none"),
                    prepared.len(),
                    self.texture_table
                        .0
                        .each_ref()
                        .map(ExactTextureTable::census),
                    self.texture_table
                        .iter()
                        .map(|table| table.rebuild_n)
                        .sum::<u32>(),
                    shadow.shadow_table.census(),
                    bindings.binding_cache.interned_n() + shadow.shadow_binding.textures.len(),
                    prepare_cost.tex_bind_hit_n,
                    prepare_cost.tex_bind_miss_n,
                    bindings
                        .constant_arena
                        .gpu
                        .iter()
                        .filter(|arena| arena.bind_group.is_some())
                        .count(),
                    self.indirect.folded,
                    self.indirect.batches,
                    self.indirect.uploaded_words,
                );
                if refused_draws != 0 || exec_refused != 0 {
                    diag::warn!(World, "{message}");
                } else {
                    diag::info!(World, "{message}");
                }
            }
        }
        self.scratch.prepared = prepared;
    }
}

pub(super) fn draw_exact_colour(
    view: ViewQuery<(
        &ViewTarget,
        &SceneDepthTexture,
        &ExtractedView,
        Option<&Msaa>,
    )>,
    source: ColourFrameSource,
    bindings: ColourPassBindings,
    shadow: ColourShadowRecorder,
    accounting: ColourSubmitAccounting,
    recorder: ColourPassRecorder,
    context: RenderContext,
) {
    recorder.record(view, source, bindings, shadow, accounting, context);
}
