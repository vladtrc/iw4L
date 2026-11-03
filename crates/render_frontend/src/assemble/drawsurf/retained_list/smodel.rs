use super::common::with_catalog;
use crate::assemble::drawsurf::tess::smodel::{SmodelGpuPlan, SmodelMeshSurfaces, SmodelPlacement};
use crate::prepare::scene::smodel_geom_cache::WorldStaticModelCache;
use bevy::prelude::Mat4;
use dpvs_iw4::{GfxDrawSurfFields, material_sort_key_row, pack};
use render_frame::{RetainedDrawItem, RetainedDrawKind, SmodelPretessRange};
use std::collections::HashSet;

fn smodel_drawsurf_key(
    sort_key: u8,
    material_sorted_index: u32,
    object_id: u16,
    reflection_probe_index: u8,
    scene_light_index: u8,
    stream: lighting_iw4::SmodelSurfPath,
) -> u64 {
    pack(GfxDrawSurfFields {
        object_id,
        reflection_probe_index,
        scene_light_index,
        surf_type: lighting_iw4::smodel_surf_type(stream),
        material_sorted_index: render_material::sort_band(material_sorted_index),
        primary_sort_key: material_sort_key_row(sort_key),
        ..Default::default()
    })
    .packed
}

fn smodel_colour_emits(
    pass: SmodelDestinationPass,
    catalog: &crate::assemble::drawsurf::RuntimeMaterialCatalog,
    authored: Option<assets::MaterialIndex>,
) -> bool {
    if pass != SmodelDestinationPass::Colour {
        return true;
    }
    authored
        .and_then(|id| catalog.derived(id))
        .is_none_or(|material| material.draw_rules.smodel_colour_emits)
}

fn smodel_sun_shadow_emits(
    pass: SmodelDestinationPass,
    catalog: &crate::assemble::drawsurf::RuntimeMaterialCatalog,
    authored: Option<assets::MaterialIndex>,
) -> bool {
    if pass != SmodelDestinationPass::SunShadow {
        return true;
    }
    let Some(id) = authored else {
        return true;
    };
    catalog
        .derived(id)
        .map(|material| lighting_iw4::smodel_surf_sun_shadow_emits(material.info_game_flags))
        .unwrap_or(true)
}

#[derive(Clone, Copy, Default)]
pub(super) struct SmodelExpandStats {
    pub(super) emitted: u32,
    pub(super) skipped_custom: u32,
}

fn note_custom_only_skip(
    stats: SmodelExpandStats,
    placement: u32,
    custom_skip: Option<&mut HashSet<u32>>,
) {
    if stats.emitted == 0
        && stats.skipped_custom > 0
        && let Some(set) = custom_skip
    {
        set.insert(placement);
    }
}

pub(super) fn smodel_vis_skips_slot(smodel_vis: &[u8], slot: usize) -> bool {
    if smodel_vis.is_empty() {
        return false;
    }
    smodel_vis.get(slot).copied().unwrap_or(0) == 0
}

pub(super) fn packed_lighting_dword_nonzero(bytes: Option<[u8; 4]>) -> bool {
    bytes.is_some_and(|b| u32::from_le_bytes(b) != 0)
}

pub(super) fn smodel_lod_smc_flag(mesh: &SmodelMeshSurfaces, lod: usize) -> u8 {
    mesh.lod_smc_rows
        .and_then(|rows| rows.get(lod).copied())
        .or_else(|| (lod == 0).then_some(mesh.lod_smc).flatten())
        .map(|row| row[1])
        .unwrap_or(0)
}

pub(super) fn smodel_cache_index_u16(
    cache: Option<&WorldStaticModelCache>,
    lighting_slot: Option<usize>,
    lod: usize,
) -> u16 {
    lighting_slot
        .and_then(|slot| cache.and_then(|c| c.cache_index.get(slot)))
        .and_then(|row| row.get(lod).copied())
        .unwrap_or(0)
}

