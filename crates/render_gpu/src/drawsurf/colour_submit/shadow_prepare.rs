use super::colour_census_clock;
use super::shadow_encode::{
    ShadowPassInputs, clear_empty_shadowmap_sun_partition_zero, record_shadowmap_draws,
};
use super::{
    Arc, ArenaDirty, ArenaPack, BTreeMap, BspCameraLane, Buffer, ExactColourGeometry,
    ExactColourPipeline, ExactPipelineRegistry, ExactPrepare, ExactPrepareTarget,
    ExactShadowBindingCache, ExactTessBind, ExactTextureTable, ExtractedColourRefs, FrameProduct,
    FrameProductKind, GPU_SPAN_SPOT, GPU_SPAN_SUN, GpuConstantArena, HashMap, Instant,
    MaterialExecView, MaterialGenerationId, MaterialRunExecutor, ModelIndexRingCopyRefuse,
    ModelIndexStream, PackDraw, PackedArenaUpdate, PackedCodeConstantLane, PackedCodeConstants,
    PassConstantBuffers, PrepareCost, PrepareTextureTables, PreparedExactDraw,
    PublishedRenderFrame, RecordCensus, RenderContext, RenderDevice, RenderQueue, RetainedDrawItem,
    RetainedDrawKind, RunPackCache, RuntimeCodeSources, RuntimeShaderStage,
    RuntimeUploadedImageRegistry, SHADOWMAP_SPOT_COLOR_FORMAT, SHADOWMAP_SPOT_DEPTH_FORMAT,
    SHADOWMAP_SUN_COLOR_FORMAT, SHADOWMAP_SUN_DEPTH_FORMAT, SamplerTable, ShadowTextureTable,
    ShadowmapSpotGpu, ShadowmapSunGpu, SmodelRigidFlush, SunShadowPartition, TableEpoch, TechType,
    TextureView, append_constant_span, as_u32, coalesce_indexed_runs, draw_spot_shadow_map,
    draw_sun_shadow_map_forced, encode, ensure_shadowmap_spot_targets, ensure_shadowmap_sun_target,
    exec_tables, geometry, overlay_packed_code_on_banks, prepare_constant_bases,
    prim_args_from_world_flush, prim_args_u32_index_span, rank_count_map, same_texture_slots,
    smodel_skinned, texture_table_layout,
};
use crate::drawsurf::ExtractedRenderFrameProducts;
use bevy::prelude::{Mat4, Resource, Vec3};
use bevy::render::diagnostic::RecordDiagnostics;

pub(super) fn open_shadow_table_epoch(
    binding_cache: &mut ExactShadowBindingCache,
    shadow_table: &mut ShadowTextureTable,
    scratch: &mut ShadowSubmitScratch,
    uploaded: &RuntimeUploadedImageRegistry,
    generation: MaterialGenerationId,
) {
    let epoch = TableEpoch {
        generation,
        replaced_revision: uploaded.replaced_revision(),
    };
    shadow_table.open_epoch(epoch);
    scratch.prepared_shadow_epoch = epoch;
    binding_cache.open_epoch(generation, uploaded.views_revision());
}

#[derive(Resource, Default)]
pub(super) struct ShadowmapSunArena {
    generation: MaterialGenerationId,
    gpu: [GpuConstantArena; crate::drawsurf::SUN_SHADOW_PARTITION_COUNT as usize],
}

#[derive(Resource, Default)]
pub(super) struct ShadowmapSpotArena {
    generation: MaterialGenerationId,
    gpu: GpuConstantArena,

    pack: ArenaPack,
}

#[derive(Default)]
struct ShadowExecScratch {
    executor: MaterialRunExecutor,
    code_sources: RuntimeCodeSources,
    pack_draws: Vec<PackDraw>,
}

#[derive(Resource, Default)]
pub(crate) struct ShadowSubmitScratch {
    sun_exec: ShadowExecScratch,
    spot_exec: ShadowExecScratch,

    skinned_tess: smodel_skinned::SmodelSkinnedTess,

    sun_prepared: PreparedSunWork,
    spot_prepared: PreparedSpotWork,
    spot_cache: Vec<SpotShadowContent>,

    pub(super) prepared_shadow_epoch: TableEpoch,
}

#[derive(Clone, Copy)]
struct SunFlush<'a> {
    kind: SunFlushKind,
    draw_start: u32,
    draw_count: u32,
    ring_epoch: u32,
    owner_draws: &'a [u32],
    draw_offset: usize,
}

fn shadow_draw_run_continues(prev: &PreparedExactDraw, next: &PreparedExactDraw) -> bool {
    prev.slice_world == next.slice_world && prev.aperture_world == next.aperture_world && {
        let same_pipeline = prev.pipeline == next.pipeline && prev.port == next.port;
        let same_tess = prev.tess == next.tess;
        let same_epoch = prev.ring_epoch == next.ring_epoch;
        let same_texture = same_texture_slots(prev, next);
        same_pipeline
            && same_tess
            && same_epoch
            && same_texture
            && prev.constant_base.is_some()
            && prev.constant_base == next.constant_base
            && (prev.depth_min, prev.depth_max) == (next.depth_min, next.depth_max)
            && prev.start.saturating_add(prev.count) == next.start
    }
}

fn coalesce_shadow_draws(draws: &mut Vec<PreparedExactDraw>) {
    let live = coalesce_shadow_draws_prefix(draws);
    draws.truncate(live);
}

fn coalesce_shadow_draws_prefix(draws: &mut [PreparedExactDraw]) -> usize {
    coalesce_indexed_runs(draws, shadow_draw_run_continues, |prev, next| {
        prev.count = prev.count.saturating_add(next.count);
        prev.bsp_surf_count = prev.bsp_surf_count.saturating_add(next.bsp_surf_count);
        prev.bsp_counted |= next.bsp_counted;
    })
}

fn sun_flush_owner<'a>(
    draws: &'a [RetainedDrawItem],
    techs: &[TechType],
    draw_indices: &[u32],
    draw_offset: usize,
    kind: SunFlushKind,
) -> Result<(&'a RetainedDrawItem, TechType), String> {
    let missing = match kind {
        SunFlushKind::Smodel => "SmodelProvenanceMissing",
        SunFlushKind::XModel => "XModelDynamicProvenanceMissing",
        SunFlushKind::World => "WorldProvenanceMissing",
    };
    let Some(&draw_index) = draw_indices.first() else {
        return Err(format!("{missing}:EmptyOwners"));
    };
    for &owner in draw_indices {
        let index = (owner as usize).saturating_add(draw_offset);
        if owner == u32::MAX {
            return Err(format!("{missing}:MaterialRefused"));
        }
        if draws.get(index).is_none() {
            return Err(format!("{missing}:OwnerOutOfRange"));
        }
        if techs.get(index).is_none() {
            return Err(format!("{missing}:TechniqueOutOfRange"));
        }
    }
    let draw_index = (draw_index as usize).saturating_add(draw_offset);
    let Some(item) = draws.get(draw_index) else {
        return Err(format!("{missing}:OwnerOutOfRange"));
    };
    let matches_kind = match (kind, &item.kind) {
        (SunFlushKind::Smodel, RetainedDrawKind::Smodel { .. }) => true,
        (SunFlushKind::XModel, RetainedDrawKind::XModel { .. }) => true,
        (SunFlushKind::World, RetainedDrawKind::World { .. }) => true,
        _ => false,
    };
    if !matches_kind {
        return Err(format!("{missing}:WrongKind"));
    }
    let Some(&tech) = techs.get(draw_index) else {
        return Err("NoListTech".into());
    };
    Ok((item, tech))
}

fn sun_flush_world_from_local(kind: &RetainedDrawKind) -> Mat4 {
    match *kind {
        RetainedDrawKind::Smodel {
            stream: Some(lighting_iw4::SmodelSurfPath::Skinned),
            ..
        } => Mat4::IDENTITY,
        RetainedDrawKind::World {
            world_from_local, ..
        }
        | RetainedDrawKind::Smodel {
            world_from_local, ..
        }
        | RetainedDrawKind::XModel {
            world_from_local, ..
        } => world_from_local,
        _ => Mat4::IDENTITY,
    }
}

fn emit_local_rigid_shadow_flush(
    source_indices: &[u32],
    hit_start: u32,
    hit_count: u32,
    stream: &mut ModelIndexStream,
    family: &'static str,
) -> Result<(u32, u32, u32), String> {
    if hit_count == 0 {
        return Err(format!("{family}DynamicFlushEmpty"));
    }
    if hit_count % 3 != 0 {
        return Err(format!("{family}DynamicIndexCountNotTriangles"));
    }
    let start = hit_start as usize;
    let end = start.saturating_add(hit_count as usize);
    let indices = source_indices
        .get(start..end)
        .ok_or_else(|| format!("{family}DynamicIndexSourceMissing"))?;
    let (epoch, append) = stream
        .append_indices(indices)
        .map_err(|refuse| match refuse {
            ModelIndexRingCopyRefuse::SourceMissing => {
                format!("{family}DynamicIndexSourceMissing")
            }
            ModelIndexRingCopyRefuse::ExceedsCapacity => format!("{family}RingExceedsCapacity"),
        })?;
    Ok((epoch, append.base_index, hit_count))
}

fn emit_local_xmodel_shadow_flush(
    source_indices: &[u32],
    hit_start: u32,
    hit_count: u32,
    stream: &mut ModelIndexStream,
) -> Result<(u32, u32, u32), String> {
    emit_local_rigid_shadow_flush(source_indices, hit_start, hit_count, stream, "XModel")
}

fn emit_local_smodel_shadow_flush(
    source_indices: &[u32],
    hit_start: u32,
    hit_count: u32,
    stream: &mut ModelIndexStream,
) -> Result<(u32, u32, u32), String> {
    emit_local_rigid_shadow_flush(source_indices, hit_start, hit_count, stream, "Smodel")
}

