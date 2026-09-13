use bevy::prelude::*;
use lighting_iw4::{
    MODEL_LIGHTING_TILE_BYTES, ModelLightingAtlasDims, ModelLightingTileIndex,
    SMODEL_LIGHTING_WARN_TOO_MUCH, SModelDirtyLightingAction, SModelLightingAlloc,
    SModelLightingCounters, SModelLightingGlob, dirty_smodel_lighting_action,
    smodel_lighting_bits_words,
};

use crate::prepare::scene::cull::smodel_cull_dist_skips_slot;
use crate::prepare::scene::model_lighting_atlas::{
    WorldModelLightingAtlas, model_lighting_atlas_write_tile,
};
use crate::prepare::scene::smodel_geom_cache::LodRampDvar;
use crate::prepare::scene::view_parms::PreparedSceneView;
use crate::prepare::scene::world::{WorldScene, WorldSmodelLightingSample};

fn smodel_lighting_enabled() -> bool {
    !matches!(
        std::env::var("IW4L_SMODEL_LIGHTING").as_deref(),
        Ok("0") | Ok("false") | Ok("off") | Ok("no")
    )
}

const SMODEL_LIGHTING_HOST_FLAGS_UNKNOWN: u8 = 0;

#[derive(Resource)]
pub struct WorldSmodelLighting {
    smodel_entry_limit: u32,

    pub div_0x100_by_height: u32,

    pub tiles: Vec<Option<[u8; MODEL_LIGHTING_TILE_BYTES]>>,

    pub packed_lighting: Vec<Option<[u8; 4]>>,

    pub handles: Vec<u16>,

    pub parents: Vec<Option<Entity>>,
    counters: SModelLightingCounters,
    freeable_handles: Vec<u16>,
    smodel_index: Vec<u16>,
    used_frame_count: Vec<i32>,
    lighting_bits: Vec<u32>,
    warned_too_much: bool,

    last_assigned: u32,
    last_reused: u32,
    last_evicted: u32,
    last_failed: u32,

    last_dirty: u32,

    pub spawn_lit_n: u32,

    pub spawn_sample_without_technique: u32,
}

impl WorldSmodelLighting {
    pub fn new(
        dims: ModelLightingAtlasDims,
        slot_count: usize,
        samples: &[WorldSmodelLightingSample],
    ) -> Self {
        let entry_limit = dims.smodel_entry_limit as usize;
        let mut tiles = vec![None; slot_count];
        let mut packed_lighting = vec![None; slot_count];
        for sample in samples {
            if let Some(slot) = tiles.get_mut(sample.authored_slot) {
                *slot = Some(sample.tile_rgba);
            }
            if let Some(slot) = packed_lighting.get_mut(sample.authored_slot) {
                *slot = Some(sample.packed_lighting);
            }
        }
        Self {
            smodel_entry_limit: dims.smodel_entry_limit,
            div_0x100_by_height: 0x100 / dims.image_height,
            tiles,
            packed_lighting,
            handles: vec![0; slot_count],
            parents: vec![None; slot_count],
            counters: SModelLightingCounters::AFTER_LIGHT_WALK,
            freeable_handles: vec![0; entry_limit],
            smodel_index: vec![0; entry_limit],
            used_frame_count: vec![0; entry_limit],
            lighting_bits: vec![0; smodel_lighting_bits_words(slot_count as u32)],
            warned_too_much: false,
            last_assigned: 0,
            last_reused: 0,
            last_evicted: 0,
            last_failed: 0,
            last_dirty: 0,
            spawn_lit_n: 0,
            spawn_sample_without_technique: 0,
        }
    }

    fn glob(
        &mut self,
    ) -> Result<(SModelLightingGlob<'_>, &mut [u16]), lighting_iw4::SModelLightingGlobError> {
        SModelLightingGlob::new(
            self.smodel_entry_limit,
            self.tiles.len() as u32,
            &mut self.counters,
            &mut self.freeable_handles,
            &mut self.smodel_index,
            &mut self.used_frame_count,
            &mut self.lighting_bits,
        )
        .map(|glob| (glob, self.handles.as_mut_slice()))
    }

