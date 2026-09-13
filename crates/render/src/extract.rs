use std::sync::Arc;
use std::time::Instant;

use bevy::prelude::*;
use bevy::render::Extract;
use render_frontend::assemble::drawsurf::tess::fx::FxCodeMeshPlan;
use render_frontend::assemble::drawsurf::tess::glass::GfxGlassMeshPlan;
use render_frontend::assemble::drawsurf::tess::mark::GfxMarkMeshPlan;
use render_frontend::assemble::drawsurf::tess::particle_cloud::FxParticleCloudPlan;
use render_frontend::assemble::drawsurf::tess::smodel::SmodelGpuPlan;
use render_frontend::assemble::drawsurf::tess::world::WorldDrawGpuPlan;
use render_frontend::assemble::drawsurf::tess::xmodel::XModelDrawPlan;
use render_gpu::diag::render_frame_diag::SharedRenderStagesSlot;
use render_gpu::{
    ExtractedExactColour, ExtractedRenderFrameProducts, ExtractedRuntimeImageHandles,
    RetailSamplerTable,
};

fn extract_share<T: Clone>(share: Option<&Arc<Vec<T>>>, live: &[T]) -> (Arc<Vec<T>>, u32) {
    match share {
        Some(arc) => (Arc::clone(arc), 1),
        None => (Arc::new(live.to_vec()), 0),
    }
}

fn tess_extract_clone_bytes<V, I>(
    v_arc: u32,
    i_arc: u32,
    verts: &Arc<Vec<V>>,
    inds: &Arc<Vec<I>>,
    ranges: &[(u32, u32)],
) -> u64 {
    let mut n = std::mem::size_of_val(ranges) as u64;
    if v_arc == 0 {
        n += std::mem::size_of_val(verts.as_slice()) as u64;
    }
    if i_arc == 0 {
        n += std::mem::size_of_val(inds.as_slice()) as u64;
    }
    n
}

fn world_colour_extract_counts(plan: Option<&WorldDrawGpuPlan>) -> (usize, usize, usize) {
    match plan {
        Some(plan) => match plan.exact_retail_vertices() {
            Ok(vertices) => (vertices.len(), plan.indices.len(), plan.vertex_layer.len()),
            Err(_) => (0, 0, 0),
        },
        None => (0, 0, 0),
    }
}

fn smodel_colour_extract_counts(plan: Option<&SmodelGpuPlan>) -> (usize, usize) {
    match plan {
        Some(plan) => match plan.exact_packed_vertices() {
            Ok(vertices) => (vertices.len(), plan.indices.len()),
            Err(_) => (0, 0),
        },
        None => (0, 0),
    }
}