#[derive(Clone)]
struct ShadowmapSunFlushGpu {
    draws: Vec<PreparedExactDraw>,
    world_from_local: Mat4,
    pass_code: Vec<PackedCodeConstants>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum ShadowStaticOwner {
    World {
        key: u64,
        surf: u16,
        run: u16,
        run_off: u32,
        bsp_kind: Option<BspCameraLane>,
        bsp_run_first: Option<u16>,
        setup_key_changed: bool,
        world_from_local: [u32; 16],
        brush_model: Option<u16>,
        samplers: crate::drawsurf::SurfaceSamplerInputs,
    },
    Smodel {
        key: u64,
        surface: u32,
        material: u32,
        world_from_local: [u32; 16],
        lighting_handle: u32,
        packed_lighting: Option<[u8; 4]>,
        placement: u32,
        visibility_slot: u32,
        owner_world_from_local: [u32; 16],
        stream: Option<lighting_iw4::SmodelSurfPath>,
        cache_index: Option<u16>,
        pretess: Option<render_frame::SmodelPretessRange>,
        samplers: crate::drawsurf::SurfaceSamplerInputs,
    },
}

fn world_shadow_static_owner(item: &RetainedDrawItem) -> Option<ShadowStaticOwner> {
    let RetainedDrawKind::World {
        surf,
        run,
        run_off,
        bsp_kind,
        bsp_run_first,
        setup_key_changed,
        world_from_local,
        brush_model,
    } = item.kind
    else {
        return None;
    };
    Some(ShadowStaticOwner::World {
        key: item.key,
        surf,
        run,
        run_off,
        bsp_kind,
        bsp_run_first,
        setup_key_changed,
        world_from_local: overlay_world_key(world_from_local),
        brush_model,
        samplers: item.surface_samplers,
    })
}

fn smodel_shadow_static_owner(item: &RetainedDrawItem) -> Option<ShadowStaticOwner> {
    let RetainedDrawKind::Smodel {
        surface,
        material,
        world_from_local,
        lighting_handle,
        packed_lighting,
        placement,
        visibility_slot,
        owner_world_from_local,
        stream,
        cache_index,
        pretess,
    } = item.kind
    else {
        return None;
    };
    Some(ShadowStaticOwner::Smodel {
        key: item.key,
        surface,
        material,
        world_from_local: overlay_world_key(world_from_local),
        lighting_handle,
        packed_lighting,
        placement,
        visibility_slot,
        owner_world_from_local: overlay_world_key(owner_world_from_local),
        stream,
        cache_index,
        pretess,
        samplers: item.surface_samplers,
    })
}

fn static_shadow_owners_answer(
    retained: &[Option<ShadowStaticOwner>],
    draws: &[RetainedDrawItem],
    draw_indices: &[u32],
    owner: fn(&RetainedDrawItem) -> Option<ShadowStaticOwner>,
) -> bool {
    retained.len() == draw_indices.len()
        && retained.iter().zip(draw_indices).all(|(expected, &index)| {
            draws.get(index as usize).and_then(owner).as_ref() == expected.as_ref()
        })
}

fn replace_static_shadow_owners(
    retained: &mut Vec<Option<ShadowStaticOwner>>,
    draws: &[RetainedDrawItem],
    draw_indices: &[u32],
    owner: fn(&RetainedDrawItem) -> Option<ShadowStaticOwner>,
) {
    retained.clear();
    retained.extend(
        draw_indices
            .iter()
            .map(|&index| draws.get(index as usize).and_then(owner)),
    );
}

#[derive(Resource, Default)]
pub(super) struct ResidentShadowStaticDraws {
    generation: Option<u64>,
    world: [Vec<Option<ShadowmapSunFlushGpu>>; 2],
    smodel: [Vec<Option<ShadowmapSunFlushGpu>>; 2],
    world_owners: [Vec<Option<ShadowStaticOwner>>; 2],
    smodel_owners: [Vec<Option<ShadowStaticOwner>>; 2],
    smodel_entries: [Vec<render_frame::GfxSmodelRigidEntry>; 2],

    commands: [ResidentSunCommands; 2],
}

impl ResidentShadowStaticDraws {
    fn reset_unless_answering(
        &mut self,
        generation: u64,
        partition: usize,
        world_n: usize,
        smodel_n: usize,
        draws: &[RetainedDrawItem],
        packed: &render_frame::PackedFrontendLists,
    ) -> bool {
        let i = partition.min(1);
        let generation_changed = self.generation != Some(generation);
        if generation_changed {
            self.generation = Some(generation);
            self.world = [Vec::new(), Vec::new()];
            self.smodel = [Vec::new(), Vec::new()];
            self.world_owners = [Vec::new(), Vec::new()];
            self.smodel_owners = [Vec::new(), Vec::new()];
            self.smodel_entries = [Vec::new(), Vec::new()];
        }
        let world_changed = generation_changed
            || !static_shadow_owners_answer(
                &self.world_owners[i],
                draws,
                &packed.world_draw_indices,
                world_shadow_static_owner,
            );
        let smodel_changed = generation_changed
            || self.smodel_entries[i] != packed.smodel
            || !static_shadow_owners_answer(
                &self.smodel_owners[i],
                draws,
                &packed.smodel_draw_indices,
                smodel_shadow_static_owner,
            );
        if world_changed || self.world[i].len() != world_n {
            self.world[i].clear();
            self.world[i].resize_with(world_n, || None);
        }
        if smodel_changed || self.smodel[i].len() != smodel_n {
            self.smodel[i].clear();
            self.smodel[i].resize_with(smodel_n, || None);
        }
        if world_changed {
            replace_static_shadow_owners(
                &mut self.world_owners[i],
                draws,
                &packed.world_draw_indices,
                world_shadow_static_owner,
            );
        }
        if smodel_changed {
            self.smodel_entries[i].clone_from(&packed.smodel);
            replace_static_shadow_owners(
                &mut self.smodel_owners[i],
                draws,
                &packed.smodel_draw_indices,
                smodel_shadow_static_owner,
            );
        }
        smodel_changed
    }
}

#[derive(Clone, Copy)]
struct SunCodeSite {
    index: u16,
    byte_off: usize,
    first_row: usize,
    rows: usize,
}

struct SunArenaSlot {
    placement: u32,
    sites: Vec<SunCodeSite>,
}

#[derive(Default)]
struct ResidentSunCommands {
    generation: Option<u64>,
    world_n: usize,
    smodel_n: usize,
    world_draws: Vec<PreparedExactDraw>,
    smodel_draws: Vec<PreparedExactDraw>,
    slots: Vec<SunArenaSlot>,
    placements: Vec<Mat4>,
    placement_index: HashMap<[u32; 16], u32>,
    placement_wvp: Vec<[[u32; 4]; 4]>,
    bytes: Vec<u8>,

    dirty: Vec<ArenaDirty>,
    resident_slots: usize,
    resident_placements: usize,
    resident_bytes: usize,

    refused: u32,
}

impl ResidentSunCommands {
    fn rebuild(
        &mut self,
        generation: u64,
        world: &[Option<ShadowmapSunFlushGpu>],
        smodel: &[Option<ShadowmapSunFlushGpu>],
    ) {
        self.generation = Some(generation);
        self.world_n = world.len();
        self.smodel_n = smodel.len();
        self.slots.clear();
        self.placements.clear();
        self.placement_index.clear();
        self.bytes.clear();
        self.refused = 0;
        let mut seen = HashMap::new();
        let mut world_draws = Vec::new();
        let mut smodel_draws = Vec::new();
        for flush in world.iter().flatten() {
            self.push_flush(flush, &mut seen, &mut world_draws);
        }
        for flush in smodel.iter().flatten() {
            self.push_flush(flush, &mut seen, &mut smodel_draws);
        }
        coalesce_shadow_draws(&mut world_draws);
        coalesce_shadow_draws(&mut smodel_draws);
        self.world_draws = world_draws;
        self.smodel_draws = smodel_draws;
        self.resident_slots = self.slots.len();
        self.resident_placements = self.placements.len();
        self.resident_bytes = self.bytes.len();
    }

    fn open_frame(&mut self) {
        self.slots.truncate(self.resident_slots);
        self.placements.truncate(self.resident_placements);
        let resident = u32::try_from(self.resident_placements).unwrap_or(u32::MAX);
        self.placement_index.retain(|_, index| *index < resident);
        self.bytes.truncate(self.resident_bytes);
        self.dirty.clear();
    }

    fn placement_for(&mut self, world_key: [u32; 16], world_from_local: Mat4) -> u32 {
        if let Some(&index) = self.placement_index.get(&world_key) {
            return index;
        }
        let index = u32::try_from(self.placements.len()).expect("sun placement index fits u32");
        self.placements.push(world_from_local);
        self.placement_index.insert(world_key, index);
        index
    }

    fn push_flush(
        &mut self,
        flush: &ShadowmapSunFlushGpu,
        seen: &mut HashMap<(usize, usize, [u32; 16]), u32>,
        out: &mut Vec<PreparedExactDraw>,
    ) {
        let world_key = overlay_world_key(flush.world_from_local);
        for (draw, code) in flush.draws.iter().zip(flush.pass_code.iter()) {
            let Some(constants) = draw.constants.as_ref() else {
                out.push(draw.clone());
                continue;
            };
            let key = (
                Arc::as_ptr(constants) as usize,
                Arc::as_ptr(&draw.texture_slots).cast::<u32>() as usize,
                world_key,
            );
            let base = match seen.get(&key) {
                Some(&base) => base,
                None => {
                    let vertex_off = self.bytes.len();
                    let pixel_off = vertex_off + std::mem::size_of_val(constants.vertex.as_slice());
                    let Some(sites) = sun_code_sites(code, constants, vertex_off, pixel_off) else {
                        self.refused = self.refused.saturating_add(1);
                        continue;
                    };
                    let base =
                        u32::try_from(vertex_off / 16).expect("sun constant arena row fits u32");
                    append_constant_span(&mut self.bytes, constants, &draw.texture_slots);
                    self.dirty.push(ArenaDirty::Identity {
                        start: vertex_off,
                        end: self.bytes.len(),
                    });
                    let placement = self.placement_for(world_key, flush.world_from_local);
                    self.slots.push(SunArenaSlot { placement, sites });
                    seen.insert(key, base);
                    base
                }
            };
            let mut packed = draw.clone();

            packed.constants = None;
            packed.constant_base = Some(base);
            out.push(packed);
        }
    }

