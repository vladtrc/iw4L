use crate::assemble::drawsurf::tess::smodel::{SmodelGpuPlan, smodel_camera_lod};
use crate::prepare::scene::cull::smodel_cull_dist_skips_slot;
use crate::prepare::scene::smodel_lighting::WorldSmodelLighting;
use crate::prepare::scene::view_parms::PreparedSceneView;
use crate::prepare::scene::world::WorldScene;
use bevy::platform::collections::{HashMap, HashSet};
use bevy::prelude::*;
use lighting_iw4::{
    CacheStaticModelSurface, SMC_BANK_N, SMC_CACHE_INDEX_LODS, SMC_CLASS_N, SMC_INDEX_U16_N,
    SMC_LEAF_N, SMC_LINK_N, SMC_TREE_N, SmcAllocatorStorage, SmcCachedVertLighting,
    SmcIndexBakeError, SmcLeafStorage, SmcPatchLock, SmcSkinSurface, SmcTree, StaticModelCache,
    r_cache_static_model_indices, r_skin_cached_static_model_cmd,
    r_skin_cached_static_model_cmd_matrix, smc_lod_cache_spec,
};
use render_frontend::{
    AddWorkerCmd, SkinCachedStaticModelCmd, WORKER_CMD_SMODELCACHE, WorkerCmdBusyInput,
    WorkerCmdQueues,
};

#[derive(Resource, Clone, Copy, Debug, Default)]
pub struct SmcEnableDvar {
    pub enabled: Option<bool>,
}

#[derive(Resource, Clone, Copy, Debug)]
pub struct PretessDvar {
    pub enabled: bool,
}

impl Default for PretessDvar {
    fn default() -> Self {
        Self { enabled: true }
    }
}

#[derive(Resource, Clone, Copy, Debug)]
pub struct LodRampDvar {
    pub scale_mid: Option<f32>,
    pub bias_mid: Option<f32>,
    pub scale_last: Option<f32>,

    pub world_unit: Option<f32>,
    pub t5_scale: f32,
    pub t5_bias: f32,
    pub t5_fov_threshold: f32,

    pub t5: assets::t5_lod::LodParmsAxis,
}

impl Default for LodRampDvar {
    fn default() -> Self {
        Self {
            scale_mid: None,
            bias_mid: None,
            scale_last: None,
            world_unit: None,
            t5_scale: assets::t5_lod::R_LOD_SCALE_RIGID_DEFAULT,
            t5_bias: assets::t5_lod::R_LOD_BIAS_RIGID_DEFAULT,
            t5_fov_threshold: assets::t5_lod::R_FOV_SCALE_THRESHOLD_DEFAULT,
            t5: assets::t5_lod::LodParmsAxis::default(),
        }
    }
}

impl LodRampDvar {
    #[must_use]
    pub fn args(self) -> crate::assemble::drawsurf::tess::smodel::LodRampArgs {
        crate::assemble::drawsurf::tess::smodel::LodRampArgs {
            scale_mid: self.scale_mid,
            bias_mid: self.bias_mid,
            scale_last: self.scale_last,
            world_unit: self.world_unit,
            t5: self.t5,
            t5_no_lod_cull_out: false,
        }
    }
}

pub use render_scene::LodRampSkinnedDvar;

#[derive(Resource, Default)]
pub struct FrontendWorkerCmds {
    pub queues: WorkerCmdQueues,
}

pub fn update_lod_parms(
    tan_half_fov_y: f32,
    rigid: &mut LodRampDvar,
    skinned: &mut LodRampSkinnedDvar,
) {
    let (scale, bias, threshold) = (rigid.t5_scale, rigid.t5_bias, rigid.t5_fov_threshold);
    rigid.t5.update(tan_half_fov_y, scale, bias, threshold);
    let (scale, bias, threshold) = (skinned.t5_scale, skinned.t5_bias, skinned.t5_fov_threshold);
    skinned.t5.update(tan_half_fov_y, scale, bias, threshold);
}

#[derive(Resource)]
pub struct WorldStaticModelCache {
    content_revision: u64,
    pub cache_index: Vec<[u16; SMC_CACHE_INDEX_LODS]>,
    leaf_word0: Vec<u32>,
    leaf_base: Vec<u32>,
    leaf_frame: Vec<i32>,