pub fn extract_exact_colour(
    mut commands: Commands,
    material: (
        Extract<Option<Res<render_frontend::assemble::drawsurf::MaterialGeneration>>>,
        Extract<Option<Res<render_frontend::assemble::drawsurf::MaterialFrameInputs>>>,
        Extract<Option<Res<render_frontend::assemble::drawsurf::FrameAssemblyInputs>>>,
        Extract<Option<Res<render_frontend::assemble::drawsurf::RenderFrameProducts>>>,
        ResMut<render_gpu::FocusedOwnerSubmitState>,
    ),
    world: Extract<Option<Res<WorldDrawGpuPlan>>>,
    smodel: Extract<Option<Res<SmodelGpuPlan>>>,
    smc: Extract<
        Option<Res<render_frontend::prepare::scene::smodel_geom_cache::WorldStaticModelCache>>,
    >,
    static_identity: (
        Extract<Option<Res<render_frontend::assemble::drawsurf::StaticDrawLane>>>,
        Extract<Option<Res<frame::WorldGeneration>>>,
    ),
    xmodel: Extract<Option<Res<XModelDrawPlan>>>,
    fx: Extract<Option<Res<FxCodeMeshPlan>>>,
    particle_cloud: Extract<Option<Res<FxParticleCloudPlan>>>,
    mark_mesh: Extract<Option<Res<GfxMarkMeshPlan>>>,
    glass_mesh: Extract<Option<Res<GfxGlassMeshPlan>>>,
    samplers: Extract<Option<Res<RetailSamplerTable>>>,
    images: Extract<Option<Res<render_frontend::assemble::drawsurf::RuntimeImageHandles>>>,
    spawn_job: Extract<Option<Res<render_gpu::GpuSubmitReady>>>,
    mut existing: Option<ResMut<ExtractedExactColour>>,
    slot: Option<Res<SharedRenderStagesSlot>>,
) {
    let (retained, world_generation) = static_identity;
    let (runtime, mat_frame, assembly, products, mut focus_submit) = material;
    if let Some(existing) = existing.as_ref() {
        render_gpu::emit_focused_owner_submit(
            &existing.frame_products,
            &mut focus_submit,
            None,
            None,
            "render_not_scheduled",
            0,
            0,
            0,
        );
    }
    let Some(runtime) = runtime.as_ref() else {
        commands.insert_resource(ExtractedExactColour::default());
        return;
    };
    let Some(mat_frame) = mat_frame.as_ref() else {
        commands.insert_resource(ExtractedExactColour::default());
        return;
    };
    let generation = runtime.catalog.generation_id;
    let world_generation = world_generation
        .as_ref()
        .map(|generation| **generation)
        .unwrap_or(frame::WorldGeneration(None));
    let Some(products) = products.as_ref() else {
        commands.insert_resource(ExtractedExactColour::default());
        return;
    };
    let product_started = Instant::now();
    let snapshot = products.published();
    let coherent =
        frame_submission_matches(assembly.as_deref(), &snapshot, generation, world_generation);
    if !coherent {
        commands.insert_resource(ExtractedExactColour::default());
        return;
    }
    if let Some(slot) = slot.as_ref() {
        slot.stamp_extract_products(
            product_started.elapsed().as_secs_f32() * 1000.0,
            1,
            u32::from(products.last_bank_new),
        );
    }
    let frame_products = ExtractedRenderFrameProducts(snapshot);
    let cpu_port_len = runtime.programs.ports().len();
    let skip_ports = existing.as_ref().is_some_and(|extracted| {
        render_gpu::colour_ports_static(
            extracted.generation,
            extracted.ports.len(),
            generation,
            cpu_port_len,
        )
    });
    let (ports, extract_ports_ms) = if skip_ports {
        (Vec::new(), 0.0_f32)
    } else {
        let ports_started = Instant::now();

        let ports: Vec<render_gpu::AdmittedExactPort> = runtime
            .programs
            .ports()
            .iter()
            .map(|port| render_gpu::AdmittedExactPort {
                id: port.id(),
                abi: port.abi().clone(),
                module: port.shared_module(),
                layout: port.wgpu_layout().clone(),
            })
            .collect();
        (ports, ports_started.elapsed().as_secs_f32() * 1000.0)
    };
    let tess_started = Instant::now();
    let warm_pipelines = spawn_job.as_ref().is_some_and(|job| job.warm_pipelines);
    let (world_v, world_i, world_layer_n) =
        world_colour_extract_counts(world.as_ref().map(|plan| &**plan));
    let (smodel_v, smodel_i) = smodel_colour_extract_counts(smodel.as_ref().map(|plan| &**plan));

    let skip_world_smodel = existing.as_ref().is_some_and(|extracted| {
        render_gpu::colour_world_smodel_static(
            extracted.generation,
            extracted.world_generation,
            extracted.static_geometry.world_vertices.len(),
            extracted.static_geometry.world_indices.len(),
            extracted.static_geometry.world_layer.len(),
            extracted.static_geometry.smodel_vertices.len(),
            extracted.static_geometry.smodel_indices.len(),
            generation,
            world_generation,
            world_v,
            world_i,
            world_layer_n,
            smodel_v,
            smodel_i,
        )
    });
    let smc_revision = smc.as_ref().map(|cache| cache.content_revision());
    let skip_smc_maps = skip_world_smodel
        && existing
            .as_ref()
            .is_some_and(|extracted| extracted.smc_revision == smc_revision);
    let (world_vertices, world_layer, world_indices, world_surface_ranges, world_vertex_refusal) =
        if skip_world_smodel {
            (Vec::new(), Vec::new(), Vec::new(), Vec::new(), None)
        } else {
            match world.as_ref() {
                Some(plan) => match plan.exact_retail_vertices() {
                    Ok(vertices) => (
                        vertices.to_vec(),
                        plan.vertex_layer.clone(),
                        plan.indices.clone(),
                        plan.surface_ranges.clone(),
                        None,
                    ),
                    Err(cause) => (Vec::new(), Vec::new(), Vec::new(), Vec::new(), Some(cause)),
                },
                None => (Vec::new(), Vec::new(), Vec::new(), Vec::new(), None),
            }
        };
    let (
        smodel_vertices,
        smodel_indices,
        smodel_surface_ranges,
        smodel_vertex_refusal,
        smodel_cached_vertices,
    ) = if skip_world_smodel {
        (Vec::new(), Vec::new(), Vec::new(), None, Vec::new())
    } else {
        match smodel.as_ref() {
            Some(plan) => match plan.exact_packed_vertices() {
                Ok(vertices) => (
                    vertices.to_vec(),
                    plan.indices.clone(),
                    plan.surface_ranges.clone(),
                    None,
                    plan.cached_vertices.clone(),
                ),
                Err(cause) => (Vec::new(), Vec::new(), Vec::new(), Some(cause), Vec::new()),
            },
            None => (Vec::new(), Vec::new(), Vec::new(), None, Vec::new()),
        }
    };
    let mut smc_vb_patches = Vec::new();
    let mut smc_ib_patches = Vec::new();
    let mut smc_index_baked = Vec::new();
    let smodel_pretess_indices = retained
        .as_ref()
        .map(|retained| Arc::clone(&retained.smodel_pretess_indices))
        .unwrap_or_else(|| Arc::new(Vec::new()));
    let smodel_index_layout_revision = retained
        .as_ref()
        .map(|retained| retained.smodel_index_layout_revision)
        .unwrap_or(0);
    if let Some(cache) = smc.as_ref() {
        smc_vb_patches = cache.vb_patches().to_vec();
        smc_ib_patches = cache.ib_patches().to_vec();
        if !skip_smc_maps {
            smc_index_baked = cache.baked_cache_indices();
        }
    }
    let (
        xmodel_vertices,
        xmodel_indices,
        xmodel_surface_ranges,
        xmodel_vertex_refusal,
        xmodel_arc,
        xmodel_i_arc,
        xmodel_r_arc,
    ) = match xmodel.as_ref() {
        Some(plan) => match plan.exact_packed_vertices() {
            Ok(vertices) => {
                let (verts, packed_arc) = if let Some(share) = plan.packed_share.clone() {
                    (share, 1u32)
                } else {
                    (Arc::new(vertices.to_vec()), 0u32)
                };
                let (inds, i_arc) = extract_share(plan.index_share.as_ref(), &plan.indices);
                let (ranges, r_arc) =
                    extract_share(plan.range_share.as_ref(), &plan.surface_ranges);
                (verts, inds, ranges, None, packed_arc, i_arc, r_arc)
            }
            Err(cause) => (
                Arc::new(Vec::new()),
                Arc::new(Vec::new()),
                Arc::new(Vec::new()),
                Some(cause),
                0u32,
                1u32,
                1u32,
            ),
        },
        None => (
            Arc::new(Vec::new()),
            Arc::new(Vec::new()),
            Arc::new(Vec::new()),
            None,
            0u32,
            1u32,
            1u32,
        ),
    };
    let xmodel_packed_segments = xmodel
        .as_ref()
        .map(|plan| plan.packed_segments)
        .unwrap_or_default();
    let xmodel_revision = xmodel.as_ref().map(|plan| plan.revision).unwrap_or(0);
    let xmodel_topology_revision = xmodel
        .as_ref()
        .map(|plan| plan.topology_revision)
        .unwrap_or(0);
    let (
        fx_vertices,
        fx_indices,
        fx_surface_ranges,
        fx_vertex_refusal,
        fx_revision,
        fx_v_arc,
        fx_i_arc,
    ) = match fx.as_ref() {
        Some(plan) => match plan.exact_packed_vertices() {
            Ok(_) => {
                let (verts, v_arc) = extract_share(plan.packed_share.as_ref(), &plan.vertices);
                let (inds, i_arc) = extract_share(plan.index_share.as_ref(), &plan.indices);
                (
                    verts,
                    inds,
                    plan.draws
                        .iter()
                        .map(|draw| (draw.index_start, draw.index_count))
                        .collect(),
                    None,
                    plan.revision,
                    v_arc,
                    i_arc,
                )
            }
            Err(cause) => (
                Arc::new(Vec::new()),
                Arc::new(Vec::new()),
                Vec::new(),
                Some(cause),
                plan.revision,
                1u32,
                1u32,
            ),
        },
        None => (
            Arc::new(Vec::new()),
            Arc::new(Vec::new()),
            Vec::new(),
            None,
            0,
            1u32,
            1u32,
        ),
    };
    let (particle_cloud_vertices, particle_cloud_indices, particle_cloud_surface_ranges) =
        match particle_cloud.as_ref() {
            Some(plan) => (
                Arc::clone(&plan.vertices),
                Arc::clone(&plan.indices),
                plan.draws
                    .iter()
                    .map(|draw| (draw.index_start, draw.index_count))
                    .collect(),
            ),
            None => (Arc::new(Vec::new()), Arc::new(Vec::new()), Vec::new()),
        };
    let (
        mark_mesh_vertices,
        mark_mesh_indices,
        mark_mesh_surface_ranges,
        mark_mesh_revision,
        mark_v_arc,
        mark_i_arc,
    ) = match mark_mesh.as_ref() {
        Some(plan) => {
            let (verts, v_arc) = extract_share(plan.packed_share.as_ref(), &plan.vertices);
            let (inds, i_arc) = extract_share(plan.index_share.as_ref(), &plan.indices);
            (
                verts,
                inds,
                plan.draws
                    .iter()
                    .map(|draw| (draw.index_start, draw.index_count))
                    .collect(),
                plan.revision,
                v_arc,
                i_arc,
            )
        }
        None => (
            Arc::new(Vec::new()),
            Arc::new(Vec::new()),
            Vec::new(),
            0,
            1u32,
            1u32,
        ),
    };
    let (
        glass_mesh_vertices,
        glass_mesh_indices,
        glass_mesh_surface_ranges,
        glass_mesh_vertex_refusal,
        glass_mesh_revision,
        glass_v_arc,
        glass_i_arc,
    ) = match glass_mesh.as_ref() {
        Some(plan) => match plan.exact_packed_vertices() {
            Ok(_) => {
                let (verts, v_arc) = extract_share(plan.packed_share.as_ref(), &plan.vertices);
                let (inds, i_arc) = extract_share(plan.index_share.as_ref(), &plan.indices);
                (
                    verts,
                    inds,
                    plan.draws
                        .iter()
                        .map(|draw| (draw.index_start, draw.index_count))
                        .collect(),
                    None,
                    plan.revision,
                    v_arc,
                    i_arc,
                )
            }
            Err(cause) => (
                Arc::new(Vec::new()),
                Arc::new(Vec::new()),
                Vec::new(),
                Some(cause),
                plan.revision,
                1u32,
                1u32,
            ),
        },
        None => (
            Arc::new(Vec::new()),
            Arc::new(Vec::new()),
            Vec::new(),
            None,
            0,
            1u32,
            1u32,
        ),
    };
    let extract_fx_arc = u32::from(
        fx_v_arc == 1
            && fx_i_arc == 1
            && mark_v_arc == 1
            && mark_i_arc == 1
            && glass_v_arc == 1
            && glass_i_arc == 1,
    );
    let extract_tess_ms = tess_started.elapsed().as_secs_f32() * 1000.0;
    let extract_world_clone_bytes = std::mem::size_of_val(world_vertices.as_slice()) as u64
        + world_layer.len() as u64
        + std::mem::size_of_val(world_indices.as_slice()) as u64
        + std::mem::size_of_val(world_surface_ranges.as_slice()) as u64
        + std::mem::size_of_val(smodel_vertices.as_slice()) as u64
        + std::mem::size_of_val(smodel_indices.as_slice()) as u64
        + std::mem::size_of_val(smodel_surface_ranges.as_slice()) as u64;
    let extract_xmodel_clone_bytes = {
        let mut n = 0u64;
        if xmodel_arc == 0 {
            n += std::mem::size_of_val(xmodel_vertices.as_slice()) as u64;
        }
        if xmodel_i_arc == 0 {
            n += std::mem::size_of_val(xmodel_indices.as_slice()) as u64;
        }
        if xmodel_r_arc == 0 {
            n += std::mem::size_of_val(xmodel_surface_ranges.as_slice()) as u64;
        }
        n
    };
    let extract_fx_clone_bytes = tess_extract_clone_bytes(
        fx_v_arc,
        fx_i_arc,
        &fx_vertices,
        &fx_indices,
        &fx_surface_ranges,
    ) + tess_extract_clone_bytes(
        1,
        1,
        &particle_cloud_vertices,
        &particle_cloud_indices,
        &particle_cloud_surface_ranges,
    ) + tess_extract_clone_bytes(
        mark_v_arc,
        mark_i_arc,
        &mark_mesh_vertices,
        &mark_mesh_indices,
        &mark_mesh_surface_ranges,
    ) + tess_extract_clone_bytes(
        glass_v_arc,
        glass_i_arc,
        &glass_mesh_vertices,
        &glass_mesh_indices,
        &glass_mesh_surface_ranges,
    );
    if let Some(slot) = slot {
        slot.stamp_extract_colour(
            extract_ports_ms,
            extract_tess_ms,
            u32::from(skip_world_smodel),
            extract_world_clone_bytes,
            extract_xmodel_clone_bytes,
            xmodel_arc,
            extract_fx_arc,
            extract_fx_clone_bytes,
        );
    }
    let image_handles = images
        .as_ref()
        .map(|handles| (**handles).clone())
        .unwrap_or_default();

    let mut exec_frame = existing
        .as_mut()
        .map(|extracted| std::mem::take(&mut extracted.exec_frame))
        .unwrap_or_default();
    render_frontend::assemble::drawsurf::material_exec::refresh(
        &mut exec_frame,
        mat_frame,
        assembly.as_deref(),
    );
    let next = ExtractedExactColour {
        frame_products,
        generation,
        catalog: Some(Arc::clone(&runtime.catalog)),
        prepared: Some(Arc::clone(&runtime.prepared)),
        exec_frame,
        world_generation,
        smc_revision,
        sun_shadow: mat_frame.sun_shadow,
        warm_pipelines,
        pipeline_world_materials: spawn_job
            .as_ref()
            .map(|job| job.pipeline_world_materials.clone())
            .unwrap_or_default(),
        pipeline_smodel_materials: spawn_job
            .as_ref()
            .map(|job| job.pipeline_smodel_materials.clone())
            .unwrap_or_default(),
        ports,
        static_geometry: render_gpu::ExtractedStaticGeometry {
            world_vertices,
            world_layer,
            world_indices,
            world_surface_ranges,
            world_vertex_refusal,
            smodel_vertices,
            smodel_indices,
            smodel_surface_ranges,
            smodel_vertex_refusal,
            smodel_cached_vertices,
        },
        smc_vb_patches,
        smc_ib_patches,
        smc_index_baked,
        smodel_pretess_indices,
        smodel_index_layout_revision,
        xmodel_vertices,
        xmodel_indices,
        xmodel_surface_ranges,
        xmodel_vertex_refusal,
        fx_vertices,
        fx_indices,
        fx_surface_ranges,
        fx_vertex_refusal,
        fx_revision,
        xmodel_revision,
        xmodel_topology_revision,
        xmodel_packed_segments,
        particle_cloud_vertices,
        particle_cloud_indices,
        particle_cloud_surface_ranges,
        mark_mesh_vertices,
        mark_mesh_indices,
        mark_mesh_surface_ranges,
        mark_mesh_revision,
        glass_mesh_vertices,
        glass_mesh_indices,
        glass_mesh_surface_ranges,
        glass_mesh_revision,
        glass_mesh_vertex_refusal,
        sampler_table: samplers.as_ref().map(|table| (**table).clone()),
        image_handles,
        sorted_material_names: if skip_ports {
            Vec::new()
        } else {
            render_gpu::dump_sorted_material_names(&runtime.catalog)
        },
        shader_program_names: if skip_ports {
            Vec::new()
        } else {
            render_gpu::dump_shader_program_names(&runtime.catalog)
        },
    };
    if let Some(mut existing) = existing {
        replace_exact_colour(
            &mut existing,
            next,
            skip_world_smodel,
            skip_smc_maps,
            skip_ports,
        );
    } else {
        commands.insert_resource(next);
    }
}