    fn patch_sun_registers(&mut self, partition: &SunShadowPartition, view_origin: Vec3) {
        let projection = crate::drawsurf::code_transpose_matrix_row4(partition.projection);
        // Retained banks outlive eye translation, so both sides of the
        // camera-relative matrix product must use the current eye.
        let view_projection = crate::drawsurf::code_transpose_matrix_row4(
            render_frame::code_math::camera_relative_view_projection(
                partition.clip_from_world,
                view_origin,
            ),
        );
        let polygon_offset = [partition.polygon_offset.map(f32::to_bits)];
        let mut wvp = std::mem::take(&mut self.placement_wvp);
        wvp.clear();
        wvp.extend(self.placements.iter().map(|world_from_local| {
            crate::drawsurf::code_transpose_matrix_row4(
                partition.clip_from_world * *world_from_local,
            )
        }));
        let camera_from_world = Mat4::from_translation(-view_origin);
        for slot in &self.slots {
            let Some(world_from_local) = self.placements.get(slot.placement as usize) else {
                continue;
            };
            let world =
                crate::drawsurf::code_transpose_matrix_row4(camera_from_world * *world_from_local);
            let Some(placement_wvp) = wvp.get(slot.placement as usize) else {
                continue;
            };
            for site in &slot.sites {
                let rows: &[[u32; 4]] = match site.index {
                    crate::drawsurf::CODE_TRANSPOSE_WORLD_VIEW_PROJECTION0 => &placement_wvp[..],
                    render_backend::overlay::CODE_TRANSPOSE_PROJECTION => &projection,
                    render_backend::overlay::CODE_TRANSPOSE_VIEW_PROJECTION => &view_projection,
                    render_backend::overlay::CODE_TRANSPOSE_WORLD0 => &world,
                    crate::drawsurf::CODE_SHADOWMAP_POLYGON_OFFSET => &polygon_offset,
                    _ => continue,
                };
                let end = site.byte_off + site.rows * 16;
                let bytes = bytemuck::cast_slice(&rows[site.first_row..site.first_row + site.rows]);
                if self.bytes[site.byte_off..end] != *bytes {
                    self.bytes[site.byte_off..end].copy_from_slice(bytes);
                    self.dirty.push(ArenaDirty::Identity {
                        start: site.byte_off,
                        end,
                    });
                }
            }
        }
        self.placement_wvp = wvp;
    }
}

fn sun_code_sites(
    code: &PackedCodeConstants,
    constants: &PassConstantBuffers,
    vertex_off: usize,
    pixel_off: usize,
) -> Option<Vec<SunCodeSite>> {
    let mut sites = Vec::new();
    for lane in &code.lanes {
        let available_rows = match lane.index {
            crate::drawsurf::CODE_TRANSPOSE_WORLD_VIEW_PROJECTION0
            | render_backend::overlay::CODE_TRANSPOSE_PROJECTION
            | render_backend::overlay::CODE_TRANSPOSE_VIEW_PROJECTION
            | render_backend::overlay::CODE_TRANSPOSE_WORLD0 => 4usize,
            crate::drawsurf::CODE_SHADOWMAP_POLYGON_OFFSET => 1usize,
            _ => continue,
        };
        let first_row = usize::from(lane.first_row);
        let rows = usize::from(lane.row_count);
        if first_row.checked_add(rows)? > available_rows {
            return None;
        }
        let (bank_len, base) = match lane.stage {
            RuntimeShaderStage::Vertex => (constants.vertex.len(), vertex_off),
            RuntimeShaderStage::Pixel => (constants.pixel.len(), pixel_off),
        };
        let first = usize::from(lane.destination);
        if first.checked_add(rows)? > bank_len {
            return None;
        }
        sites.push(SunCodeSite {
            index: lane.index,
            byte_off: base + first * 16,
            first_row,
            rows,
        });
    }
    Some(sites)
}

fn sun_overlay_refusal(partition: usize) -> &'static str {
    if partition == 0 {
        "NearOverlayRefused"
    } else {
        "FarOverlayRefused"
    }
}

fn overlay_world_key(world_from_local: Mat4) -> [u32; 16] {
    world_from_local.to_cols_array().map(f32::to_bits)
}

type ShadowOverlayIntern = HashMap<(usize, u8, [u32; 16]), Arc<PassConstantBuffers>>;

fn intern_overlaid_shadow_banks(
    interned: &Arc<PassConstantBuffers>,
    overlay: &PackedCodeConstants,
    intern: &mut ShadowOverlayIntern,
    partition_i: u8,
    world_from_local: Mat4,
) -> Result<Arc<PassConstantBuffers>, ()> {
    let key = (
        Arc::as_ptr(interned) as usize,
        partition_i,
        overlay_world_key(world_from_local),
    );
    if let Some(hit) = intern.get(&key) {
        return Ok(Arc::clone(hit));
    }
    let mut banks = (**interned).clone();
    overlay_packed_code_on_banks(&mut banks.vertex, &mut banks.pixel, &overlay.lanes)
        .map_err(|_| ())?;
    let out = Arc::new(banks);
    intern.insert(key, Arc::clone(&out));
    Ok(out)
}

fn overlay_shadow_draw_constants_with(
    draw: &PreparedExactDraw,
    overlay: &PackedCodeConstants,
    world_from_local: Mat4,
    overlay_i: u8,
    intern: &mut ShadowOverlayIntern,
) -> Result<PreparedExactDraw, ()> {
    let mut out = draw.clone();
    let Some(constants) = draw.constants.as_ref() else {
        return Ok(out);
    };
    let key = (
        Arc::as_ptr(constants) as usize,
        overlay_i,
        overlay_world_key(world_from_local),
    );
    let overlaid = if let Some(hit) = intern.get(&key) {
        Arc::clone(hit)
    } else {
        intern_overlaid_shadow_banks(constants, overlay, intern, overlay_i, world_from_local)?
    };
    out.constants = Some(overlaid);
    out.constant_base = None;
    Ok(out)
}

fn spot_overlay_lane(
    template: &PackedCodeConstantLane,
    rows: Vec<[u32; 4]>,
) -> PackedCodeConstantLane {
    PackedCodeConstantLane {
        stage: template.stage,
        destination: template.destination,
        index: template.index,
        first_row: 0,
        row_count: u8::try_from(rows.len()).unwrap_or(u8::MAX),
        rows: Arc::from(rows.into_boxed_slice()),
    }
}

fn shadowmap_spot_overlay(
    template: &PackedCodeConstants,
    parms: &lighting_iw4::SpotShadowViewParms,
    world_from_local: Mat4,
) -> PackedCodeConstants {
    let clip_from_world = Mat4::from_cols_array(&parms.view_projection)
        * Mat4::from_translation(-Vec3::from_array(parms.origin));
    let wvp = crate::drawsurf::code_transpose_matrix_rows(clip_from_world * world_from_local);
    let projection =
        crate::drawsurf::code_transpose_matrix_rows(Mat4::from_cols_array(&parms.projection));
    let lanes = template
        .lanes
        .iter()
        .filter_map(|lane| match lane.index {
            crate::drawsurf::CODE_TRANSPOSE_WORLD_VIEW_PROJECTION0 => {
                Some(spot_overlay_lane(lane, wvp.clone()))
            }
            render_backend::overlay::CODE_TRANSPOSE_PROJECTION => {
                Some(spot_overlay_lane(lane, projection.clone()))
            }
            _ => None,
        })
        .collect();
    PackedCodeConstants::from_lanes(lanes)
}

#[derive(Clone, Copy)]
enum SunFlushKind {
    World,
    Smodel,
    XModel,
}

impl SunFlushKind {
    fn family(self) -> &'static str {
        match self {
            SunFlushKind::World => "World",
            SunFlushKind::Smodel => "Smodel",
            SunFlushKind::XModel => "XModel",
        }
    }
}

impl ExactPrepare<'_> {
    fn prepare_shadow_flush_draws(
        &mut self,
        executor: &mut MaterialRunExecutor,
        view: Option<MaterialExecView<'_>>,
        flush: SunFlush<'_>,
        product: &FrameProduct,
        target: ExactPrepareTarget,
    ) -> Result<ShadowmapSunFlushGpu, String> {
        let owner_draws = flush.owner_draws;
        let draw_offset = flush.draw_offset;
        let result: Result<ShadowmapSunFlushGpu, String> = (|| {
            if flush.draw_count == 0 {
                return Err("EmptyFlush".into());
            }
            let (item, tech) = sun_flush_owner(
                &product.ordered_draws,
                &product.draw_tech,
                owner_draws,
                draw_offset,
                flush.kind,
            )?;
            let view = view.ok_or_else(|| "MaterialTablesMissing".to_string())?;
            let vertex_type = MaterialRunExecutor::vertex_type(view, item, tech);
            executor
                .execute(view, item, tech, vertex_type, false)
                .map_err(|_| "ExecRefused".to_string())?;
            if matches!(flush.kind, SunFlushKind::Smodel)
                && vertex_type == asset_iw4::vertex_decl::STATICMODELCACHE_VERTEX_TYPE
            {
                return Err("SmodelCachedNotRigidStream".into());
            }
            let execution = executor.execution();
            let mut prepared = Vec::new();
            self.slice_plane = match item.kind {
                RetainedDrawKind::XModel { portal_slice, .. } => portal_slice,
                _ => None,
            };
            self.prepare_ready_hit(
                item,
                executor,
                target,
                &mut None,
                super::ShadowProductDependencies::NONE,
                &mut prepared,
            )
            .map_err(|cause| format!("{cause:?}"))?;
            for draw in &mut prepared {
                if draw.tess != ExactTessBind::SmodelSkinned {
                    draw.start = flush.draw_start;
                    draw.count = flush.draw_count;
                    draw.ring_epoch = flush.ring_epoch;
                }
            }
            Ok(ShadowmapSunFlushGpu {
                draws: prepared,
                world_from_local: sun_flush_world_from_local(&item.kind),
                pass_code: execution.code_constants().to_vec(),
            })
        })();
        result.map_err(|cause| format!("{}:{cause}", flush.kind.family()))
    }
}

struct SkinnedShadowFlushes<'a> {
    owner_indices: &'a [u32],
    flushes: &'a [SmodelRigidFlush],
}

impl<'a> SkinnedShadowFlushes<'a> {
    fn draws(self, draw_offset: usize) -> impl Iterator<Item = SunFlush<'a>> {
        self.flushes.iter().map(move |flush| {
            let owner_start = flush.entry_start as usize;
            let owner_end = owner_start.saturating_add(flush.entry_count as usize);
            let owners = self
                .owner_indices
                .get(owner_start..owner_end)
                .unwrap_or(&[]);
            SunFlush {
                kind: SunFlushKind::Smodel,
                draw_start: 0,
                draw_count: 1,
                ring_epoch: 0,
                owner_draws: owners,
                draw_offset,
            }
        })
    }
}

#[derive(Default)]
pub(super) struct SunShadowSubmit {
    pub(super) gpu: u32,
    pub(super) miss: u32,
    pub(super) cause: Option<String>,
    pub(super) causes: Option<String>,
    pub(super) static_hit: u32,
    pub(super) world_ib_n: u32,
    pub(super) record_n: RecordCensus,

    pub(super) emit_ms: f32,
    pub(super) prepare_ms: f32,
    pub(super) patch_ms: f32,
    pub(super) arena_ms: f32,
    pub(super) record_ms: f32,
    pub(super) finish_ms: f32,
}

