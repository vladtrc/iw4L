use crate::WorldPipelineWarmup;

use super::*;

pub fn cached_lighting_port_variant(
    port: &crate::AdmittedExactPort,
    packed_buffers: &[VertexBufferLayout],
) -> (Option<String>, Vec<VertexBufferLayout>) {
    if port.abi().vertex_type != asset_iw4::vertex_decl::PACKED_VERTEX_TYPE {
        return (None, Vec::new());
    }
    let Some(location) = cached_lighting_location(port.abi()) else {
        return (None, Vec::new());
    };
    let buffers = cached_lighting_vertex_buffers(packed_buffers, location);
    let source = lighting_texcoord_register(port.abi()).and_then(|register| {
        inject_cached_lighting_attribute(&port.module().source, location, Some(&register))
    });
    (source, buffers)
}

fn adapt_admitted_port(port: &crate::AdmittedExactPort) -> ExactColourPortGpu {
    let (constants, textures) = split_bind_layout(port.wgpu_layout());
    let vertex_buffers = crate::vertex_layouts_from_contract(port.wgpu_layout());
    let (cached_source, cached_vertex_buffers) =
        cached_lighting_port_variant(port, &vertex_buffers);

    let cached_source = cached_source
        .filter(|_| !cached_vertex_buffers.is_empty())
        .map(Arc::<str>::from);
    ExactColourPortGpu {
        constants_layout: crate::bind_group_layout_from_entries(
            "iw4_exact_colour_constants",
            &constants,
            true,
        ),
        textures_layout: crate::bind_group_layout_from_entries(
            "iw4_exact_colour_textures",
            &textures,
            true,
        ),
        vertex_buffers,
        cached_source,
        clipped_sources: std::array::from_fn(|_| std::sync::OnceLock::new()),
        cached_vertex_buffers,
        port: port.clone(),
    }
}

pub(super) fn init_or_update_pipeline(
    world: Option<Res<InstalledRenderWorld>>,
    mut pipeline: ResMut<ExactColourPipeline>,
) {
    let Some(world) = world else {
        return;
    };
    if world.ports.is_empty() {
        if !pipeline.ports.is_empty() {
            pipeline.ports.clear();
            pipeline.by_id.clear();
        }
        return;
    }
    if pipeline.generation == world.generation && pipeline.ports.len() == world.ports.len() {
        return;
    }
    pipeline.ports = world.ports.iter().map(adapt_admitted_port).collect();
    pipeline.generation = world.generation;
    pipeline.rebuild_index();
}

pub(super) fn kick_extracted_colour_pipelines(
    frame: Option<Res<PublishedRenderFrame>>,
    views: Query<(&ViewTarget, Option<&Msaa>), (With<Camera3d>, With<ViewUpscalingPipeline>)>,
    device: Res<RenderDevice>,
    mut registry: ResMut<ExactPipelineRegistry>,
    pipeline: Res<ExactColourPipeline>,
    mut kicked: ResMut<ExactPipelineKickCache>,
    mut warmup: ResMut<WorldPipelineWarmup>,
) {
    let extracted = frame.as_deref().map(ExtractedColourRefs::new);
    registry.poll();

    for key in registry.take_discovered() {
        if pipeline.get(key.port).is_some() {
            request_exact_pipeline(&mut registry, &pipeline, &device, key);
        }
    }
    kick_admitted_pipelines(
        extracted,
        &views,
        &device,
        &mut registry,
        &pipeline,
        &mut kicked,
        &mut warmup,
    );
    registry.flush(&device);
}

pub(super) fn kick_admitted_pipelines(
    extracted: Option<ExtractedColourRefs<'_>>,
    views: &Query<(&ViewTarget, Option<&Msaa>), (With<Camera3d>, With<ViewUpscalingPipeline>)>,
    device: &RenderDevice,
    registry: &mut ExactPipelineRegistry,
    pipeline: &ExactColourPipeline,
    kicked: &mut ExactPipelineKickCache,
    warmup: &mut WorldPipelineWarmup,
) {
    let Some(extracted) = extracted else {
        return;
    };

    let Some((_, prepared_table)) = exec_tables(extracted) else {
        return;
    };

    let view_sig: Vec<(TextureFormat, u32)> = if views.is_empty() {
        vec![crate::LENS_VIEW_SIGNATURE]
    } else {
        views
            .iter()
            .map(|(target, msaa)| (target.main_texture_format(), msaa.map_or(1, Msaa::samples)))
            .fold(Vec::new(), |mut signatures, signature| {
                // Pipelines depend on the attachment signature, not the number
                // of cameras using it (for example, an outgoing transition view).
                if !signatures.contains(&signature) {
                    signatures.push(signature);
                }
                signatures
            })
    };
    let generation_changed = kicked.generation != Some(extracted.world.generation);
    let views_changed = kicked.views != view_sig;
    let portal_mode = extracted
        .frame
        .exec_frame
        .portal
        .as_ref()
        .map(|_| super::portal_scene::exit_clip_mode(device));
    let demand_changed = kicked.demand_revision != extracted.frame.pipeline_demand_revision
        || kicked.portal_mode != portal_mode;
    if generation_changed || views_changed {
        if kicked.generation == Some(extracted.world.generation)
            && kicked.views == [crate::LENS_VIEW_SIGNATURE]
        {
            diag::warn!(
                World,
                "exact pipelines: lens view is {:?}, spawn_world declares {:?}; {} slots were walked for the wrong target",
                view_sig,
                crate::LENS_VIEW_SIGNATURE,
                kicked.current.len()
            );
        }

        kicked.current.clear();
        kicked.scheduled.clear();
        kicked.generation = Some(extracted.world.generation);
        kicked.views = view_sig.clone();
        kicked.schedule_admitted_pipelines(
            extracted,
            prepared_table,
            &view_sig,
            registry,
            pipeline,
            device,
        );
        kicked.demand_revision = extracted.frame.pipeline_demand_revision;
        kicked.portal_mode = portal_mode;
        diag::info!(
            World,
            "exact pipelines: working set={} modules held={} tasks in flight={}",
            kicked.current.len(),
            registry.module_n(),
            registry.building_n()
        );
    } else if demand_changed {
        let before = kicked.current.len();
        kicked.schedule_admitted_pipelines(
            extracted,
            prepared_table,
            &view_sig,
            registry,
            pipeline,
            device,
        );
        kicked.demand_revision = extracted.frame.pipeline_demand_revision;
        kicked.portal_mode = portal_mode;
        diag::info!(
            World,
            "exact pipelines: demand revision={} added={} working set={}",
            extracted.frame.pipeline_demand_revision,
            kicked.current.len().saturating_sub(before),
            kicked.current.len()
        );
    }
    *warmup = WorldPipelineWarmup {
        initialized: true,
        total: kicked.current.len() as u32,
        ready: kicked
            .current
            .iter()
            .filter(|slot| registry.is_ready(**slot))
            .count() as u32,
    };
}