fn frame_submission_matches(
    inputs: Option<&render_frontend::assemble::drawsurf::FrameAssemblyInputs>,
    snapshot: &render_frame::FrameProductsSnapshot,
    generation: render_frontend::assemble::drawsurf::MaterialGenerationId,
    world_generation: frame::WorldGeneration,
) -> bool {
    inputs.is_some_and(|inputs| {
        inputs.frame_id == snapshot.frame_id
            && inputs.catalog_generation == generation
            && inputs.world_generation == world_generation
    }) && snapshot.products.iter().all(|product| {
        matches!(product.status, render_frame::FrameProductStatus::Missing(_))
            || product.generation_id == generation
    })
}

fn replace_exact_colour(
    existing: &mut ExtractedExactColour,
    mut next: ExtractedExactColour,
    skip_world_smodel: bool,
    skip_smc_maps: bool,
    skip_ports: bool,
) {
    // Reuse only explicitly stable payload. Every frame field is replaced,
    // including pipeline demand even when the geometry has not changed.
    if skip_world_smodel {
        next.static_geometry = std::mem::take(&mut existing.static_geometry);
    }
    if skip_smc_maps {
        next.smc_index_baked = std::mem::take(&mut existing.smc_index_baked);
    }
    if skip_ports {
        next.ports = std::mem::take(&mut existing.ports);
        next.sorted_material_names = std::mem::take(&mut existing.sorted_material_names);
        next.shader_program_names = std::mem::take(&mut existing.shader_program_names);
    }
    *existing = next;
}