pub(super) fn smodel_lod_is_rigid(mesh: &SmodelMeshSurfaces, lod: usize) -> Option<bool> {
    let bytes = mesh.xsurface_plus_1_by_lod.get(lod)?;
    let mut raw = Vec::with_capacity(bytes.len());
    for byte in bytes {
        raw.push((*byte)?);
    }
    Some(lighting_iw4::smodel_lod_is_rigid(&raw))
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum SmodelDestinationPass {
    Colour,
    SunShadow,
}

#[derive(Clone, Copy, Debug)]
pub(super) struct SmodelBucketEmitSource {
    pub(super) placement: u32,
    pub(super) lod: u8,
    pub(super) payload: u16,
    pub(super) world_from_local: Mat4,
    pub(super) lighting_handle: u32,
    pub(super) packed_lighting: Option<[u8; 4]>,
    pub(super) pass: SmodelDestinationPass,
}

pub(super) struct SmodelBucketEmitQueues {
    rows: [Vec<SmodelBucketEmitSource>; lighting_iw4::SMODEL_BUCKET_LIST_N],
}

impl Default for SmodelBucketEmitQueues {
    fn default() -> Self {
        Self {
            rows: std::array::from_fn(|_| Vec::new()),
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub(super) struct SmodelDestinationRecord {
    key: u64,
    surface: u32,
    material: u32,
    world_from_local: Mat4,
    lighting_handle: u32,
    pub(super) packed_lighting: Option<[u8; 4]>,
    placement: u32,
    visibility_slot: u32,
    stream: Option<lighting_iw4::SmodelSurfPath>,
    cache_index: Option<u16>,
    pretess: Option<SmodelPretessRange>,
    surface_samplers: crate::assemble::drawsurf::SurfaceSamplerInputs,
    material_rank: u32,
}

impl SmodelDestinationRecord {
    pub(super) fn placement(&self) -> u32 {
        self.placement
    }

    pub(super) fn into_item(
        self,
        catalog: &crate::assemble::drawsurf::material_runtime::RuntimeMaterialCatalog,
    ) -> RetainedDrawItem {
        let world_from_local = match self.stream {
            Some(lighting_iw4::SmodelSurfPath::Cached | lighting_iw4::SmodelSurfPath::Pretess) => {
                Mat4::IDENTITY
            }
            Some(lighting_iw4::SmodelSurfPath::Rigid)
            | Some(lighting_iw4::SmodelSurfPath::Skinned)
            | None => self.world_from_local,
        };
        with_catalog(
            self.key,
            self.material_rank,
            RetainedDrawKind::Smodel {
                surface: self.surface,
                material: self.material,
                world_from_local,
                lighting_handle: self.lighting_handle,
                packed_lighting: self.packed_lighting,
                placement: self.placement,
                visibility_slot: self.visibility_slot,
                owner_world_from_local: self.world_from_local,
                stream: self.stream,
                cache_index: self.cache_index,
                pretess: self.pretess,
            },
            self.surface_samplers,
            catalog,
        )
    }
}

pub(super) struct SmodelPretessBuilder {
    enabled: bool,
    cmd_used: usize,
    cmd_cap: usize,
    indices: Vec<u16>,
}

impl SmodelPretessBuilder {
    pub(super) fn indices(&self) -> &[u16] {
        &self.indices
    }
    pub(super) fn finish(self) -> Vec<u16> {
        self.indices
    }

    pub(super) fn new(enabled: bool) -> Self {
        Self {
            enabled,
            cmd_used: 0,
            cmd_cap: usize::MAX,
            indices: Vec::new(),
        }
    }

    fn dest_used(&self) -> u32 {
        u32::try_from(self.indices.len()).unwrap_or(u32::MAX)
    }

    fn decide(&self, cache_indices: &[u16], runs: &[&[u16]]) -> lighting_iw4::SmodelCachedCmdPlan {
        lighting_iw4::smodel_cached_cmd_plan(
            lighting_iw4::SmodelSurfPath::Cached,
            cache_indices,
            self.enabled,
            false,
            lighting_iw4::SmodelPretessSpace {
                command_used: self.cmd_used,
                command_capacity: self.cmd_cap,
                index_used: self.dest_used(),
                index_capacity: render_frame::DYNAMIC_INDEX_BUFFER_CAPACITY,
            },
            runs,
        )
    }

    fn commit_pretess(
        &mut self,
        alloc: lighting_iw4::SmodelPretessAlloc,
        runs: &[&[u16]],
    ) -> Option<SmodelPretessRange> {
        let start = usize::try_from(alloc.first_index).ok()?;
        let count = usize::try_from(alloc.index_count).ok()?;
        if start != self.indices.len() {
            return None;
        }
        self.indices.resize(start.saturating_add(count), 0);
        if !lighting_iw4::smodel_pretess_indices_copy(&mut self.indices, alloc, runs) {
            self.indices.truncate(start);
            return None;
        }
        self.cmd_used = self.cmd_used.saturating_add(alloc.cmd.len());
        Some(SmodelPretessRange {
            start: alloc.first_index,
            count: alloc.index_count,
        })
    }

    fn append_cached_fallback(&mut self, source: &[u16]) -> Option<SmodelPretessRange> {
        if source.is_empty() || !source.len().is_multiple_of(3) {
            return None;
        }
        let start = u32::try_from(self.indices.len()).ok()?;
        let count = u32::try_from(source.len()).ok()?;
        if start.checked_add(count)? > render_frame::DYNAMIC_INDEX_BUFFER_CAPACITY {
            return None;
        }
        self.indices.extend_from_slice(source);
        Some(SmodelPretessRange { start, count })
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) struct SmodelConsumeResult {
    pub(super) buckets: u32,
    pub(super) context_refused: u32,
    pub(super) skipped_custom: u32,
}

pub(super) fn push_smodel_surf_bucket(
    lists: &mut lighting_iw4::SmodelSurfBucketLists,
    queues: &mut SmodelBucketEmitQueues,
    selected_bucket: Option<i32>,
    cache_index: u16,
    smodel_index: u16,
    mut source: SmodelBucketEmitSource,
) -> (Option<i32>, bool) {
    let Some(bucket) = selected_bucket else {
        return (None, false);
    };
    let payload = lighting_iw4::smodel_bucket_store_payload(bucket, smodel_index, cache_index);
    let Some(push) = lighting_iw4::smodel_surf_bucket_push(lists, bucket, payload) else {
        return (None, false);
    };
    let Some(queue) = usize::try_from(bucket)
        .ok()
        .and_then(|bucket| queues.rows.get_mut(bucket))
    else {
        return (None, false);
    };
    source.payload = payload;
    queue.push(source);
    (Some(bucket), push == lighting_iw4::SmodelBucketPush::Full)
}

pub(super) struct SmodelDestinationWriter<'a> {
    pub(super) smodel_plan: &'a SmodelGpuPlan,
    pub(super) catalog: &'a crate::assemble::drawsurf::RuntimeMaterialCatalog,
    pub(super) out: &'a mut Vec<SmodelDestinationRecord>,
}

impl SmodelDestinationWriter<'_> {
    pub(super) fn expand(
        &mut self,
        source: SmodelBucketEmitSource,
        source_path: Option<lighting_iw4::SmodelSurfPath>,
        custom_skip: Option<&mut HashSet<u32>>,
    ) -> SmodelExpandStats {
        let smodel_plan = self.smodel_plan;
        let catalog = self.catalog;
        let out = &mut *self.out;
        let mut stats = SmodelExpandStats::default();
        let Some(placement) = smodel_plan.placements.get(source.placement as usize) else {
            return stats;
        };
        let Some(mesh) = smodel_plan.meshes.get(placement.mesh) else {
            return stats;
        };
        let lod_surfs = mesh
            .surfaces_by_lod
            .get(usize::from(source.lod))
            .map(Vec::as_slice)
            .unwrap_or(&[]);
        let Some(object_id) = source
            .placement
            .checked_add(1)
            .and_then(|id| u16::try_from(id).ok())
        else {
            return stats;
        };
        for &(surface, authored) in lod_surfs {
            if !smodel_colour_emits(source.pass, catalog, authored) {
                continue;
            }
            if !smodel_sun_shadow_emits(source.pass, catalog, authored) {
                stats.skipped_custom = stats.skipped_custom.saturating_add(1);
                continue;
            }
            let material = match source.pass {
                SmodelDestinationPass::Colour if placement.lit => smodel_plan
                    .lit_material_key
                    .get(&(authored, placement.reflection_probe_index))
                    .copied(),
                SmodelDestinationPass::Colour => {
                    smodel_plan.unlit_material_key.get(&authored).copied()
                }
                SmodelDestinationPass::SunShadow => smodel_plan
                    .lit_material_key
                    .get(&(authored, placement.reflection_probe_index))
                    .copied()
                    .or_else(|| smodel_plan.unlit_material_key.get(&authored).copied())
                    .or_else(|| smodel_plan.shadow_material_key.get(&authored).copied()),
            };
            let Some(material) = material else {
                continue;
            };
            let Some(mat) = smodel_plan.materials.get(material as usize) else {
                continue;
            };
            if source.pass == SmodelDestinationPass::Colour
                && placement.lit != mat.model_lighting_required
            {
                continue;
            }
            let Some(material_sorted_index) = mat.material_sorted_index else {
                continue;
            };
            let Some(source_path) = source_path else {
                continue;
            };
            let stream = lighting_iw4::smodel_dest_path(source_path, false);
            let pretess_range = None;
            let cache_index = matches!(
                stream,
                lighting_iw4::SmodelSurfPath::Cached | lighting_iw4::SmodelSurfPath::Pretess
            )
            .then_some(source.payload);
            out.push(SmodelDestinationRecord {
                material_rank: material_sorted_index,
                key: smodel_drawsurf_key(
                    mat.sort_key,
                    material_sorted_index,
                    object_id,
                    placement.reflection_probe_index,
                    placement.primary_light_index,
                    stream,
                ),
                surface,
                material,
                world_from_local: source.world_from_local,
                lighting_handle: source.lighting_handle,
                packed_lighting: source.packed_lighting,
                placement: source.placement,
                visibility_slot: placement.lighting_slot.unwrap_or(source.placement as usize)
                    as u32,
                stream: Some(stream),
                cache_index,
                pretess: pretess_range,
                surface_samplers: crate::assemble::drawsurf::SurfaceSamplerInputs {
                    reflection_probe: Some(crate::assemble::drawsurf::SurfaceReflectionProbeId(
                        placement.reflection_probe_index,
                    )),
                    ..Default::default()
                },
            });
            stats.emitted = stats.emitted.saturating_add(1);
        }
        note_custom_only_skip(stats, source.placement, custom_skip);
        stats
    }

    fn push(
        &mut self,
        source: SmodelBucketEmitSource,
        surface: u32,
        authored: Option<assets::MaterialIndex>,
        stream: Option<lighting_iw4::SmodelSurfPath>,
        pretess: Option<SmodelPretessRange>,
    ) -> SmodelExpandStats {
        let smodel_plan = self.smodel_plan;
        let catalog = self.catalog;
        let out = &mut *self.out;
        let mut stats = SmodelExpandStats::default();
        if !smodel_colour_emits(source.pass, catalog, authored) {
            return stats;
        }
        if !smodel_sun_shadow_emits(source.pass, catalog, authored) {
            stats.skipped_custom = 1;
            return stats;
        }
        let Some(placement) = smodel_plan.placements.get(source.placement as usize) else {
            return stats;
        };
        let Some(object_id) = source
            .placement
            .checked_add(1)
            .and_then(|id| u16::try_from(id).ok())
        else {
            return stats;
        };
        let material = match source.pass {
            SmodelDestinationPass::Colour if placement.lit => smodel_plan
                .lit_material_key
                .get(&(authored, placement.reflection_probe_index))
                .copied(),
            SmodelDestinationPass::Colour => smodel_plan.unlit_material_key.get(&authored).copied(),
            SmodelDestinationPass::SunShadow => smodel_plan
                .lit_material_key
                .get(&(authored, placement.reflection_probe_index))
                .copied()
                .or_else(|| smodel_plan.unlit_material_key.get(&authored).copied())
                .or_else(|| smodel_plan.shadow_material_key.get(&authored).copied()),
        };
        let Some(material) = material else {
            return stats;
        };
        let Some(mat) = smodel_plan.materials.get(material as usize) else {
            return stats;
        };
        if source.pass == SmodelDestinationPass::Colour
            && placement.lit != mat.model_lighting_required
        {
            return stats;
        }
        let Some(material_sorted_index) = mat.material_sorted_index else {
            return stats;
        };
        let Some(stream) = stream else {
            return stats;
        };
        let cache_index = matches!(
            stream,
            lighting_iw4::SmodelSurfPath::Cached | lighting_iw4::SmodelSurfPath::Pretess
        )
        .then_some(source.payload);
        out.push(SmodelDestinationRecord {
            material_rank: material_sorted_index,
            key: smodel_drawsurf_key(
                mat.sort_key,
                material_sorted_index,
                object_id,
                placement.reflection_probe_index,
                placement.primary_light_index,
                stream,
            ),
            surface,
            material,
            world_from_local: source.world_from_local,
            lighting_handle: source.lighting_handle,
            packed_lighting: source.packed_lighting,
            placement: source.placement,
            visibility_slot: placement.lighting_slot.unwrap_or(source.placement as usize) as u32,
            stream: Some(stream),
            cache_index,
            pretess,
            surface_samplers: crate::assemble::drawsurf::SurfaceSamplerInputs {
                reflection_probe: Some(crate::assemble::drawsurf::SurfaceReflectionProbeId(
                    placement.reflection_probe_index,
                )),
                ..Default::default()
            },
        });
        stats.emitted = 1;
        stats
    }

    fn expand_cached_batch(
        &mut self,
        sources: &[SmodelBucketEmitSource],
        cache: Option<&WorldStaticModelCache>,
        pretess: &mut SmodelPretessBuilder,
        mut custom_skip: Option<&mut HashSet<u32>>,
    ) -> SmodelExpandStats {
        let smodel_plan = self.smodel_plan;
        let mut stats = SmodelExpandStats::default();
        let Some(&first) = sources.first() else {
            return stats;
        };
        let Some(placement) = smodel_plan.placements.get(first.placement as usize) else {
            return stats;
        };
        let Some(mesh) = smodel_plan.meshes.get(placement.mesh) else {
            return stats;
        };
        let lod_surfs = mesh
            .surfaces_by_lod
            .get(usize::from(first.lod))
            .map(Vec::as_slice)
            .unwrap_or(&[]);
        if lod_surfs.is_empty() {
            return stats;
        }
        let ids: Vec<u16> = sources.iter().map(|source| source.payload).collect();
        let mut runs: Vec<&[u16]> = Vec::new();
        let mut complete = true;
        for &(surface, _) in lod_surfs {
            for source in sources {
                let Some(place) = smodel_plan.placements.get(source.placement as usize) else {
                    complete = false;
                    continue;
                };
                match cached_index_run(cache, place, surface) {
                    Some(run) => runs.push(run),
                    None => complete = false,
                }
            }
        }
        let decision = if complete {
            pretess.decide(&ids, &runs)
        } else {
            pretess.decide(&ids, &[])
        };
        if decision.cmd_kind == lighting_iw4::SmodelCmdKind::Unchanged && decision.consumed == 0 {
            for &source in sources {
                let one = self.expand(
                    source,
                    Some(lighting_iw4::SmodelSurfPath::Cached),
                    custom_skip.as_mut().map(|set| &mut **set),
                );
                stats.emitted = stats.emitted.saturating_add(one.emitted);
                stats.skipped_custom = stats.skipped_custom.saturating_add(one.skipped_custom);
            }
            return stats;
        }
        if let Some(alloc) = decision.pretess
            && decision.dest == lighting_iw4::SmodelSurfPath::Pretess
            && let Some(batch_range) = pretess.commit_pretess(alloc, &runs)
        {
            let mut start = batch_range.start;
            for &(surface, authored) in lod_surfs {
                let mut count = 0u32;
                for source in sources {
                    if let Some(place) = smodel_plan.placements.get(source.placement as usize)
                        && let Some(run) = cached_index_run(cache, place, surface)
                    {
                        count = count.saturating_add(run.len() as u32);
                    }
                }
                let range = (count > 0).then_some(SmodelPretessRange { start, count });
                start = start.saturating_add(count);
                let one = self.push(
                    first,
                    surface,
                    authored,
                    Some(lighting_iw4::SmodelSurfPath::Pretess),
                    range,
                );
                stats.emitted = stats.emitted.saturating_add(one.emitted);
                stats.skipped_custom = stats.skipped_custom.saturating_add(one.skipped_custom);
            }
            note_custom_only_skip(
                stats,
                first.placement,
                custom_skip.as_mut().map(|set| &mut **set),
            );
            return stats;
        }
        for &source in sources {
            let Some(place) = smodel_plan.placements.get(source.placement as usize) else {
                continue;
            };
            let mut one = SmodelExpandStats::default();
            for &(surface, authored) in lod_surfs {
                let range = cached_index_run(cache, place, surface)
                    .and_then(|run| pretess.append_cached_fallback(run));
                let pushed = self.push(
                    source,
                    surface,
                    authored,
                    Some(lighting_iw4::SmodelSurfPath::Cached),
                    range,
                );
                one.emitted = one.emitted.saturating_add(pushed.emitted);
                one.skipped_custom = one.skipped_custom.saturating_add(pushed.skipped_custom);
            }
            note_custom_only_skip(
                one,
                source.placement,
                custom_skip.as_mut().map(|set| &mut **set),
            );
            stats.emitted = stats.emitted.saturating_add(one.emitted);
            stats.skipped_custom = stats.skipped_custom.saturating_add(one.skipped_custom);
        }
        stats
    }

    pub(super) fn consume_buckets(
        &mut self,
        lists: &mut lighting_iw4::SmodelSurfBucketLists,
        queues: &mut SmodelBucketEmitQueues,
        active_mask: u32,
        cache: Option<&WorldStaticModelCache>,
        pretess: &mut SmodelPretessBuilder,
        mut custom_skip: Option<&mut HashSet<u32>>,
    ) -> SmodelConsumeResult {
        let smodel_plan = self.smodel_plan;
        let mut consumed = [None; lighting_iw4::SMODEL_BUCKET_LIST_N];
        let n = lighting_iw4::smodel_bucket_lists_consume(lists, active_mask, &mut consumed);
        let mut result = SmodelConsumeResult {
            buckets: n as u32,
            context_refused: 0,
            skipped_custom: 0,
        };
        for bucket in consumed.into_iter().take(n).flatten() {
            let sources = std::mem::take(&mut queues.rows[bucket.bucket as usize]);
            if sources.len() != usize::from(bucket.count) {
                result.context_refused = result.context_refused.saturating_add(
                    u32::try_from(sources.len().abs_diff(usize::from(bucket.count)))
                        .unwrap_or(u32::MAX),
                );
            }
            let count = usize::from(bucket.count).min(sources.len());
            let mut i = 0usize;
            while i < count {
                let source = sources[i];
                if source.payload != bucket.ids[i] || source.lod != bucket.lod {
                    result.context_refused = result.context_refused.saturating_add(1);
                    i += 1;
                    continue;
                }
                if bucket.source != lighting_iw4::SmodelSurfPath::Cached {
                    let stats = self.expand(
                        source,
                        Some(bucket.source),
                        custom_skip.as_mut().map(|set| &mut **set),
                    );
                    result.skipped_custom =
                        result.skipped_custom.saturating_add(stats.skipped_custom);
                    i += 1;
                    continue;
                }
                let ids = &bucket.ids[i..count];
                let rest = &sources[i..count];
                let bank_n = lighting_iw4::smodel_same_bank_count(ids).min(rest.len());
                let batch_state = smodel_plan
                    .placements
                    .get(source.placement as usize)
                    .map(smodel_cached_batch_state);
                let mut n = 1usize;
                while n < bank_n {
                    let next = rest[n];
                    if next.payload != ids[n]
                        || next.lod != bucket.lod
                        || smodel_plan
                            .placements
                            .get(next.placement as usize)
                            .map(smodel_cached_batch_state)
                            != batch_state
                    {
                        break;
                    }
                    n += 1;
                }
                let stats = self.expand_cached_batch(
                    &rest[..n],
                    cache,
                    pretess,
                    custom_skip.as_mut().map(|set| &mut **set),
                );
                result.skipped_custom = result.skipped_custom.saturating_add(stats.skipped_custom);
                i += n;
            }
        }
        result
    }

    pub(super) fn consume_tail(
        &mut self,
        lists: &mut lighting_iw4::SmodelSurfBucketLists,
        queues: &mut SmodelBucketEmitQueues,
        cache: Option<&WorldStaticModelCache>,
        pretess: &mut SmodelPretessBuilder,
        custom_skip: Option<&mut HashSet<u32>>,
    ) -> SmodelConsumeResult {
        let mut mask = 0u32;
        for (bucket, count) in lists.count.iter().enumerate() {
            if *count > 0
                && let Some(bucket_mask) = lighting_iw4::smodel_bucket_mask(bucket as u8)
            {
                mask |= bucket_mask;
            }
        }
        self.consume_buckets(lists, queues, mask, cache, pretess, custom_skip)
    }
}

fn cached_index_run<'a>(
    cache: Option<&'a WorldStaticModelCache>,
    placement: &SmodelPlacement,
    surface: u32,
) -> Option<&'a [u16]> {
    let slot = u32::try_from(placement.lighting_slot?).ok()?;
    let run = cache?.index_runs.get(&(slot, surface))?;
    (!run.is_empty() && run.len().is_multiple_of(3)).then_some(run.as_slice())
}

fn smodel_cached_batch_state(placement: &SmodelPlacement) -> (usize, bool, u8, u8) {
    (
        placement.mesh,
        placement.lit,
        placement.reflection_probe_index,
        placement.primary_light_index,
    )
}