#[derive(Default)]
pub(super) struct SpotShadowSubmit {
    pub(super) gpu: u32,
    pub(super) miss: u32,
    pub(super) slots: u32,
    pub(super) cache_hits: u32,
    pub(super) updates: u32,
    pub(super) cause: Option<String>,
    pub(super) record_n: RecordCensus,
}

struct PreparedSpotSlot {
    envelope: crate::drawsurf::backend::SpotShadowMapPass,
    color_view: TextureView,
    depth_view: TextureView,
    smodel_index_epochs: Vec<Buffer>,
    xmodel_index_epochs: Vec<Buffer>,
    draw_range: std::ops::Range<usize>,
    content: Option<SpotShadowContent>,
}

#[derive(Default)]
struct PreparedSpotWork {
    generation: MaterialGenerationId,
    clear_pipeline: Option<bevy::render::render_resource::RenderPipeline>,
    miss: u32,
    miss_rows: BTreeMap<String, u32>,
    all_prepared: Vec<PreparedExactDraw>,
    prepared_slots: Vec<PreparedSpotSlot>,
}

struct SpotShadowContent {
    light: u8,
    target: TextureView,
    viewport: render_frame::SunShadowViewport,
    generation: MaterialGenerationId,
    geometry: Arc<super::ExtractedStaticGeometry>,
    smc_revision: Option<u64>,
    index_layout: u64,
    // Published stream versions deliberately over-invalidate on unrelated
    // animation; copying/sorting each light's vertex set cost more than the GPU work.
    xmodel_revision: Option<(u64, u64)>,
    skin_jobs: Vec<render_frame::skinning::PackedSkinJob>,
    image_revision: (u64, u64),
    smodel_indices: Vec<Vec<u32>>,
    xmodel_indices: Vec<Vec<u32>>,
    draws: Vec<PreparedExactDraw>,
}

fn same_spot_draw(a: &PreparedExactDraw, b: &PreparedExactDraw) -> bool {
    a.pipeline == b.pipeline
        && a.port == b.port
        && a.constants == b.constants
        && a.texture_slots == b.texture_slots
        && a.start == b.start
        && a.count == b.count
        && a.tess == b.tess
        && a.depth_min.to_bits() == b.depth_min.to_bits()
        && a.depth_max.to_bits() == b.depth_max.to_bits()
        && a.ring_epoch == b.ring_epoch
        && a.smc_stream_off == b.smc_stream_off
        && a.state == b.state
        && a.slice_world == b.slice_world
        && a.aperture_world == b.aperture_world
}

impl SpotShadowContent {
    fn overlaps(&self, other: &Self) -> bool {
        let a = self.viewport;
        let b = other.viewport;
        self.target.id() == other.target.id()
            && a.x < b.x + b.width
            && b.x < a.x + a.width
            && a.y < b.y + b.height
            && b.y < a.y + a.height
    }
    fn same(&self, other: &Self) -> bool {
        let a = self.viewport;
        let b = other.viewport;
        self.light == other.light
            && self.target.id() == other.target.id()
            && [a.x, a.y, a.width, a.height] == [b.x, b.y, b.width, b.height]
            && self.generation == other.generation
            && Arc::ptr_eq(&self.geometry, &other.geometry)
            && self.smc_revision == other.smc_revision
            && self.index_layout == other.index_layout
            && self.xmodel_revision == other.xmodel_revision
            && self.image_revision == other.image_revision
            && self.smodel_indices == other.smodel_indices
            && self.xmodel_indices == other.xmodel_indices
            && self.skin_jobs.len() == other.skin_jobs.len()
            && self.skin_jobs.iter().zip(&other.skin_jobs).all(|(a, b)| {
                a.shape.id() == b.shape.id()
                    && a.bones == b.bones
                    && a.destination == b.destination
                    && a.offset.map(f32::to_bits) == b.offset.map(f32::to_bits)
                    && a.ranges.len() == b.ranges.len()
                    && a.ranges.iter().zip(b.ranges.iter()).all(|(a, b)| {
                        a.start == b.start && a.count == b.count && a.rigid == b.rigid
                    })
            })
            && self.draws.len() == other.draws.len()
            && self
                .draws
                .iter()
                .zip(&other.draws)
                .all(|(a, b)| same_spot_draw(a, b))
    }
}

struct PreparedSunPartition {
    pi: usize,
    envelope: crate::drawsurf::backend::SunShadowPartitionPass,
    xmodel_draws: Vec<PreparedExactDraw>,
    skinned_draws: Vec<PreparedExactDraw>,
}

#[derive(Default)]
struct PreparedSunWork {
    early: Option<SunShadowSubmit>,
    color_view: Option<TextureView>,
    depth_view: Option<TextureView>,
    clear_partition_zero: bool,
    partitions: Vec<PreparedSunPartition>,
    miss: u32,
    miss_rows: BTreeMap<String, u32>,
    any_flush: bool,
    smodel_reusable_all: bool,
    retry_n: u32,
    world_ib_n: u32,
    emit_ms: f32,
    prepare_ms: f32,
    patch_ms: f32,
    arena_ms: f32,
}

impl PreparedSunWork {
    fn done(submit: SunShadowSubmit) -> Self {
        Self {
            early: Some(submit),
            ..Self::default()
        }
    }
}

impl SunShadowSubmit {
    fn refused(cause: &str) -> Self {
        Self {
            miss: 1,
            cause: Some(format!("{cause}:1")),
            ..Self::default()
        }
    }
}

fn world_shadow_zone_span(start: u32, count: u32, src_index_n: usize) -> Option<(u32, u32)> {
    let end = (start as usize).saturating_add(count as usize);
    (end <= src_index_n).then_some((start, count))
}

fn sun_model_index_span(
    source_len: usize,
    start: u32,
    count: u32,
) -> Result<(u32, u32, u32), String> {
    if count == 0
        || !count.is_multiple_of(3)
        || (start as usize)
            .checked_add(count as usize)
            .is_none_or(|end| end > source_len)
    {
        return Err("SunModelIndexSourceMissing".into());
    }
    Ok((0, start, count))
}

#[derive(Clone, Copy)]
struct SmodelShadowSpan {
    draw_start: u32,
    draw_count: u32,
    ring_epoch: u32,
    entry_start: u32,
    entry_count: u32,
}

fn finish_sun_submit(
    work: &PreparedSunWork,
    gpu: u32,
    record_n: RecordCensus,
    record_ms: f32,
) -> SunShadowSubmit {
    let static_hit = u32::from(work.any_flush && work.smodel_reusable_all && work.retry_n == 0);
    SunShadowSubmit {
        gpu,
        emit_ms: work.emit_ms,
        prepare_ms: work.prepare_ms,
        patch_ms: work.patch_ms,
        arena_ms: work.arena_ms,
        record_ms,
        finish_ms: 0.0,
        miss: work.miss,
        cause: rank_count_map(&work.miss_rows, 1),
        causes: rank_count_map(&work.miss_rows, 6),
        static_hit,
        world_ib_n: work.world_ib_n,
        record_n,
    }
}

pub(super) struct ShadowLane<'a> {
    pub frame: &'a PublishedRenderFrame,
    pub installed_ready: bool,
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