pub fn extract_image_handles(
    mut commands: Commands,
    handles: Extract<Option<Res<render_frontend::assemble::drawsurf::RuntimeImageHandles>>>,
    existing: Option<ResMut<ExtractedRuntimeImageHandles>>,
    slot: Option<Res<SharedRenderStagesSlot>>,
) {
    let Some(handles) = handles.as_ref() else {
        commands.insert_resource(ExtractedRuntimeImageHandles::default());
        return;
    };
    let started = Instant::now();
    let extract_images_arc = 1;
    let next = (**handles).clone();
    if let Some(mut existing) = existing {
        existing.handles = next;
    } else {
        commands.insert_resource(ExtractedRuntimeImageHandles { handles: next });
    }
    if let Some(slot) = slot {
        slot.stamp_extract_images(started.elapsed().as_secs_f32() * 1000.0, extract_images_arc);
    }
}

pub fn extract_postfx(
    runtime: Extract<Option<Res<render_frontend::assemble::drawsurf::MaterialGeneration>>>,
    samplers: Extract<Option<Res<RetailSamplerTable>>>,
    frame: Extract<Res<render_frontend::assemble::drawsurf::dof::DofFrame>>,
    film: Extract<Res<render_frontend::assemble::drawsurf::FilmVisionView>>,
    glow_dvars: Extract<Res<render_frontend::assemble::drawsurf::dof::GlowDvars>>,
    draw_method: Extract<Res<render_frontend::assemble::drawsurf::ColourDrawMethod>>,
    mut extracted: ResMut<render_gpu::ExtractedPostFx>,
) {
    use render_frontend::assemble::drawsurf::postfx_plan::RuntimePostFxResources;
    let films = runtime.as_ref().and_then(|r| match &r.postfx {
        RuntimePostFxResources::Ready(films) => Some(films),
        _ => None,
    });
    match films {
        Some(films)
            if extracted.films.first().map(|f| f.generation)
                != films.first().map(|f| f.generation) =>
        {
            extracted.films = films
                .iter()
                .map(|film| render_gpu::ExtractedFilm {
                    name: film.name,
                    generation: film.generation,
                    port: render_gpu::AdmittedExactPort {
                        id: film.port.id(),
                        abi: film.port.abi().clone(),
                        module: film.port.shared_module(),
                        layout: film.port.wgpu_layout().clone(),
                    },
                    shader: film.shader.clone(),
                    shell: film.shell.clone(),
                })
                .collect();
        }
        None => extracted.films.clear(),
        _ => {}
    }
    extracted.vision = film.current;
    extracted.frame = render_gpu::DofFrame {
        dof: render_gpu::DepthOfField {
            view_model_start: frame.dof.view_model_start,
            view_model_end: frame.dof.view_model_end,
            near_start: frame.dof.near_start,
            near_end: frame.dof.near_end,
            far_start: frame.dof.far_start,
            far_end: frame.dof.far_end,
            near_blur: frame.dof.near_blur,
            far_blur: frame.dof.far_blur,
        },
        bias: frame.bias,
        scene_near: frame.scene_near,
        view_model_near: frame.view_model_near,
        glow: render_gpu::GlowFrame {
            r_glow: glow_dvars.enable,
            r_fullbright: matches!(
                **draw_method,
                render_frontend::assemble::drawsurf::ColourDrawMethod::Fullbright
            ),
            ..render_gpu::GlowFrame::from_vision(extracted.vision)
        },
    };
    extracted.sampler = samplers.as_ref().and_then(|s| s.decode(0x62).ok());
    extracted.depth_sampler = samplers.as_ref().and_then(|s| s.decode(0x61).ok());
}