    leaf_verts: Vec<u32>,
    link_next: Vec<u16>,
    link_prev: Vec<u16>,
    trees: Vec<SmcTree>,
    used_head_next: Vec<u16>,
    used_head_prev: Vec<u16>,
    level_sweep: Vec<i32>,
    budget: Vec<i32>,
    frame: i32,
    patch_surfs: u32,
    patch_verts: u32,

    pub indices: Vec<u16>,
    pending_vb: Vec<(SmcPatchLock, Vec<u8>)>,
    pending_ib: Vec<(u32, Vec<u8>)>,

    index_baked: HashSet<u16>,

    pub draw_ranges: HashMap<(u32, u32), (u32, u32)>,

    pub index_runs: HashMap<(u32, u32), Vec<u16>>,

    pending_skin: HashMap<u16, PendingSmcSkin>,
}

struct PendingSmcSkin {
    authored: usize,
    mesh: usize,
    lod: u8,
    origin: [f32; 3],
    axis: [[f32; 3]; 3],
    scale: f32,
    packed_light: [u8; 4],
    lighting_handle: Option<u16>,
    div_0x100_by_height: u32,
    cache_index: u16,
    base_vert_index: u32,
    class_verts: u32,
}

impl WorldStaticModelCache {
    pub fn new(slot_count: usize) -> Self {
        let mut this = Self {
            content_revision: 0,
            cache_index: vec![[0; SMC_CACHE_INDEX_LODS]; slot_count],
            leaf_word0: vec![0; SMC_LEAF_N],
            leaf_base: vec![0; SMC_LEAF_N],
            leaf_frame: vec![0; SMC_LEAF_N],
            leaf_verts: vec![0; SMC_LEAF_N],
            link_next: vec![0; SMC_LINK_N],
            link_prev: vec![0; SMC_LINK_N],
            trees: vec![SmcTree::default(); SMC_TREE_N],
            used_head_next: vec![0; SMC_BANK_N],
            used_head_prev: vec![0; SMC_BANK_N],
            level_sweep: vec![0; SMC_BANK_N * SMC_CLASS_N],
            budget: vec![0; SMC_BANK_N * SMC_CLASS_N],
            frame: 0,
            patch_surfs: 0,
            patch_verts: 0,
            indices: vec![0; SMC_INDEX_U16_N],
            pending_vb: Vec::new(),
            pending_ib: Vec::new(),
            index_baked: HashSet::new(),
            draw_ranges: HashMap::new(),
            index_runs: HashMap::new(),
            pending_skin: HashMap::new(),
        };
        this.with_smc(|smc| smc.clear(0));
        this
    }

    pub fn uncache_slot(&mut self, slot: usize) {
        let Some(indices) = self.cache_index.get(slot).copied() else {
            return;
        };
        let content_len = (
            self.index_baked.len(),
            self.draw_ranges.len(),
            self.index_runs.len(),
        );
        self.with_smc(|smc| {
            for index in indices {
                smc.uncache_index(index);
            }
        });
        for index in indices {
            self.index_baked.remove(&index);
        }
        self.draw_ranges.retain(|(p, _), _| *p != slot as u32);
        self.index_runs.retain(|(p, _), _| *p != slot as u32);
        if content_len
            != (
                self.index_baked.len(),
                self.draw_ranges.len(),
                self.index_runs.len(),
            )
        {
            self.bump_content_revision();
        }
    }

    pub fn bake_indices(
        &mut self,
        base_vert_index: u32,
        xsurface_base_index: u32,
        xsurface_vert_offset: u16,
        tri_count: u16,
        src_indices: &[u16],
    ) -> Result<u32, SmcIndexBakeError> {
        r_cache_static_model_indices(
            &mut self.indices,
            base_vert_index,
            xsurface_base_index,
            xsurface_vert_offset,
            tri_count,
            src_indices,
        )
    }

    pub fn vb_patches(&self) -> &[(SmcPatchLock, Vec<u8>)] {
        &self.pending_vb
    }

    pub fn ib_patches(&self) -> &[(u32, Vec<u8>)] {
        &self.pending_ib
    }