impl ShadowLane<'_> {
    fn prepare_shadowmap_spot(
        &mut self,
        products: &ExtractedRenderFrameProducts,
        extracted: ExtractedColourRefs<'_>,
        sampler_table: &SamplerTable,
        shadow_exec: &mut ShadowExecScratch,
    ) -> PreparedSpotWork {
        let geometry = self.geometry;
        let pipeline_res = self.pipeline;
        let registry = self.registry;
        let device = self.device;
        let queue = self.queue;
        let uploaded = self.uploaded;
        let binding_cache = &mut *self.binding_cache;
        let texture_table: &mut ExactTextureTable = &mut *self.shadow_table;
        let shadow_arena = &mut *self.spot_arena;
        let shadowmap = &mut *self.spotmap;
        let skinned_tess = &mut self.scratch.skinned_tess;

        let ShadowExecScratch {
            executor: shadow_exec_executor,
            ..
        } = shadow_exec;
        shadow_exec_executor.begin_list();
        let shadow_exec_view = exec_tables(extracted).map(|(catalog, prepared_table)| {
            MaterialExecView::camera(catalog, prepared_table, &extracted.frame.exec_frame)
        });
        let spot = products.0.product(FrameProductKind::SpotShadow);
        if spot.spot_slots.is_empty() {
            return PreparedSpotWork::default();
        }
        ensure_shadowmap_spot_targets(shadowmap, device);
        let mut slots: Vec<_> = spot.spot_slots.iter().collect();
        slots.sort_by_key(|slot| {
            (
                match slot.emitted.plan.render_target_id {
                    lighting_iw4::GFX_SPOT_SHADOW_RT_SMALL => 0u8,
                    lighting_iw4::GFX_SPOT_SHADOW_RT_LARGE => 1u8,
                    _ => 2u8,
                },
                slot.emitted.slot_index,
            )
        });

        let mut prepare = ExactPrepare {
            portal_view: None,
            slice_plane: None,
            instance: 0,
            slice_groups: Default::default(),
            extracted,
            geometry,
            pretess: None,
            pipeline_res,
            registry,
            device,
            uploaded,
            spot_shadow_select: None,
            sampler_table,
            textures: PrepareTextureTables::Shadow {
                slots: &mut binding_cache.textures,
                table: texture_table,
            },
            arena: None,
            run_pack: RunPackCache::default(),
            cost: PrepareCost::default(),
            skinned_tess: Some(skinned_tess),
            skinned_shared: None,
        };
        let target = ExactPrepareTarget {
            color: SHADOWMAP_SPOT_COLOR_FORMAT,
            samples: 1,
            depth: SHADOWMAP_SPOT_DEPTH_FORMAT,
            forward_z: true,
            use_world_pretess: false,
        };
        let mut miss = 0u32;
        let mut miss_rows = BTreeMap::<String, u32>::new();
        let mut overlay_intern = ShadowOverlayIntern::new();
        let mut all_prepared = Vec::new();
        let mut prepared_slots = Vec::new();

        for slot in slots {
            let miss_before = miss;
            let emitted = slot.emitted;
            let Some(view_parms) = emitted.view_parms.as_ref() else {
                miss = miss.saturating_add(1);
                *miss_rows
                    .entry("MissingSpotShadowViewParms".into())
                    .or_default() += 1;
                continue;
            };
            let envelope =
                draw_spot_shadow_map(emitted.slot_index, emitted.plan, Some(&slot.packed));
            let Some(work) = envelope.work.as_ref() else {
                miss = miss.saturating_add(1);
                *miss_rows.entry("MissingSpotShadowWork".into()).or_default() += 1;
                continue;
            };
            let Some(color_view) = shadowmap.color_view(envelope.render_target_id).cloned() else {
                miss = miss.saturating_add(1);
                *miss_rows
                    .entry("MissingSpotShadowColorTarget".into())
                    .or_default() += 1;
                continue;
            };
            let Some(depth_view) = shadowmap.depth_view(envelope.render_target_id).cloned() else {
                miss = miss.saturating_add(1);
                *miss_rows
                    .entry("MissingSpotShadowDepthTarget".into())
                    .or_default() += 1;
                continue;
            };

            let mut flushes = Vec::<ShadowmapSunFlushGpu>::new();
            // Receivers sample every emitted slot, so a slot whose casters all
            // compacted away still clears its tile to fully lit.
            let work = if spot.ordered_draws.is_empty() {
                &Default::default()
            } else {
                work
            };
            for flush in &work.world_flushes {
                let (start, count) = prim_args_u32_index_span(prim_args_from_world_flush(*flush));
                let Some((start, count)) =
                    world_shadow_zone_span(start, count, geometry.world_cpu_indices.len())
                else {
                    miss = miss.saturating_add(1);
                    *miss_rows
                        .entry("WorldShadowSpanOutsideZoneIb".into())
                        .or_default() += 1;
                    continue;
                };
                let owner_start = flush.entry_start as usize;
                let owner_end = owner_start.saturating_add(flush.entry_count as usize);
                let owners = slot
                    .packed
                    .world_draw_indices
                    .get(owner_start..owner_end)
                    .unwrap_or(&[]);
                match prepare.prepare_shadow_flush_draws(
                    shadow_exec_executor,
                    shadow_exec_view,
                    SunFlush {
                        kind: SunFlushKind::World,
                        draw_start: start,
                        draw_count: count,
                        ring_epoch: 0,
                        owner_draws: owners,
                        draw_offset: 0,
                    },
                    spot,
                    target,
                ) {
                    Ok(gpu) => flushes.push(gpu),
                    Err(cause) => {
                        miss = miss.saturating_add(1);
                        *miss_rows.entry(cause).or_default() += 1;
                    }
                }
            }

            let mut xmodel_stream = ModelIndexStream::new();
            let mut xmodel_spans = Vec::new();
            for flush in &work.xmodel_flushes {
                let owner_start = flush.entry_start as usize;
                let owner_end = owner_start.saturating_add(flush.entry_count as usize);
                let Some(entries) = slot.packed.xmodel.get(owner_start..owner_end) else {
                    miss = miss.saturating_add(1);
                    *miss_rows
                        .entry("XModelDynamicProvenanceMissing".into())
                        .or_default() += 1;
                    continue;
                };
                for (entry_offset, entry) in entries.iter().enumerate() {
                    let hit_start = entry.index_byte_offset / 2;
                    let hit_count = u32::from(entry.tri_count).saturating_mul(3);
                    match emit_local_xmodel_shadow_flush(
                        &extracted.frame.xmodel_indices,
                        hit_start,
                        hit_count,
                        &mut xmodel_stream,
                    ) {
                        Ok((ring_epoch, draw_start, draw_count)) => xmodel_spans.push((
                            ring_epoch,
                            draw_start,
                            draw_count,
                            owner_start.saturating_add(entry_offset),
                        )),
                        Err(cause) => {
                            miss = miss.saturating_add(1);
                            *miss_rows.entry(cause).or_default() += 1;
                        }
                    }
                }
            }
            let xmodel_epochs = xmodel_stream.finish();
            for (ring_epoch, draw_start, draw_count, owner) in xmodel_spans {
                let owners = slot
                    .packed
                    .xmodel_draw_indices
                    .get(owner..owner + 1)
                    .unwrap_or(&[]);
                match prepare.prepare_shadow_flush_draws(
                    shadow_exec_executor,
                    shadow_exec_view,
                    SunFlush {
                        kind: SunFlushKind::XModel,
                        draw_start,
                        draw_count,
                        ring_epoch,
                        owner_draws: owners,
                        draw_offset: 0,
                    },
                    spot,
                    target,
                ) {
                    Ok(gpu) => flushes.push(gpu),
                    Err(cause) => {
                        miss = miss.saturating_add(1);
                        *miss_rows.entry(cause).or_default() += 1;
                    }
                }
            }

            let mut smodel_stream = ModelIndexStream::new();
            let mut smodel_spans = Vec::new();
            for flush in &work.smodel_flushes {
                let owner_start = flush.entry_start as usize;
                let owner_end = owner_start.saturating_add(flush.entry_count as usize);
                let Some(entries) = slot.packed.smodel.get(owner_start..owner_end) else {
                    miss = miss.saturating_add(1);
                    *miss_rows
                        .entry("SmodelDynamicProvenanceMissing".into())
                        .or_default() += 1;
                    continue;
                };
                for (entry_offset, entry) in entries.iter().enumerate() {
                    let hit_start = entry.index_byte_offset / 2;
                    let hit_count = u32::from(entry.tri_count).saturating_mul(3);
                    match emit_local_smodel_shadow_flush(
                        extracted.world.static_geometry.smodel_indices.as_slice(),
                        hit_start,
                        hit_count,
                        &mut smodel_stream,
                    ) {
                        Ok((ring_epoch, draw_start, draw_count)) => smodel_spans.push((
                            ring_epoch,
                            draw_start,
                            draw_count,
                            owner_start.saturating_add(entry_offset),
                        )),
                        Err(cause) => {
                            miss = miss.saturating_add(1);
                            *miss_rows.entry(cause).or_default() += 1;
                        }
                    }
                }
            }
            let smodel_epochs = smodel_stream.finish();
            for (ring_epoch, draw_start, draw_count, owner) in smodel_spans {
                let owners = slot
                    .packed
                    .smodel_draw_indices
                    .get(owner..owner + 1)
                    .unwrap_or(&[]);
                match prepare.prepare_shadow_flush_draws(
                    shadow_exec_executor,
                    shadow_exec_view,
                    SunFlush {
                        kind: SunFlushKind::Smodel,
                        draw_start,
                        draw_count,
                        ring_epoch,
                        owner_draws: owners,
                        draw_offset: 0,
                    },
                    spot,
                    target,
                ) {
                    Ok(gpu) => flushes.push(gpu),
                    Err(cause) => {
                        miss = miss.saturating_add(1);
                        *miss_rows.entry(cause).or_default() += 1;
                    }
                }
            }

            let mut skinned_gpu = Vec::new();
            for flush in (SkinnedShadowFlushes {
                owner_indices: &slot.packed.smodel_skinned_draw_indices,
                flushes: &work.smodel_skinned_flushes,
            })
            .draws(0)
            {
                match prepare.prepare_shadow_flush_draws(
                    shadow_exec_executor,
                    shadow_exec_view,
                    flush,
                    spot,
                    target,
                ) {
                    Ok(gpu) => skinned_gpu.push(gpu),
                    Err(cause) => {
                        miss = miss.saturating_add(1);
                        *miss_rows.entry(cause).or_default() += 1;
                    }
                }
            }
            flushes.extend(skinned_gpu);

            let start = all_prepared.len();
            for flush in &flushes {
                for (draw, code) in flush.draws.iter().zip(&flush.pass_code) {
                    let overlay = shadowmap_spot_overlay(code, view_parms, flush.world_from_local);
                    match overlay_shadow_draw_constants_with(
                        draw,
                        &overlay,
                        flush.world_from_local,
                        u8::try_from(emitted.slot_index).unwrap_or(u8::MAX),
                        &mut overlay_intern,
                    ) {
                        Ok(mut draw) => {
                            let clip = Mat4::from_cols_array(&view_parms.view_projection)
                                * Mat4::from_translation(-Vec3::from_array(view_parms.origin));
                            prepare_shadow_slice(
                                &mut draw,
                                clip,
                                device,
                                &mut prepare.slice_groups,
                            );
                            all_prepared.push(draw);
                        }
                        Err(()) => {
                            miss = miss.saturating_add(1);
                            *miss_rows.entry("SpotOverlayRefused".into()).or_default() += 1;
                        }
                    }
                }
            }
            let end = all_prepared.len();
            let smodel_index_epochs = shadowmap.upload_smodel_index_epochs(
                emitted.slot_index,
                device,
                queue,
                &smodel_epochs,
            );
            let xmodel_index_epochs = shadowmap.upload_xmodel_index_epochs(
                emitted.slot_index,
                device,
                queue,
                &xmodel_epochs,
            );
            let draws = &all_prepared[start..end];
            let has_xmodel = draws.iter().any(|draw| draw.tess == ExactTessBind::XModel);
            let has_cached = draws
                .iter()
                .any(|draw| draw.tess == ExactTessBind::SmodelCached);
            let content = (miss == miss_before
                && spot.code_sampler_mask == 0
                && !(has_xmodel && extracted.frame.skin_refused)
                && draws.iter().all(|draw| {
                    !draw.shadow_dependencies.spot
                        && !draw.shadow_dependencies.sun
                        && !draw.after_scene_resolve
                }))
            .then(|| SpotShadowContent {
                light: emitted.light_index,
                target: color_view.clone(),
                viewport: envelope.viewport,
                generation: extracted.frame.generation,
                geometry: Arc::clone(&extracted.world.static_geometry),
                smc_revision: if has_cached {
                    extracted.world.smc_revision
                } else {
                    None
                },
                index_layout: if has_cached {
                    extracted.world.smodel_index_layout_revision
                } else {
                    0
                },
                xmodel_revision: has_xmodel.then_some((
                    extracted.frame.xmodel_revision,
                    extracted.frame.xmodel_topology_revision,
                )),
                skin_jobs: if has_xmodel {
                    extracted.frame.skin_jobs.clone()
                } else {
                    Vec::new()
                },
                image_revision: (uploaded.views_revision(), uploaded.replaced_revision()),
                smodel_indices: smodel_epochs,
                xmodel_indices: xmodel_epochs,
                draws: draws.to_vec(),
            });
            prepared_slots.push(PreparedSpotSlot {
                content,
                envelope,
                color_view,
                depth_view,
                smodel_index_epochs,
                xmodel_index_epochs,
                draw_range: start..end,
            });
        }
        drop(prepare);

        if !all_prepared.is_empty() {
            let ShadowmapSpotArena { gpu, pack, .. } = shadow_arena;
            pack.begin_list();
            let layout = prepare_constant_bases(&mut all_prepared, pack, pipeline_res);
            pack.upload(
                gpu,
                layout,
                registry,
                device,
                queue,
                "iw4_shadowmap_spot_vs_arena",
            );
        }
        PreparedSpotWork {
            generation: extracted.frame.generation,
            clear_pipeline: shadowmap.clear_pipeline().cloned(),
            miss,
            miss_rows,
            all_prepared,
            prepared_slots,
        }
    }

    fn prepare_shadowmap_sun(
        &mut self,
        products: &ExtractedRenderFrameProducts,
        extracted: ExtractedColourRefs<'_>,
        sampler_table: &SamplerTable,
        shadow_exec: &mut ShadowExecScratch,
    ) -> PreparedSunWork {
        let geometry = self.geometry;
        let pipeline_res = self.pipeline;
        let registry = self.registry;
        let device = self.device;
        let queue = self.queue;
        let uploaded = self.uploaded;
        let binding_cache = &mut *self.binding_cache;
        let texture_table: &mut ExactTextureTable = &mut *self.shadow_table;
        let shadow_arena = &mut *self.shadow_arena;
        let shadowmap = &mut *self.shadowmap;
        let static_draws = &mut *self.static_draws;
        let skinned_tess = &mut self.scratch.skinned_tess;

        let sun = products.0.product(FrameProductKind::SunShadow);
        if sun.ordered_draws.is_empty() {
            return PreparedSunWork::done(SunShadowSubmit::default());
        }
        let (color_view, resized) = ensure_shadowmap_sun_target(shadowmap, device);
        if resized {
            binding_cache.textures.clear();
        }
        let Some(color_view) = color_view else {
            return PreparedSunWork::done(SunShadowSubmit::refused("ShadowmapSunTargetMissing"));
        };
        let Some(depth_view) = shadowmap.depth_view().cloned() else {
            return PreparedSunWork::done(SunShadowSubmit::refused("ShadowmapSunDepthMissing"));
        };
        if draw_sun_shadow_map_forced(0, None).is_none() {
            return PreparedSunWork::done(SunShadowSubmit::refused("ShadowmapSunPartitionMissing"));
        }
        let generation = extracted.world.generation.get();

        let ShadowExecScratch {
            executor: shadow_exec_executor,
            code_sources: shadow_code_sources,
            pack_draws,
        } = shadow_exec;
        shadow_exec_executor.begin_list();
        shadow_code_sources.clone_from(&extracted.frame.exec_frame.code_sources);
        if let Some(sun_frame) = extracted.frame.sun_shadow.as_ref() {
            shadow_code_sources.set_constant(
                crate::drawsurf::CODE_TRANSPOSE_WORLD_VIEW_PROJECTION0,
                crate::drawsurf::code_transpose_matrix_rows(
                    sun_frame.partitions[0].clip_from_world,
                ),
            );
        }
        let shadow_exec_view = extracted.frame.sun_shadow.as_ref().and_then(|sun_frame| {
            let (catalog, prepared_table) = exec_tables(extracted)?;
            Some(MaterialExecView::shadow_partition(
                catalog,
                prepared_table,
                &extracted.frame.exec_frame,
                sun_frame.partitions[0].clip_from_world,
                sun_frame.partitions[0].view,
                shadow_code_sources,
            ))
        });
        let mut prepare = ExactPrepare {
            portal_view: None,
            slice_plane: None,
            instance: 0,
            slice_groups: Default::default(),
            extracted,
            geometry,
            pretess: None,
            pipeline_res,
            registry,
            device,
            uploaded,
            spot_shadow_select: None,
            sampler_table,
            textures: PrepareTextureTables::Shadow {
                slots: &mut binding_cache.textures,
                table: texture_table,
            },
            arena: None,
            run_pack: RunPackCache::default(),
            cost: PrepareCost::default(),
            skinned_tess: Some(skinned_tess),
            skinned_shared: None,
        };
        let mut miss = 0u32;
        let mut miss_rows = BTreeMap::<String, u32>::new();
        let world_ib_n = 0u32;
        let mut retry_n = 0u32;
        if texture_table_layout(pipeline_res).is_none() {
            return PreparedSunWork::done(SunShadowSubmit::refused("TextureTableLayoutMissing"));
        }
        let mut smodel_reusable_all = true;
        let mut any_flush = false;
        let mut clear_partition_zero = false;
        let mut partitions = Vec::new();
        let mut ms_emit = 0.0f32;
        let mut ms_prepare = 0.0f32;
        let mut ms_patch = 0.0f32;
        let mut ms_arena = 0.0f32;
        let near_n = sun.sun_near_n.min(sun.ordered_draws.len());
        for partition in 0..crate::drawsurf::SUN_SHADOW_PARTITION_COUNT {
            let pi = partition as usize;
            let draws = if pi == 0 {
                &sun.ordered_draws[..near_n]
            } else {
                &sun.ordered_draws[near_n..]
            };
            if draws.is_empty() {
                if pi == 0 {
                    clear_partition_zero = true;
                }
                continue;
            }
            let packed_fallback;
            let packed = match &sun.sun_packed {
                Some(lists) => &lists[pi],
                None => {
                    packed_fallback = crate::drawsurf::backend::pack_sun_shadow_frontend(
                        draws,
                        &products.0.world_run_surfs,
                        &geometry.world_surface_ranges,
                        u32::try_from(geometry.static_identity.world_vertex_count)
                            .unwrap_or(u32::MAX),
                        &geometry.smodel_surface_ranges,
                        &geometry.xmodel_surface_ranges,
                        &mut *pack_draws,
                    );
                    &packed_fallback
                }
            };
            let Some(envelope) = draw_sun_shadow_map_forced(partition, Some(&packed)) else {
                continue;
            };
            let Some(work) = envelope.work.as_ref() else {
                continue;
            };

            let cached_absent = packed.smodel_cached.len() + packed.smodel_pretess.len();
            if cached_absent > 0 {
                let n = u32::try_from(cached_absent).unwrap_or(u32::MAX);
                miss = miss.saturating_add(n);
                *miss_rows
                    .entry("SmodelCachedStreamAbsentFromSunAtlas".into())
                    .or_default() += n;
            }
            any_flush = true;
            let smodel_identity_changed = static_draws.reset_unless_answering(
                generation,
                pi,
                work.world_flushes.len(),
                packed.smodel.len(),
                draws,
                packed,
            );
            let emit_started = Instant::now();
            let mut xmodel_spans = Vec::with_capacity(packed.xmodel.len());
            for flush in &work.xmodel_flushes {
                let owner_start = flush.entry_start as usize;
                let owner_end = owner_start.saturating_add(flush.entry_count as usize);
                let Some(entries) = packed.xmodel.get(owner_start..owner_end) else {
                    miss = miss.saturating_add(1);
                    *miss_rows
                        .entry("XModelDynamicProvenanceMissing".into())
                        .or_default() += 1;
                    continue;
                };
                if packed
                    .xmodel_draw_indices
                    .get(owner_start..owner_end)
                    .is_none()
                {
                    miss = miss.saturating_add(1);
                    *miss_rows
                        .entry("XModelDynamicProvenanceMissing".into())
                        .or_default() += 1;
                    continue;
                }
                for (entry_offset, entry) in entries.iter().enumerate() {
                    let hit_start = entry.index_byte_offset / 2;
                    let hit_count = u32::from(entry.tri_count).saturating_mul(3);
                    match sun_model_index_span(
                        extracted.frame.xmodel_indices.len(),
                        hit_start,
                        hit_count,
                    ) {
                        Ok((ring_epoch, draw_start, draw_count)) => {
                            xmodel_spans.push((
                                draw_start,
                                draw_count,
                                ring_epoch,
                                u32::try_from(owner_start.saturating_add(entry_offset))
                                    .unwrap_or(u32::MAX),
                                1,
                            ));
                        }
                        Err(cause) => {
                            miss = miss.saturating_add(1);
                            *miss_rows.entry(cause).or_default() += 1;
                        }
                    }
                }
            }
            smodel_reusable_all &= !smodel_identity_changed;
            let mut smodel_spans = Vec::with_capacity(packed.smodel.len());
            for flush in &work.smodel_flushes {
                let owner_start = flush.entry_start as usize;
                let owner_end = owner_start.saturating_add(flush.entry_count as usize);
                let Some(entries) = packed.smodel.get(owner_start..owner_end) else {
                    smodel_spans.extend((owner_start..owner_end).map(|_| None));
                    *miss_rows
                        .entry("SmodelDynamicProvenanceMissing".into())
                        .or_default() += 1;
                    continue;
                };
                if packed
                    .smodel_draw_indices
                    .get(owner_start..owner_end)
                    .is_none()
                {
                    smodel_spans.extend(entries.iter().map(|_| None));
                    *miss_rows
                        .entry("SmodelDynamicProvenanceMissing".into())
                        .or_default() += 1;
                    continue;
                }
                for (entry_offset, entry) in entries.iter().enumerate() {
                    let hit_start = entry.index_byte_offset / 2;
                    let hit_count = u32::from(entry.tri_count).saturating_mul(3);
                    match sun_model_index_span(
                        extracted.world.static_geometry.smodel_indices.len(),
                        hit_start,
                        hit_count,
                    ) {
                        Ok((ring_epoch, draw_start, draw_count)) => {
                            smodel_spans.push(Some(SmodelShadowSpan {
                                draw_start,
                                draw_count,
                                ring_epoch,
                                entry_start: u32::try_from(
                                    owner_start.saturating_add(entry_offset),
                                )
                                .unwrap_or(u32::MAX),
                                entry_count: 1,
                            }));
                        }
                        Err(cause) => {
                            smodel_spans.push(None);
                            *miss_rows.entry(cause).or_default() += 1;
                        }
                    }
                }
            }
            ms_emit += emit_started.elapsed().as_secs_f32() * 1000.0;
            let prepare_started = Instant::now();
            let src_index_n = geometry.world_cpu_indices.len();
            let world_spans: Vec<Option<(u32, u32)>> = work
                .world_flushes
                .iter()
                .map(|flush| {
                    let (start, count) =
                        prim_args_u32_index_span(prim_args_from_world_flush(*flush));
                    world_shadow_zone_span(start, count, src_index_n)
                })
                .collect();
            let world_span_holes = as_u32(world_spans.iter().filter(|span| span.is_none()).count());
            let smodel_span_holes =
                as_u32(smodel_spans.iter().filter(|span| span.is_none()).count());
            if world_span_holes > 0 {
                miss = miss.saturating_add(world_span_holes);
                *miss_rows
                    .entry("WorldShadowSpanOutsideZoneIb".into())
                    .or_default() += world_span_holes;
            }
            if smodel_span_holes > 0 {
                miss = miss.saturating_add(smodel_span_holes);
                *miss_rows
                    .entry("SmodelShadowStreamRefused".into())
                    .or_default() += smodel_span_holes;
            }
            let retry_before = retry_n;
            for slot_i in 0..static_draws.world[pi].len() {
                if static_draws.world[pi][slot_i].is_some() {
                    continue;
                }
                let Some((src_start, count)) = world_spans[slot_i] else {
                    continue;
                };
                let flush = work.world_flushes[slot_i];
                let owner_start = flush.entry_start as usize;
                let owner_end = owner_start.saturating_add(flush.entry_count as usize);
                let owners = packed
                    .world_draw_indices
                    .get(owner_start..owner_end)
                    .unwrap_or(&[]);
                retry_n = retry_n.saturating_add(1);
                match prepare.prepare_shadow_flush_draws(
                    shadow_exec_executor,
                    shadow_exec_view,
                    SunFlush {
                        kind: SunFlushKind::World,
                        draw_start: src_start,
                        draw_count: count,
                        ring_epoch: 0,
                        owner_draws: owners,
                        draw_offset: if pi == 0 { 0 } else { near_n },
                    },
                    sun,
                    ExactPrepareTarget {
                        color: SHADOWMAP_SUN_COLOR_FORMAT,
                        samples: 1,
                        depth: SHADOWMAP_SUN_DEPTH_FORMAT,
                        forward_z: true,
                        use_world_pretess: false,
                    },
                ) {
                    Ok(gpu) => static_draws.world[pi][slot_i] = Some(gpu),
                    Err(cause) => {
                        miss = miss.saturating_add(1);
                        *miss_rows.entry(cause).or_default() += 1;
                    }
                }
            }
            for slot_i in 0..static_draws.smodel[pi].len() {
                if static_draws.smodel[pi][slot_i].is_some() {
                    continue;
                }
                let Some(span) = smodel_spans[slot_i] else {
                    continue;
                };
                let owner_start = span.entry_start as usize;
                let owner_end = owner_start.saturating_add(span.entry_count as usize);
                let owners = packed
                    .smodel_draw_indices
                    .get(owner_start..owner_end)
                    .unwrap_or(&[]);
                retry_n = retry_n.saturating_add(1);
                match prepare.prepare_shadow_flush_draws(
                    shadow_exec_executor,
                    shadow_exec_view,
                    SunFlush {
                        kind: SunFlushKind::Smodel,
                        draw_start: span.draw_start,
                        draw_count: span.draw_count,
                        ring_epoch: span.ring_epoch,
                        owner_draws: owners,
                        draw_offset: if pi == 0 { 0 } else { near_n },
                    },
                    sun,
                    ExactPrepareTarget {
                        color: SHADOWMAP_SUN_COLOR_FORMAT,
                        samples: 1,
                        depth: SHADOWMAP_SUN_DEPTH_FORMAT,
                        forward_z: true,
                        use_world_pretess: false,
                    },
                ) {
                    Ok(gpu) => static_draws.smodel[pi][slot_i] = Some(gpu),
                    Err(cause) => {
                        miss = miss.saturating_add(1);
                        *miss_rows.entry(cause).or_default() += 1;
                    }
                }
            }
            let mut xmodel_gpu = Vec::with_capacity(xmodel_spans.len());
            for (draw_start, draw_count, ring_epoch, entry_start, entry_count) in xmodel_spans {
                let start = entry_start as usize;
                let end = start.saturating_add(entry_count as usize);
                let owners = packed.xmodel_draw_indices.get(start..end).unwrap_or(&[]);
                match prepare.prepare_shadow_flush_draws(
                    shadow_exec_executor,
                    shadow_exec_view,
                    SunFlush {
                        kind: SunFlushKind::XModel,
                        draw_start,
                        draw_count,
                        ring_epoch,
                        owner_draws: owners,
                        draw_offset: if pi == 0 { 0 } else { near_n },
                    },
                    sun,
                    ExactPrepareTarget {
                        color: SHADOWMAP_SUN_COLOR_FORMAT,
                        samples: 1,
                        depth: SHADOWMAP_SUN_DEPTH_FORMAT,
                        forward_z: true,
                        use_world_pretess: false,
                    },
                ) {
                    Ok(gpu) => xmodel_gpu.push(gpu),
                    Err(cause) => {
                        miss = miss.saturating_add(1);
                        *miss_rows.entry(cause).or_default() += 1;
                    }
                }
            }
            let mut skinned_gpu = Vec::new();
            for flush in (SkinnedShadowFlushes {
                owner_indices: &packed.smodel_skinned_draw_indices,
                flushes: &work.smodel_skinned_flushes,
            })
            .draws(if pi == 0 { 0 } else { near_n })
            {
                match prepare.prepare_shadow_flush_draws(
                    shadow_exec_executor,
                    shadow_exec_view,
                    flush,
                    sun,
                    ExactPrepareTarget {
                        color: SHADOWMAP_SUN_COLOR_FORMAT,
                        samples: 1,
                        depth: SHADOWMAP_SUN_DEPTH_FORMAT,
                        forward_z: true,
                        use_world_pretess: false,
                    },
                ) {
                    Ok(gpu) => skinned_gpu.push(gpu),
                    Err(cause) => {
                        miss = miss.saturating_add(1);
                        *miss_rows.entry(cause).or_default() += 1;
                    }
                }
            }
            ms_prepare += prepare_started.elapsed().as_secs_f32() * 1000.0;
            let patch_started = Instant::now();
            let ResidentShadowStaticDraws {
                world: resident_world,
                smodel: resident_smodel,
                commands,
                ..
            } = &mut *static_draws;
            let plan = &mut commands[pi];
            plan.open_frame();

            if plan.generation != Some(generation)
                || plan.world_n != resident_world[pi].len()
                || plan.smodel_n != resident_smodel[pi].len()
                || retry_n != retry_before
                || smodel_identity_changed
            {
                plan.rebuild(generation, &resident_world[pi], &resident_smodel[pi]);
            }
            if plan.refused > 0 {
                miss = miss.saturating_add(plan.refused);
                *miss_rows.entry(sun_overlay_refusal(pi).into()).or_default() += plan.refused;
            }
            let refused_before = plan.refused;
            let mut xmodel_draws = Vec::with_capacity(xmodel_gpu.len());
            let mut xmodel_seen = HashMap::new();
            for flush in &xmodel_gpu {
                plan.push_flush(flush, &mut xmodel_seen, &mut xmodel_draws);
            }
            let dynamic_refused = plan.refused.saturating_sub(refused_before);
            plan.refused = refused_before;
            if dynamic_refused > 0 {
                miss = miss.saturating_add(dynamic_refused);
                *miss_rows.entry(sun_overlay_refusal(pi).into()).or_default() += dynamic_refused;
            }
            coalesce_shadow_draws(&mut xmodel_draws);
            let mut skinned_draws = Vec::with_capacity(skinned_gpu.len());
            let mut skinned_seen = HashMap::new();
            for flush in &skinned_gpu {
                plan.push_flush(flush, &mut skinned_seen, &mut skinned_draws);
            }
            coalesce_shadow_draws(&mut skinned_draws);
            if plan.world_draws.is_empty()
                && plan.smodel_draws.is_empty()
                && xmodel_draws.is_empty()
                && skinned_draws.is_empty()
            {
                continue;
            }

            let Some(frame) = extracted.frame.sun_shadow else {
                miss = miss.saturating_add(1);
                *miss_rows.entry("MissingSunShadowFrame".into()).or_default() += 1;
                continue;
            };
            plan.patch_sun_registers(
                &frame.partitions[pi],
                extracted.frame.exec_frame.view_origin,
            );
            // Each partition has its own projection. Colour-view clip coordinates
            // must never be reused in a shadow pass.
            for draw in xmodel_draws.iter_mut().chain(skinned_draws.iter_mut()) {
                prepare_shadow_slice(
                    draw,
                    frame.partitions[pi].clip_from_world,
                    device,
                    &mut prepare.slice_groups,
                );
            }
            ms_patch += patch_started.elapsed().as_secs_f32() * 1000.0;
            let arena_started = Instant::now();
            // A growing arena invalidates its old bind group during upload.
            let layout = plan
                .world_draws
                .iter()
                .chain(xmodel_draws.iter())
                .chain(skinned_draws.iter())
                .chain(plan.smodel_draws.iter())
                .find_map(|draw| {
                    pipeline_res
                        .get(draw.port)
                        .map(|port| &port.constants_layout)
                });
            shadow_arena.gpu[pi].upload(
                PackedArenaUpdate {
                    identity: &plan.bytes,
                    placed: &[],
                    identity_reserved: plan.bytes.len(),
                    dirty: &plan.dirty,
                    full_upload: false,
                },
                layout,
                registry,
                device,
                queue,
                "iw4_shadowmap_sun_vs_arena",
            );
            ms_arena += arena_started.elapsed().as_secs_f32() * 1000.0;
            partitions.push(PreparedSunPartition {
                pi,
                envelope,
                xmodel_draws,
                skinned_draws,
            });
        }
        drop(prepare);
        PreparedSunWork {
            early: None,
            color_view: Some(color_view),
            depth_view: Some(depth_view),
            clear_partition_zero,
            partitions,
            miss,
            miss_rows,
            any_flush,
            smodel_reusable_all,
            retry_n,
            world_ib_n,
            emit_ms: ms_emit,
            prepare_ms: ms_prepare,
            patch_ms: ms_patch,
            arena_ms: ms_arena,
        }
    }
}