pub fn extract_geometry(
    mut commands: Commands,
    world: Extract<Option<Res<WorldDrawGpuPlan>>>,
    smodel: Extract<Option<Res<SmodelGpuPlan>>>,
    xmodel: Extract<Option<Res<XModelDrawPlan>>>,
    runtime: Extract<Option<Res<render_frontend::assemble::drawsurf::MaterialGeneration>>>,
    dpvs: Extract<Option<Res<render_frontend::prepare::scene::cull::DpvsFrameStats>>>,
    spawn_job: Extract<Option<Res<render_gpu::GpuSubmitReady>>>,
    existing: Option<ResMut<render_gpu::ExtractedDiagnosticGeometry>>,
    slot: Option<Res<SharedRenderStagesSlot>>,
) {
    if !render_gpu::geometry_diagnostic_enabled() {
        return;
    }
    let Some(runtime) = runtime.as_ref() else {
        return;
    };
    let started = Instant::now();
    let g0_world_surfs = dpvs
        .as_ref()
        .map(|stats| stats.g0_world_surfs.clone())
        .unwrap_or_default();
    let generation = runtime.catalog.generation_id;
    let world_v = world.as_ref().map_or(0, |plan| plan.vertices.len());
    let world_i = world.as_ref().map_or(0, |plan| plan.indices.len());
    let smodel_v = smodel.as_ref().map_or(0, |plan| plan.vertices.len());
    let smodel_i = smodel.as_ref().map_or(0, |plan| plan.indices.len());
    let overlay_gpu_wait = spawn_job.as_ref().is_some_and(|job| job.overlay_gpu_wait);
    let skip_world_smodel = overlay_gpu_wait
        || existing.as_ref().is_some_and(|extracted| {
            extracted.generation == generation
                && extracted.world_vertices.len() == world_v
                && extracted.world_indices.len() == world_i
                && extracted.smodel_vertices.len() == smodel_v
                && extracted.smodel_indices.len() == smodel_i
        });
    if skip_world_smodel {
        let Some(mut existing) = existing else {
            if let Some(slot) = slot {
                slot.stamp_extract_diag(started.elapsed().as_secs_f32() * 1000.0);
            }
            return;
        };
        existing.overlay_gpu_wait = overlay_gpu_wait;
        if overlay_gpu_wait {
            if let Some(slot) = slot {
                slot.stamp_extract_diag(started.elapsed().as_secs_f32() * 1000.0);
            }
            return;
        }
        existing.world_surface_ranges = world
            .as_ref()
            .map_or_else(Vec::new, |plan| plan.surface_ranges.clone());
        existing.smodel_surface_ranges = smodel
            .as_ref()
            .map_or_else(Vec::new, |plan| plan.surface_ranges.clone());
        existing.xmodel_vertices = xmodel
            .as_ref()
            .map_or_else(Vec::new, |plan| plan.vertices.clone());
        existing.xmodel_indices = xmodel
            .as_ref()
            .map_or_else(Vec::new, |plan| plan.indices.clone());
        existing.xmodel_surface_ranges = xmodel
            .as_ref()
            .map_or_else(Vec::new, |plan| plan.surface_ranges.clone());
        existing.xmodel_revision = xmodel.as_ref().map_or(0, |plan| plan.revision);
        existing.g0_world_surfs = g0_world_surfs;
        if let Some(slot) = slot {
            slot.stamp_extract_diag(started.elapsed().as_secs_f32() * 1000.0);
        }
        return;
    }
    let next = render_gpu::ExtractedDiagnosticGeometry {
        generation,
        overlay_gpu_wait,
        world_vertices: world
            .as_ref()
            .map_or_else(Vec::new, |plan| plan.vertices.clone()),
        world_indices: world
            .as_ref()
            .map_or_else(Vec::new, |plan| plan.indices.clone()),
        world_surface_ranges: world
            .as_ref()
            .map_or_else(Vec::new, |plan| plan.surface_ranges.clone()),
        smodel_vertices: smodel
            .as_ref()
            .map_or_else(Vec::new, |plan| plan.vertices.clone()),
        smodel_indices: smodel
            .as_ref()
            .map_or_else(Vec::new, |plan| plan.indices.clone()),
        smodel_surface_ranges: smodel
            .as_ref()
            .map_or_else(Vec::new, |plan| plan.surface_ranges.clone()),
        xmodel_vertices: xmodel
            .as_ref()
            .map_or_else(Vec::new, |plan| plan.vertices.clone()),
        xmodel_indices: xmodel
            .as_ref()
            .map_or_else(Vec::new, |plan| plan.indices.clone()),
        xmodel_surface_ranges: xmodel
            .as_ref()
            .map_or_else(Vec::new, |plan| plan.surface_ranges.clone()),
        xmodel_revision: xmodel.as_ref().map_or(0, |plan| plan.revision),
        g0_world_surfs,
    };
    if let Some(mut existing) = existing {
        *existing = next;
    } else {
        commands.insert_resource(next);
    }
    if let Some(slot) = slot {
        slot.stamp_extract_diag(started.elapsed().as_secs_f32() * 1000.0);
    }
}