    #[inline]
    pub fn frame_count(&self) -> i32 {
        self.counters.frame_count
    }

    #[inline]
    pub fn last_census(&self) -> (u32, u32, u32, u32) {
        (
            self.last_assigned,
            self.last_reused,
            self.last_evicted,
            self.last_failed,
        )
    }

    #[inline]
    pub fn last_dirty(&self) -> u32 {
        self.last_dirty
    }

    pub fn last_assigned(&self) -> u32 {
        self.last_assigned
    }

    pub fn last_failed(&self) -> u32 {
        self.last_failed
    }

    #[inline]
    pub fn packed_lighting_for_slot(&self, slot: usize) -> Option<[u8; 4]> {
        self.packed_lighting.get(slot).copied().flatten()
    }
}

pub(crate) fn prepare_smodel_lighting(
    dims: ModelLightingAtlasDims,
    samples: &[WorldSmodelLightingSample],
    slot_count: usize,
) -> Option<WorldSmodelLighting> {
    if !smodel_lighting_enabled() || samples.is_empty() {
        return None;
    }
    Some(WorldSmodelLighting::new(dims, slot_count, samples))
}

pub(crate) fn slot_has_lighting_sample(lighting: &WorldSmodelLighting, index: usize) -> bool {
    lighting.tiles.get(index).is_some_and(|t| t.is_some())
}

pub(crate) fn surfaces_take_model_lighting(
    surface_materials: impl IntoIterator<Item = Option<assets::MaterialIndex>>,
    materials: &crate::assemble::drawsurf::RuntimeMaterialCatalog,
) -> bool {
    surface_materials.into_iter().any(|material| {
        material
            .and_then(|id| materials.derived(id))
            .is_some_and(|m| m.uses_model_lighting_const || m.takes_model_lighting)
    })
}

pub(crate) fn slot_is_lit_candidate(
    lighting: &WorldSmodelLighting,
    index: usize,
    surfaces_take_model_lighting: bool,
) -> bool {
    surfaces_take_model_lighting && slot_has_lighting_sample(lighting, index)
}