pub(super) fn prepare_shadow_passes(mut lane: ShadowLane<'_>) {
    let _span = perf::span!("prepare_shadow", "iw4l.render");
    if !lane.installed_ready {
        return;
    }
    let frame = lane.frame;
    let Some(sampler_table) = frame.world().sampler_table.as_ref() else {
        return;
    };
    let extracted = ExtractedColourRefs::new(frame);
    let products = &extracted.frame.frame_products;
    let mut sun_exec = std::mem::take(&mut lane.scratch.sun_exec);
    let mut spot_exec = std::mem::take(&mut lane.scratch.spot_exec);
    let sun_prepared =
        lane.prepare_shadowmap_sun(products, extracted, sampler_table, &mut sun_exec);
    let spot_prepared =
        lane.prepare_shadowmap_spot(products, extracted, sampler_table, &mut spot_exec);
    lane.scratch.sun_prepared = sun_prepared;
    lane.scratch.spot_prepared = spot_prepared;
    lane.scratch.sun_exec = sun_exec;
    lane.scratch.spot_exec = spot_exec;
}

impl ShadowmapSunArena {
    pub(super) fn open_generation(&mut self, generation: MaterialGenerationId) {
        if self.generation != generation {
            self.generation = generation;
            for gpu in &mut self.gpu {
                gpu.bind_group = None;
            }
        }
    }
}