#[cfg(test)]
mod ownership_tests {
    use super::*;

    #[test]
    fn submission_rejects_missing_inputs_and_mixed_frames_or_generations() {
        let mut inputs = render_frontend::assemble::drawsurf::FrameAssemblyInputs::default();
        inputs.frame_id = 12;
        inputs.catalog_generation = render_frontend::assemble::drawsurf::MaterialGenerationId(3);
        let mut snapshot = render_frame::FrameProductsSnapshot::empty();
        snapshot.frame_id = 12;
        let matches =
            |inputs: Option<&render_frontend::assemble::drawsurf::FrameAssemblyInputs>| {
                frame_submission_matches(
                    inputs,
                    &snapshot,
                    render_frontend::assemble::drawsurf::MaterialGenerationId(3),
                    frame::WorldGeneration::default(),
                )
            };
        assert!(matches(Some(&inputs)));
        assert!(!matches(None));
        inputs.frame_id = 11;
        assert!(!matches(Some(&inputs)));
        inputs.frame_id = 12;
        inputs.catalog_generation = render_frontend::assemble::drawsurf::MaterialGenerationId(2);
        assert!(!matches(Some(&inputs)));
        inputs.catalog_generation = render_frontend::assemble::drawsurf::MaterialGenerationId(3);
        inputs.world_generation = frame::WorldGeneration::from_install(99);
        assert!(!matches(Some(&inputs)));
        inputs.world_generation = frame::WorldGeneration::default();
        snapshot.products[0].status = render_frame::FrameProductStatus::ResolveReady {
            target: render_frame::ProductTarget::Core3dViewColour,
        };
        assert!(!frame_submission_matches(
            Some(&inputs),
            &snapshot,
            render_frontend::assemble::drawsurf::MaterialGenerationId(3),
            frame::WorldGeneration::default()
        ));
    }