pub(crate) fn update_smodel_lighting(
    mut lighting: Option<ResMut<WorldSmodelLighting>>,
    mut geom_cache: Option<ResMut<crate::prepare::scene::smodel_geom_cache::WorldStaticModelCache>>,
    atlas: Option<Res<WorldModelLightingAtlas>>,
    scene: Res<WorldScene>,
    mut images: ResMut<Assets<Image>>,
    prepared: Res<PreparedSceneView>,
    lock_pvs: Res<crate::prepare::scene::view_parms::RLockPvs>,
    lod_ramp: Res<LodRampDvar>,
) {
    let Some(lighting) = lighting.as_mut() else {
        return;
    };
    lighting.last_dirty = 0;
    let Some(atlas) = atlas else {
        return;
    };
    let Some(cull) = scene.cull.as_ref() else {
        return;
    };

    let mut visible: Vec<usize> = Vec::new();
    let eye = prepared.ready.then_some(lock_pvs.dpvs_eye(&prepared));
    for slot in 0..lighting.tiles.len() {
        if lighting.tiles.get(slot).is_none_or(|t| t.is_none()) {
            continue;
        }
        if lighting.parents.get(slot).copied().flatten().is_none() {
            continue;
        }
        let seen = cull.smodel_vis.get(slot).copied().unwrap_or(0) != 0;
        if !seen {
            continue;
        }
        let origin = scene
            .static_model_instances
            .get(slot)
            .and_then(|slot| slot.as_ref())
            .map(|inst| inst.origin)
            .unwrap_or([0.0; 3]);
        if smodel_cull_dist_skips_slot(
            &cull.static_model_cull_dists,
            slot,
            origin,
            eye,
            lod_ramp.scale_last,
        ) {
            continue;
        }
        visible.push(slot);
    }

    let mut assigned = 0u32;
    let mut reused = 0u32;
    let mut evicted = 0u32;
    let mut failed = 0u32;
    {
        let Ok((mut glob, handles)) = lighting.glob() else {
            diag::error!(
                World,
                "smodel lighting glob borrow failed — entryLimit={} slots={}",
                lighting.smodel_entry_limit,
                lighting.tiles.len()
            );
            return;
        };

        glob.toggle_frame();
        glob.clear_dirty_bits();

        for slot in visible {
            let outcome = glob.alloc_static_model_lighting(slot as u32, handles[slot]);
            match outcome {
                SModelLightingAlloc::Reused { handle } => {
                    reused += 1;
                    handles[slot] = handle;
                }
                SModelLightingAlloc::Assigned { handle } => {
                    assigned += 1;
                    handles[slot] = handle;
                }
                SModelLightingAlloc::Evicted {
                    handle,
                    evicted_smodel_index,
                } => {
                    evicted += 1;
                    let displaced = usize::from(evicted_smodel_index);
                    handles[displaced] = 0;
                    if let Some(cache) = geom_cache.as_mut() {
                        cache.uncache_slot(displaced);
                    }
                    handles[slot] = handle;
                }
                SModelLightingAlloc::Failed => {
                    failed += 1;

                    handles[slot] = 0;
                }
            }
        }
    }

    lighting.last_assigned = assigned;
    lighting.last_reused = reused;
    lighting.last_evicted = evicted;
    lighting.last_failed = failed;

    if failed > 0 && !lighting.warned_too_much {
        lighting.warned_too_much = true;
        diag::error!(
            World,
            "smodel lighting: R_WarnOncePerFrame 0x{SMODEL_LIGHTING_WARN_TOO_MUCH:x} — \
             {failed} visible lit smodel(s) skipped this frame (atlas full, entryLimit={})",
            lighting.smodel_entry_limit
        );
    }

    let mut dirty_indices: Vec<u32> = Vec::new();
    {
        let Ok((mut glob, _)) = lighting.glob() else {
            diag::error!(
                World,
                "smodel lighting dirty walk: glob borrow failed — entryLimit={} slots={}",
                lighting.smodel_entry_limit,
                lighting.tiles.len()
            );
            return;
        };
        glob.update_dirty_smodel_lighting(|smodel_index| {
            dirty_indices.push(smodel_index);
        });
    }

    let mut dirty_slots: Vec<(usize, u16)> = Vec::with_capacity(dirty_indices.len());
    let mut deferred_patches = 0u32;
    for smodel_index in dirty_indices {
        let slot = smodel_index as usize;
        let handle = lighting.handles.get(slot).copied().unwrap_or(0);
        match dirty_smodel_lighting_action(smodel_index, handle, SMODEL_LIGHTING_HOST_FLAGS_UNKNOWN)
        {
            Some(SModelDirtyLightingAction::SampleImmediate { entry, .. }) => {
                dirty_slots.push((slot, entry.wrapping_add(1)));
            }
            Some(SModelDirtyLightingAction::EnqueuePatch { .. }) => {
                deferred_patches = deferred_patches.saturating_add(1);
            }
            None => {}
        }
    }

    if deferred_patches > 0 {
        diag::error!(
            World,
            "smodel lighting: {deferred_patches} dirty smodel(s) want EnqueuePatch \
             (DrawInst+0x3e bit 0x20) — modelLightingPatchList upload Missing"
        );
    }

    if !dirty_slots.is_empty() {
        let dims = atlas.dims;
        let atlas_handle = atlas.image.clone();
        if let Some(mut atlas) = images.get_mut(&atlas_handle) {
            for &(slot, handle) in &dirty_slots {
                let Some(Some(tile)) = lighting.tiles.get(slot) else {
                    continue;
                };
                let Some(entry) = ModelLightingTileIndex::from_handle(handle) else {
                    continue;
                };
                if !model_lighting_atlas_write_tile(&mut atlas, dims, entry, tile) {
                    diag::error!(
                        World,
                        "smodel lighting: tile for slot {slot} handle {handle} refused by atlas write"
                    );
                }
            }
        }
    }

    lighting.last_dirty = dirty_slots.len() as u32;
}