impl ShadowmapSpotArena {
    pub(super) fn open_generation(&mut self, generation: MaterialGenerationId) {
        if self.generation != generation {
            self.generation = generation;
            self.gpu.bind_group = None;
        }
    }
}

pub(super) struct ShadowRecorder<'a, 'w, 's> {
    pub(super) pipeline: &'a ExactColourPipeline,
    pub(super) registry: &'a ExactPipelineRegistry,
    pub(super) geometry: &'a ExactColourGeometry,
    pub(super) device: &'a RenderDevice,
    pub(super) table: &'a mut ExactTextureTable,
    pub(super) context: &'a mut RenderContext<'w, 's>,
}

impl ShadowRecorder<'_, '_, '_> {
    fn record_shadowmap_spot(
        &mut self,
        mut work: PreparedSpotWork,
        cache: &mut Vec<SpotShadowContent>,
        shadow_arena: &ShadowmapSpotArena,
        skinned: encode::RecordMesh<'_>,
    ) -> SpotShadowSubmit {
        let pipeline_res = self.pipeline;
        let registry = self.registry;
        let geometry = self.geometry;
        let device = self.device;
        let texture_table = &mut *self.table;
        let context = &mut *self.context;

        if work.prepared_slots.is_empty() {
            cache.clear();
            return SpotShadowSubmit {
                miss: work.miss,
                cause: rank_count_map(&work.miss_rows, 1),
                ..SpotShadowSubmit::default()
            };
        }
        let Some(table_layout) = texture_table_layout(pipeline_res) else {
            return SpotShadowSubmit {
                miss: 1,
                cause: Some("TextureTableLayoutMissing:1".into()),
                ..SpotShadowSubmit::default()
            };
        };
        let table_binds = texture_table.binds(device, registry, table_layout);
        let diagnostics = context.diagnostic_recorder();
        let diagnostics = diagnostics.as_deref();
        let encoder = context.command_encoder();
        let gpu_span = diagnostics.time_span(encoder, GPU_SPAN_SPOT);
        perf::counter!(
            "spot_shadow_cache_eligible",
            "iw4l.render",
            work.prepared_slots
                .iter()
                .filter(|slot| slot.content.is_some())
                .count() as f64
        );
        let mut indexed = 0u32;
        let mut cache_hits = 0;
        let mut updates = 0;
        let mut record_n = RecordCensus::default();
        let world_index = geometry.world_index.as_ref();
        cache.retain(|entry| {
            entry.generation == work.generation
                && work
                    .prepared_slots
                    .iter()
                    .any(|slot| slot.color_view.id() == entry.target.id())
        });
        for slot in &mut work.prepared_slots {
            if slot
                .content
                .as_ref()
                .is_some_and(|content| cache.iter().any(|entry| entry.same(content)))
            {
                cache_hits += 1;
                continue;
            }
            // This tile is about to change even when some caster recording fails.
            cache.retain(|entry| {
                if let Some(content) = &slot.content {
                    !entry.overlaps(content)
                } else {
                    let a = entry.viewport;
                    let b = slot.envelope.viewport;
                    entry.target.id() != slot.color_view.id()
                        || a.x >= b.x + b.width
                        || b.x >= a.x + a.width
                        || a.y >= b.y + b.height
                        || b.y >= a.y + a.height
                }
            });
            updates += 1;
            let draws = &mut work.all_prepared[slot.draw_range.clone()];
            let live = coalesce_shadow_draws_prefix(draws);
            let result = record_shadowmap_draws(
                encoder,
                ShadowPassInputs {
                    device,
                    registry,
                    geometry: geometry::shadow_record_geometry(
                        geometry,
                        world_index,
                        &slot.smodel_index_epochs,
                        &slot.xmodel_index_epochs,
                        skinned,
                    ),
                    constants_bind: shadow_arena.gpu.bind_group.as_ref(),
                    textures_bind: &table_binds.scene,
                    color_view: &slot.color_view,
                    depth_view: &slot.depth_view,
                    scissor: slot.envelope.scissor,
                    viewport: slot.envelope.viewport,
                    cleared: false,
                    tile_clear: Some(
                        work.clear_pipeline
                            .as_ref()
                            .expect("prepared spot slots have a clear pipeline"),
                    ),
                },
                &draws[..live],
                "iw4_shadowmap_spot_slot",
            );
            if result.indexed as usize == live {
                if let Some(content) = slot.content.take() {
                    cache.push(content);
                }
            }
            indexed = indexed.saturating_add(result.indexed);
            record_n.add(result.bindings);
        }
        gpu_span.end(encoder);
        SpotShadowSubmit {
            cache_hits,
            updates,
            gpu: indexed,
            miss: work.miss,
            slots: work.prepared_slots.len() as u32,
            cause: rank_count_map(&work.miss_rows, 1),
            record_n,
        }
    }

    fn record_shadowmap_sun(
        &mut self,
        mut work: PreparedSunWork,
        shadow_arena: &ShadowmapSunArena,
        static_draws: &ResidentShadowStaticDraws,
        skinned: encode::RecordMesh<'_>,
    ) -> SunShadowSubmit {
        let pipeline_res = self.pipeline;
        let registry = self.registry;
        let geometry = self.geometry;
        let device = self.device;
        let texture_table = &mut *self.table;
        let context = &mut *self.context;

        if let Some(submit) = work.early.take() {
            return submit;
        }
        let Some(color_view) = work.color_view.take() else {
            return SunShadowSubmit::refused("ShadowmapSunTargetMissing");
        };
        let Some(depth_view) = work.depth_view.take() else {
            return SunShadowSubmit::refused("ShadowmapSunDepthMissing");
        };
        if !work.clear_partition_zero && work.partitions.is_empty() {
            return finish_sun_submit(&work, 0, RecordCensus::default(), 0.0);
        }
        let diagnostics = context.diagnostic_recorder();
        let diagnostics = diagnostics.as_deref();
        let encoder = context.command_encoder();
        let gpu_span = diagnostics.time_span(encoder, GPU_SPAN_SUN);
        if work.clear_partition_zero {
            clear_empty_shadowmap_sun_partition_zero(encoder, &color_view, &depth_view);
        }
        let mut indexed = 0u32;
        let mut record_n = RecordCensus::default();
        let record_started = Instant::now();
        if !work.partitions.is_empty() {
            let Some(table_layout) = texture_table_layout(pipeline_res) else {
                gpu_span.end(encoder);
                return SunShadowSubmit::refused("TextureTableLayoutMissing");
            };
            let table_binds = texture_table.binds(device, registry, table_layout);
            let world_index = geometry.world_index.as_ref();
            for part in &work.partitions {
                let plan = &static_draws.commands[part.pi];
                let result = record_shadowmap_draws(
                    encoder,
                    ShadowPassInputs {
                        device,
                        registry,
                        geometry: geometry::shadow_record_geometry(
                            geometry,
                            world_index,
                            geometry.smodel_index.as_slice(),
                            geometry.xmodel_index_epochs(),
                            skinned,
                        ),
                        constants_bind: shadow_arena.gpu[part.pi].bind_group.as_ref(),
                        textures_bind: &table_binds.sun_caster,
                        color_view: &color_view,
                        depth_view: &depth_view,
                        scissor: part.envelope.scissor,
                        viewport: part.envelope.viewport,
                        cleared: part.envelope.cleared,
                        tile_clear: None,
                    },
                    plan.world_draws
                        .iter()
                        .chain(part.xmodel_draws.iter())
                        .chain(part.skinned_draws.iter())
                        .chain(plan.smodel_draws.iter()),
                    "iw4_shadowmap_sun_partition",
                );
                indexed = indexed.saturating_add(result.indexed);
                record_n.add(result.bindings);
            }
        }
        let ms_record = record_started.elapsed().as_secs_f32() * 1000.0;
        gpu_span.end(encoder);
        if indexed == 0 {
            return finish_sun_submit(&work, 0, RecordCensus::default(), ms_record);
        }

        finish_sun_submit(&work, indexed, record_n, ms_record)
    }
}