impl ExactPipelineKickCache {
    fn schedule_admitted_pipelines(
        &mut self,
        extracted: ExtractedColourRefs<'_>,
        prepared_table: &render_material::PreparedMaterialTable,
        view_sig: &[(TextureFormat, u32)],
        registry: &mut ExactPipelineRegistry,
        pipeline: &ExactColourPipeline,
        device: &RenderDevice,
    ) {
        let scheduled = &mut self.scheduled;
        let current = &mut self.current;
        for (main_format, samples) in view_sig.iter().copied() {
            let techs = lighting_iw4::LIT_TECH_NO_SHADOW_DIR_SLOTS
                .into_iter()
                .chain(lighting_iw4::LIT_TECH_NO_SHADOW_LOCAL_SLOTS)
                .chain(lighting_iw4::LIT_TECH_SHADOW_DIR_SLOTS)
                .chain(lighting_iw4::LIT_TECH_SHADOW_SPOT_SLOTS)
                .chain([super::super::SUN_SHADOW_CASTER_TECH, 5]);
            for tech in techs {
                let shadow = tech == super::super::SUN_SHADOW_CASTER_TECH;
                for (material, prepared_pass) in
                    prepared_table.passes_for_tech(super::super::TechType(tech))
                {
                    let state =
                        super::super::state::GfxPassState::from_prepared(prepared_pass.state);
                    let state0 = state.apply_change_state_0_host(false);
                    let state1 = state.apply_change_state_1_host();
                    let target = exact_colour_target_format(
                        if shadow {
                            SHADOWMAP_SUN_COLOR_FORMAT
                        } else {
                            main_format
                        },
                        state0.srgb_write,
                    );
                    for port in prepared_pass.port.iter().flatten() {
                        use asset_iw4::vertex_decl::{
                            PACKED_VERTEX_TYPE, STATICMODELCACHE_VERTEX_TYPE,
                        };
                        if tech != 5 && port.vertex_type != PACKED_VERTEX_TYPE {
                            let used = if port.vertex_type == STATICMODELCACHE_VERTEX_TYPE {
                                extracted
                                    .frame
                                    .pipeline_smodel_materials
                                    .contains(&material.0)
                            } else {
                                extracted
                                    .frame
                                    .pipeline_world_materials
                                    .contains(&material.0)
                            };
                            if !used {
                                continue;
                            }
                        }
                        let key = ExactColourPipelineKey {
                            target,
                            depth_format: if shadow {
                                SHADOWMAP_SUN_DEPTH_FORMAT
                            } else {
                                SCENE_DEPTH_FORMAT
                            },
                            samples: if shadow { 1 } else { samples },
                            state0,
                            state1,
                            port: *port,
                            cached_lighting: false,
                            mark_mesh: false,
                            forward_z: shadow,
                            portal_stencil: false,
                            exit_clip: None,
                        };
                        if scheduled.insert(key) {
                            current.push(request_exact_pipeline(registry, pipeline, device, key));
                        }
                        // A foreign world first appears with exit clipping, then
                        // becomes the unclipped root after a crossing. Both use
                        // portal stencil state, unlike the ordinary warmup key.
                        // Compile both before releasing the loading screen.
                        // Donor materials from other games are admitted too, but
                        // are not geometry of this scene and need no portal variant.
                        let scene_material = extracted
                            .frame
                            .pipeline_world_materials
                            .contains(&material.0)
                            || extracted
                                .frame
                                .pipeline_smodel_materials
                                .contains(&material.0);
                        if !shadow && scene_material && extracted.frame.exec_frame.portal.is_some()
                        {
                            for exit_clip in
                                [None, Some(super::portal_scene::exit_clip_mode(device))]
                            {
                                let portal_key = ExactColourPipelineKey {
                                    portal_stencil: true,
                                    exit_clip,
                                    ..key
                                };
                                if scheduled.insert(portal_key) {
                                    current.push(request_exact_pipeline(
                                        registry, pipeline, device, portal_key,
                                    ));
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}