    #[test]
    fn static_geometry_reuse_still_replaces_material_demand_and_frame_payload() {
        let mut existing = ExtractedExactColour::default();
        existing.static_geometry.world_indices = vec![1, 2, 3];
        existing.sorted_material_names = vec!["retained".into()];
        existing.smc_index_baked = vec![5];
        existing.pipeline_world_materials = Arc::new([1].into_iter().collect());
        existing.pipeline_smodel_materials = Arc::new([2].into_iter().collect());
        let mut next = ExtractedExactColour::default();
        next.pipeline_world_materials = Arc::new([3].into_iter().collect());
        next.pipeline_smodel_materials = Arc::new([4].into_iter().collect());
        next.fx_revision = 42;
        next.smodel_pretess_indices = Arc::new(vec![9]);
        replace_exact_colour(&mut existing, next, true, true, true);
        assert_eq!(existing.static_geometry.world_indices, [1, 2, 3]);
        assert_eq!(existing.sorted_material_names, ["retained"]);
        assert_eq!(existing.smc_index_baked, [5]);
        assert_eq!(
            *existing.pipeline_world_materials,
            [3].into_iter().collect()
        );
        assert_eq!(
            *existing.pipeline_smodel_materials,
            [4].into_iter().collect()
        );
        assert_eq!(existing.fx_revision, 42);
        assert_eq!(*existing.smodel_pretess_indices, [9]);

        replace_exact_colour(
            &mut existing,
            ExtractedExactColour::default(),
            false,
            false,
            false,
        );
        assert!(existing.static_geometry.world_indices.is_empty());
        assert!(existing.sorted_material_names.is_empty());
        assert!(existing.smc_index_baked.is_empty());
        assert!(existing.pipeline_world_materials.is_empty());
    }
}