impl ShadowSubmitScratch {
    pub(super) fn record(
        &mut self,
        mut recorder: ShadowRecorder<'_, '_, '_>,
        queue: &RenderQueue,
        sun_arena: &ShadowmapSunArena,
        spot_arena: &ShadowmapSpotArena,
        static_draws: &ResidentShadowStaticDraws,
        census_on: bool,
    ) -> ShadowSubmitResult {
        self.skinned_tess.upload(recorder.device, queue);
        let vertex = self.skinned_tess.vertex_buffer().cloned();
        let lighting = self.skinned_tess.vertex_lighting_buffer().cloned();
        let index = self.skinned_tess.index_buffer().cloned();
        let skinned = encode::RecordMesh {
            vertex: vertex.as_ref(),
            index: index.as_ref(),
            lighting: lighting.as_ref(),
        };
        let sun_prepared = std::mem::take(&mut self.sun_prepared);
        let spot_prepared = std::mem::take(&mut self.spot_prepared);
        let started = colour_census_clock(census_on);
        let sun = recorder.record_shadowmap_sun(sun_prepared, sun_arena, static_draws, skinned);
        let spot = recorder.record_shadowmap_spot(
            spot_prepared,
            &mut self.spot_cache,
            spot_arena,
            skinned,
        );
        ShadowSubmitResult {
            sun,
            spot,
            submit_started: started,
        }
    }
}

pub(super) struct ShadowSubmitResult {
    pub(super) sun: SunShadowSubmit,
    pub(super) spot: SpotShadowSubmit,
    pub(super) submit_started: Option<Instant>,
}

fn prepare_shadow_slice(
    draw: &mut PreparedExactDraw,
    clip_from_world: Mat4,
    device: &RenderDevice,
    groups: &mut HashMap<[u32; 28], Arc<bevy::render::render_resource::BindGroup>>,
) {
    if let Some(plane) = draw.slice_world {
        let planes = super::portal_scene::entity_clip_uniform(
            clip_from_world,
            None,
            plane,
            draw.aperture_world,
            false,
        );
        let key = std::array::from_fn(|i| planes[i / 4][i % 4].to_bits());
        draw.slice_clip = Some(Arc::clone(groups.entry(key).or_insert_with(|| {
            Arc::new(super::portal_scene::clip_planes_group(device, planes))
        })));
    }
}