    pub fn baked_cache_indices(&self) -> Vec<u16> {
        let mut v: Vec<u16> = self.index_baked.iter().copied().collect();
        v.sort_unstable();
        v
    }

    pub fn content_revision(&self) -> u64 {
        self.content_revision
    }

    fn bump_content_revision(&mut self) {
        self.content_revision = self.content_revision.wrapping_add(1);
    }

    fn cache_surface(
        &mut self,
        authored_slot: usize,
        lod_slot: u8,
        bank: u8,
        size_class: u8,
        verts: u32,
    ) -> CacheStaticModelSurface {
        let existing = self
            .cache_index
            .get(authored_slot)
            .and_then(|row| row.get(usize::from(lod_slot)))
            .copied()
            .unwrap_or(0);
        let Ok(smodel_index) = u16::try_from(authored_slot) else {
            return CacheStaticModelSurface::Refused;
        };
        let out = {
            let mut result = CacheStaticModelSurface::Refused;
            self.with_smc(|smc| {
                result = smc.cache_surface(
                    bank,
                    size_class,
                    smodel_index,
                    lod_slot,
                    existing,
                    verts,
                    true,
                );
            });
            result
        };
        out
    }

    fn with_smc(&mut self, f: impl FnOnce(&mut StaticModelCache<'_>)) {
        let leaves = SmcLeafStorage::new(
            &mut self.leaf_word0,
            &mut self.leaf_base,
            &mut self.leaf_frame,
            self.cache_index.as_flattened_mut(),
            &mut self.leaf_verts,
        )
        .expect("SMC host leaf arrays are sized at spawn");
        let allocator = SmcAllocatorStorage::new(
            &mut self.link_next,
            &mut self.link_prev,
            &mut self.used_head_next,
            &mut self.used_head_prev,
            &mut self.level_sweep,
            &mut self.budget,
            &mut self.trees,
        )
        .expect("SMC host allocator arrays are sized at spawn");
        let mut smc = StaticModelCache::new(leaves, allocator);
        smc.frame = self.frame;
        smc.patch_surfs = self.patch_surfs;
        smc.patch_verts = self.patch_verts;
        f(&mut smc);
        self.frame = smc.frame;
        self.patch_surfs = smc.patch_surfs;
        self.patch_verts = smc.patch_verts;
    }
}

pub(crate) fn cache_visible_smodel_surfaces(
    mut cache: Option<ResMut<WorldStaticModelCache>>,
    plan: Option<Res<SmodelGpuPlan>>,
    lighting: Option<Res<WorldSmodelLighting>>,
    scene: Res<WorldScene>,
    prepared: Res<PreparedSceneView>,
    lock_pvs: Res<crate::prepare::scene::view_parms::RLockPvs>,
    smc_enable: Res<SmcEnableDvar>,
    lod_ramp: Res<LodRampDvar>,
    mut worker_cmds: ResMut<FrontendWorkerCmds>,
) {
    let Some(cache) = cache.as_mut() else {
        return;
    };
    let Some(plan) = plan else {
        return;
    };
    if plan.exact_packed_vertices().is_err() {
        return;
    }
    if smc_enable.enabled == Some(false) {
        let had_content = !cache.index_baked.is_empty()
            || !cache.draw_ranges.is_empty()
            || !cache.index_runs.is_empty();
        for row in cache.cache_index.iter_mut() {
            *row = [0; lighting_iw4::SMC_CACHE_INDEX_LODS];
        }

        cache.with_smc(|smc| {
            let frame = smc.frame;
            smc.clear(frame);
        });
        cache.index_baked.clear();
        cache.draw_ranges.clear();
        cache.index_runs.clear();
        cache.pending_vb.clear();
        cache.pending_ib.clear();
        cache.pending_skin.clear();
        if had_content {
            cache.bump_content_revision();
        }
        let _ = worker_cmds.queues.reset_type(WORKER_CMD_SMODELCACHE);
        return;
    }
    cache.with_smc(|smc| {
        smc.frame = smc.frame.wrapping_add(1);
        smc.patch_surfs = 0;
        smc.patch_verts = 0;
    });
    cache.pending_vb.clear();
    cache.pending_ib.clear();
    cache.pending_skin.clear();
    let _ = worker_cmds.queues.reset_type(WORKER_CMD_SMODELCACHE);
    let eye = prepared.ready.then_some(lock_pvs.dpvs_eye(&prepared));
    let cull = scene.cull.as_ref();
    let cull_dists = cull
        .map(|c| c.static_model_cull_dists.as_slice())
        .unwrap_or(&[]);
    for (_placement_i, placement) in plan.placements.iter().enumerate() {
        let Some(authored) = placement.lighting_slot else {
            continue;
        };
        let vis = cull
            .and_then(|c| c.smodel_vis.get(authored).copied())
            .unwrap_or(0)
            != 0;
        if !vis {
            continue;
        }
        if smodel_cull_dist_skips_slot(
            cull_dists,
            authored,
            placement.origin,
            eye,
            lod_ramp.scale_last,
        ) {
            continue;
        }
        let Some(lighting) = lighting.as_ref() else {
            continue;
        };
        let packed_light = lighting
            .packed_lighting
            .get(authored)
            .copied()
            .flatten()
            .filter(|bytes| *bytes != [0; 4]);
        let Some(packed_light) = packed_light else {
            continue;
        };
        let lighting_handle = lighting.handles.get(authored).copied().filter(|h| *h != 0);
        let Some(mesh) = plan.meshes.get(placement.mesh) else {
            continue;
        };
        let Some(lod) = smodel_camera_lod(
            mesh.lod,
            placement.origin,
            placement.scale,
            eye,
            lod_ramp.args(),
        ) else {
            continue;
        };
        let lod_i = usize::from(lod);
        let smc_surfs = mesh
            .smc_surfs_by_lod
            .get(lod_i)
            .map(|v| v.as_slice())
            .unwrap_or(&[]);
        if smc_surfs.is_empty() {
            continue;
        }
        let Some(lod_bytes) = mesh
            .lod_smc_rows
            .and_then(|rows| rows.get(lod_i).copied())
            .or(mesh.lod_smc)
        else {
            continue;
        };
        let Some(spec) = smc_lod_cache_spec(lod_bytes, placement.flags) else {
            continue;
        };
        let vert_count = mesh.vert_count_by_lod.get(lod_i).copied().unwrap_or(0);

        let first_patch_vert = cache.patch_verts;
        let result = cache.cache_surface(
            authored,
            spec.lod_slot,
            spec.bank,
            spec.size_class,
            vert_count,
        );
        match result {
            CacheStaticModelSurface::Hit { .. } => {}
            CacheStaticModelSurface::Refused => {}
            CacheStaticModelSurface::Miss {
                cache_index,
                base_vert_index,
                verts: class_verts,
            } => {
                let Some(first_patch_vert) = u16::try_from(first_patch_vert).ok() else {
                    continue;
                };
                let cmd = SkinCachedStaticModelCmd {
                    cache_index,
                    first_patch_vert,
                };
                let job = PendingSmcSkin {
                    authored,
                    mesh: placement.mesh,
                    lod,
                    origin: placement.origin,
                    axis: placement.axis,
                    scale: placement.scale,
                    packed_light,
                    lighting_handle,
                    div_0x100_by_height: lighting.div_0x100_by_height,
                    cache_index,
                    base_vert_index,
                    class_verts,
                };
                match worker_cmds
                    .queues
                    .add(WORKER_CMD_SMODELCACHE, &cmd.to_bytes())
                {
                    Ok(AddWorkerCmd::Queued) => {
                        cache.pending_skin.insert(cache_index, job);
                    }
                    Ok(AddWorkerCmd::OverflowInline) => {
                        skin_cached_static_model_job(cache, &plan, job);
                    }
                    Err(_) => {}
                }
            }
        }
    }
}

pub(crate) fn skin_cached_static_model_cmd(
    mut cache: Option<ResMut<WorldStaticModelCache>>,
    plan: Option<Res<SmodelGpuPlan>>,
    mut worker_cmds: ResMut<FrontendWorkerCmds>,
) {
    let Some(cache) = cache.as_mut() else {
        return;
    };
    let Some(plan) = plan else {
        return;
    };
    let mut drained = Vec::new();
    let _ = worker_cmds.queues.wait_of_type(
        WORKER_CMD_SMODELCACHE,
        WorkerCmdBusyInput::default(),
        |data| {
            if let Some(cmd) = SkinCachedStaticModelCmd::from_bytes(data) {
                drained.push(cmd);
            }
        },
    );
    for cmd in drained {
        let Some(job) = cache.pending_skin.remove(&cmd.cache_index) else {
            continue;
        };
        skin_cached_static_model_job(cache, &plan, job);
    }
}

fn skin_cached_static_model_job(
    cache: &mut WorldStaticModelCache,
    plan: &SmodelGpuPlan,
    job: PendingSmcSkin,
) {
    let packed_rows = match plan.exact_packed_vertices() {
        Ok(rows) => rows,
        Err(_) => return,
    };
    let Some(lock) =
        lighting_iw4::rb_patch_static_model_cache_lock(job.base_vert_index, job.class_verts)
    else {
        return;
    };
    let Some(mesh) = plan.meshes.get(job.mesh) else {
        return;
    };
    let smc_surfs = mesh
        .smc_surfs_by_lod
        .get(usize::from(job.lod))
        .map(Vec::as_slice)
        .unwrap_or(&[]);
    let mut dest = vec![0u8; lock.byte_count as usize];
    let m = r_skin_cached_static_model_cmd_matrix(job.origin, job.axis, job.scale);

    let normal_matrix = r_skin_cached_static_model_cmd_matrix([0.0; 3], job.axis, 1.0);
    let fixed_norm_axis = lighting_iw4::setup_transform_unit_vec(&normal_matrix);
    let mut skin_surfs = Vec::new();
    for surf in smc_surfs {
        let start = surf.packed_off as usize;
        let end = start.saturating_add(surf.packed_n as usize);
        let Some(src) = packed_rows.get(start..end) else {
            continue;
        };
        let Some(state_flags) = surf
            .material
            .and_then(|material| plan.material_state_flags.get(&material).copied())
        else {
            continue;
        };
        let lighting = if state_flags & 2 != 0 {
            SmcCachedVertLighting::Packed(job.packed_light)
        } else {
            let Some(lighting_handle) = job.lighting_handle else {
                continue;
            };
            SmcCachedVertLighting::FromHandle {
                lighting_handle,
                div_0x100_by_height: job.div_0x100_by_height,
            }
        };
        skin_surfs.push(SmcSkinSurface {
            packed: src,
            vert_offset: surf.xsurface_vert_offset,
            lighting,
        });
    }
    let _ = r_skin_cached_static_model_cmd(&mut dest, &m, &fixed_norm_axis, &skin_surfs);
    let mut baked_any = false;
    for surf in smc_surfs {
        let Some(src_ix) = local_u16_indices(
            plan.indices(),
            surf.index_start,
            surf.index_count,
            surf.vert_base,
        ) else {
            continue;
        };
        let tri_count = (surf.index_count / 3) as u16;
        let written = usize::from(tri_count >> 1).saturating_mul(6);
        if let Ok(slot) = cache.bake_indices(
            job.base_vert_index,
            u32::from(surf.xsurface_base_index),
            surf.xsurface_vert_offset,
            tri_count,
            &src_ix,
        ) {
            let off = slot as usize;
            let run = cache
                .indices
                .get(off..off.saturating_add(written))
                .map(|run| run.to_vec());
            if let Some(run) = run {
                cache
                    .pending_ib
                    .push((slot.saturating_mul(2), bytemuck::cast_slice(&run).to_vec()));
                cache
                    .index_runs
                    .insert((job.authored as u32, surf.range_idx), run);
            }
            cache.draw_ranges.insert(
                (job.authored as u32, surf.range_idx),
                (slot, written as u32),
            );
            baked_any = true;
        }
    }
    cache.pending_vb.push((lock, dest));
    if baked_any {
        cache.index_baked.insert(job.cache_index);
        cache.bump_content_revision();
    }
}

fn local_u16_indices(indices: &[u32], start: u32, count: u32, vert_base: u32) -> Option<Vec<u16>> {
    let s = start as usize;
    let e = s.checked_add(count as usize)?;
    let slice = indices.get(s..e)?;
    let mut out = Vec::with_capacity(slice.len());
    for &i in slice {
        out.push(u16::try_from(i.checked_sub(vert_base)?).ok()?);
    }
    Some(out)
}
