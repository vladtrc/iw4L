use crate::smodel_lighting::SMC_CACHE_INDEX_LODS;

pub const SMC_VB_BYTES: u32 = 0x00c0_0000;

pub const SMC_VB_VERTS: u32 = 393_216;

pub const SMC_INDEX_U16_N: usize = (SMC_VB_VERTS as usize) * 4;

pub const SMC_VERT_STRIDE: u32 = 0x20;

pub const SMC_VB_CREATE_USAGE: u32 = 0x208;

pub const SMC_PATCH_LOCK_NOOVERWRITE: u32 = 0x1000;

pub const SMC_LEAF_N: usize = 0x6000;

pub const SMC_TREE_N: usize = 192;
pub const SMC_TREES_PER_BANK: usize = 32;
pub const SMC_BANK_N: usize = 6;
pub const SMC_CLASS_N: usize = 25;

pub const SMC_LINK_N: usize = 0x612c;
pub const SMC_PATCH_SURF_MAX: u32 = 0x100;
pub const SMC_PATCH_VERT_MAX: u32 = 0x2000;

pub const SMC_IDLE_FRAMES: u32 = 4;

pub const SMC_SIZE_CLASS_VERTS: [u32; SMC_CLASS_N] = [
    16, 21, 28, 32, 36, 42, 51, 56, 64, 73, 85, 102, 113, 128, 146, 170, 204, 227, 256, 292, 341,
    409, 512, 682, 1024,
];

const SMC_PARENT: [u8; SMC_CLASS_N] = [
    3, 5, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19, 20, 21, 0x19, 22, 0x19, 23, 0x19, 24,
    0x19, 0x19,
];

const SMC_CHILD: [u8; SMC_CLASS_N] = [
    0x19, 0x19, 0x19, 0, 0x19, 1, 0x19, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 18, 20,
    22,
];

const SMC_SIBLING_STRIDE: [u8; SMC_CLASS_N] = [
    1, 1, 1, 2, 1, 2, 1, 2, 4, 2, 4, 2, 4, 8, 4, 8, 4, 8, 16, 8, 16, 8, 32, 32, 64,
];

const SMC_CLASS_LEVEL: [u8; SMC_CLASS_N] = [
    0, 1, 4, 0, 3, 1, 2, 4, 0, 3, 1, 2, 4, 0, 3, 1, 2, 4, 0, 3, 1, 2, 0, 1, 0,
];

const SMC_LEVEL_BLOCKS: [u32; 5] = [2, 3, 5, 7, 9];

const SMC_LEVEL_VERT_STRIDE: [u32; 5] = [0x100, 0x155, 0x332, 0x248, 0x1c6];

const SMC_LEVEL_LEAF_RANGE: [u16; 5] = [0x80, 0x60, 0x28, 0x38, 0x48];

const SMC_TREE_LEVEL_FREE: i32 = 6;

const TREE_SENTINEL: u16 = 0xffff;

pub fn smc_size_class_for_verts(verts: u32) -> Option<u8> {
    SMC_SIZE_CLASS_VERTS
        .iter()
        .position(|&n| n >= verts)
        .map(|i| i as u8)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SmcLodCacheSpec {
    pub lod_slot: u8,
    pub bank: u8,
    pub size_class: u8,
}

pub fn smc_lod_cache_spec(lod_info_28_2b: [u8; 4], draw_inst_flags: u8) -> Option<SmcLodCacheSpec> {
    let [lod_slot, b29, b2a, size_class] = lod_info_28_2b;
    if b29 == 0 {
        return None;
    }
    if usize::from(lod_slot) >= SMC_CACHE_INDEX_LODS || usize::from(size_class) >= SMC_CLASS_N {
        return None;
    }
    Some(SmcLodCacheSpec {
        lod_slot,
        bank: smc_index(b29, b2a, draw_inst_flags)?,
        size_class,
    })
}

pub fn smc_index(lod_info_29: u8, lod_info_2a: u8, draw_inst_flags: u8) -> Option<u8> {
    let raw = u32::from(lod_info_29)
        .wrapping_add(u32::from((lod_info_2a & draw_inst_flags) & 7))
        .wrapping_sub(1);
    (raw < SMC_BANK_N as u32).then_some(raw as u8)
}

pub const SMODEL_BUCKET_STRIDE: i32 = 4;
pub const SMODEL_BUCKET_RIGID: i32 = 0;
pub const SMODEL_BUCKET_SKINNED: i32 = 1;
pub const SMODEL_BUCKET_CACHED: i32 = 2;
pub const SMODEL_BUCKET_PRETESS: i32 = 3;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SmodelSurfPath {
    Rigid,
    Skinned,
    Cached,
    Pretess,
}

#[must_use]
pub const fn r_smodel_surf_type(path: SmodelSurfPath) -> u8 {
    match path {
        SmodelSurfPath::Rigid => 2,
        SmodelSurfPath::Skinned => 5,
        SmodelSurfPath::Cached => 4,
        SmodelSurfPath::Pretess => 3,
    }
}

#[must_use]
pub fn r_smodel_bucket_source_path(bucket: i32) -> Option<SmodelSurfPath> {
    match bucket & 3 {
        SMODEL_BUCKET_RIGID => Some(SmodelSurfPath::Rigid),
        SMODEL_BUCKET_SKINNED => Some(SmodelSurfPath::Skinned),
        SMODEL_BUCKET_CACHED => Some(SmodelSurfPath::Cached),
        _ => None,
    }
}

#[must_use]
pub fn r_smodel_dest_path(
    source: SmodelSurfPath,
    cached_pretess_succeeded: bool,
) -> SmodelSurfPath {
    if source == SmodelSurfPath::Cached && cached_pretess_succeeded {
        SmodelSurfPath::Pretess
    } else {
        source
    }
}

#[must_use]
pub fn r_smodel_lod_is_rigid(surf_plus_1: &[u8]) -> bool {
    surf_plus_1.iter().all(|&b| b == 0)
}

#[must_use]
pub fn r_add_static_model_surf_to_bucket(
    lod: i32,
    smc_enable: bool,
    draw_inst_lighting_nonzero: bool,
    lodinfo_plus_0x29: u8,
    cache_index: u16,
    lod_is_rigid: bool,
) -> i32 {
    if smc_enable && draw_inst_lighting_nonzero && lodinfo_plus_0x29 != 0 && cache_index != 0 {
        return lod * SMODEL_BUCKET_STRIDE + SMODEL_BUCKET_CACHED;
    }
    if !lod_is_rigid {
        return lod * SMODEL_BUCKET_STRIDE + SMODEL_BUCKET_SKINNED;
    }
    lod * SMODEL_BUCKET_STRIDE + SMODEL_BUCKET_RIGID
}

pub const SMODEL_BUCKET_LIST_N: usize = 16;
pub const SMODEL_BUCKET_CAP: usize = 0x80;

#[derive(Clone, Copy, Debug)]
pub struct SmodelSurfBucketLists {
    pub ids: [[u16; SMODEL_BUCKET_CAP]; SMODEL_BUCKET_LIST_N],
    pub count: [i32; SMODEL_BUCKET_LIST_N],
}

impl Default for SmodelSurfBucketLists {
    fn default() -> Self {
        Self {
            ids: [[0; SMODEL_BUCKET_CAP]; SMODEL_BUCKET_LIST_N],
            count: [0; SMODEL_BUCKET_LIST_N],
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SmodelBucketPush {
    Stored,
    Full,
}

#[must_use]
pub fn r_smodel_bucket_store_payload(bucket: i32, smodel_index: u16, cache_index: u16) -> u16 {
    if bucket & 3 == SMODEL_BUCKET_CACHED {
        cache_index
    } else {
        smodel_index
    }
}

#[must_use]
pub fn r_smodel_surf_bucket_push(
    lists: &mut SmodelSurfBucketLists,
    bucket: i32,
    payload: u16,
) -> Option<SmodelBucketPush> {
    let b = usize::try_from(bucket).ok()?;
    if b >= SMODEL_BUCKET_LIST_N {
        return None;
    }
    let n = lists.count[b];
    if n < 0 || (n as usize) >= SMODEL_BUCKET_CAP {
        return None;
    }
    lists.ids[b][n as usize] = payload;
    lists.count[b] = n + 1;
    Some(if lists.count[b] == SMODEL_BUCKET_CAP as i32 {
        SmodelBucketPush::Full
    } else {
        SmodelBucketPush::Stored
    })
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SmodelConsumedBucket {
    pub bucket: u8,
    pub lod: u8,
    pub source: SmodelSurfPath,
    pub count: u16,
    pub ids: [u16; SMODEL_BUCKET_CAP],
}

#[must_use]
pub const fn r_smodel_bucket_mask(bucket: u8) -> Option<u32> {
    if bucket < SMODEL_BUCKET_LIST_N as u8 {
        Some(0x8000_0000u32 >> bucket)
    } else {
        None
    }
}

pub fn smodel_bucket_lists_consume(
    lists: &mut SmodelSurfBucketLists,
    active_mask: u32,
    out: &mut [Option<SmodelConsumedBucket>; SMODEL_BUCKET_LIST_N],
) -> usize {
    out.fill(None);
    let mut written = 0usize;
    for bucket in 0..SMODEL_BUCKET_LIST_N {
        if active_mask & (0x8000_0000u32 >> bucket) == 0 {
            continue;
        }
        let Some(source) = r_smodel_bucket_source_path(bucket as i32) else {
            continue;
        };
        let count = lists.count[bucket];
        if count <= 0 || count as usize > SMODEL_BUCKET_CAP {
            continue;
        }
        let mut ids = [0u16; SMODEL_BUCKET_CAP];
        ids[..count as usize].copy_from_slice(&lists.ids[bucket][..count as usize]);
        out[written] = Some(SmodelConsumedBucket {
            bucket: bucket as u8,
            lod: (bucket / SMODEL_BUCKET_STRIDE as usize) as u8,
            source,
            count: count as u16,
            ids,
        });
        written += 1;
        lists.count[bucket] = 0;
    }
    written
}

#[inline]
#[must_use]
pub const fn smodel_surf_sun_shadow_emits(material_plus_4: u8) -> bool {
    material_plus_4 & 0xc0 != 0
}

#[inline]
pub const fn smc_cache_index_offset(lod: u8) -> usize {
    crate::smodel_lighting::GFX_STATIC_MODEL_DRAW_INST_CACHE_INDEX + (lod as usize) * 2
}

pub const SMC_BANK_VERTS: u32 = 0x10000;

pub const SMC_BANK_VB_BYTES: u32 = 0x0020_0000;

pub fn r_smc_stream_source_byte_offset(cache_index: u16) -> Option<u32> {
    if cache_index == 0 {
        return None;
    }
    let leaf = u32::from(cache_index.wrapping_sub(1));
    if (leaf as usize) >= SMC_LEAF_N {
        return None;
    }
    Some((leaf & 0xffff_f000) << 9)
}

pub fn r_cache_static_model_indices_u16_slot(
    base_vert_index: u32,
    xsurface_base_index: u32,
) -> Option<u32> {
    let verts = base_vert_index.checked_mul(4)?;
    let tris = xsurface_base_index.checked_mul(3)?;
    verts.checked_add(tris)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SmcIndexBakeError {
    ShortSource,
    ShortDest,
    SlotOverflow,
}

pub fn r_cache_static_model_indices(
    dest: &mut [u16],
    base_vert_index: u32,
    xsurface_base_index: u32,
    xsurface_vert_offset: u16,
    tri_count: u16,
    src_indices: &[u16],
) -> Result<u32, SmcIndexBakeError> {
    let pairs = usize::from(tri_count >> 1);
    let u16_n = pairs.saturating_mul(6);
    if src_indices.len() < u16_n {
        return Err(SmcIndexBakeError::ShortSource);
    }
    let slot = r_cache_static_model_indices_u16_slot(base_vert_index, xsurface_base_index)
        .ok_or(SmcIndexBakeError::SlotOverflow)?;
    let end = (slot as usize)
        .checked_add(u16_n)
        .ok_or(SmcIndexBakeError::SlotOverflow)?;
    if end > dest.len() {
        return Err(SmcIndexBakeError::ShortDest);
    }
    let addend = (base_vert_index.wrapping_add(u32::from(xsurface_vert_offset))) as u16;
    let start = slot as usize;
    for i in 0..u16_n {
        dest[start + i] = src_indices[i].wrapping_add(addend);
    }
    Ok(slot)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SmcDrawCacheIndex {
    Empty,
    Sole(u16),
    Ambiguous,
}

pub fn r_smc_draw_cache_index(row: [u16; SMC_CACHE_INDEX_LODS]) -> SmcDrawCacheIndex {
    let mut found = None;
    for index in row {
        if index == 0 {
            continue;
        }
        if found.is_some() {
            return SmcDrawCacheIndex::Ambiguous;
        }
        found = Some(index);
    }
    match found {
        None => SmcDrawCacheIndex::Empty,
        Some(index) => SmcDrawCacheIndex::Sole(index),
    }
}

fn free_sentinel(bank: usize, class: usize) -> u16 {
    (bank + (class * 3 + 0x3000) * 2) as u16
}

fn used_sentinel(bank: usize, class: usize) -> u16 {
    (bank + (class * 3 + 0x304b) * 2) as u16
}

#[derive(Clone, Copy, Debug, Default)]
pub struct SmcTree {
    next: u16,
    prev: u16,
    level: i32,
    used_verts: i32,
    frame: i32,
}

pub struct StaticModelCache<'a> {
    leaf_word0: &'a mut [u32],
    leaf_base: &'a mut [u32],
    leaf_frame: &'a mut [i32],

    cache_index: &'a mut [u16],

    leaf_verts: &'a mut [u32],
    link_next: &'a mut [u16],
    link_prev: &'a mut [u16],
    trees: &'a mut [SmcTree],
    used_head_next: &'a mut [u16],
    used_head_prev: &'a mut [u16],
    level_sweep: &'a mut [i32],
    budget: &'a mut [i32],
    pub frame: i32,
    pub patch_surfs: u32,
    pub patch_verts: u32,
}

pub struct SmcLeafStorage<'a> {
    leaf_word0: &'a mut [u32],
    leaf_base: &'a mut [u32],
    leaf_frame: &'a mut [i32],
    cache_index: &'a mut [u16],
    leaf_verts: &'a mut [u32],
}

impl<'a> SmcLeafStorage<'a> {
    pub fn new(
        leaf_word0: &'a mut [u32],
        leaf_base: &'a mut [u32],
        leaf_frame: &'a mut [i32],
        cache_index: &'a mut [u16],
        leaf_verts: &'a mut [u32],
    ) -> Result<Self, SmcCacheError> {
        if leaf_word0.len() < SMC_LEAF_N
            || leaf_base.len() < SMC_LEAF_N
            || leaf_frame.len() < SMC_LEAF_N
            || leaf_verts.len() < SMC_LEAF_N
            || !cache_index.len().is_multiple_of(SMC_CACHE_INDEX_LODS)
        {
            return Err(SmcCacheError::ShortArray);
        }
        Ok(Self {
            leaf_word0,
            leaf_base,
            leaf_frame,
            cache_index,
            leaf_verts,
        })
    }
}

pub struct SmcAllocatorStorage<'a> {
    link_next: &'a mut [u16],
    link_prev: &'a mut [u16],
    used_head_next: &'a mut [u16],
    used_head_prev: &'a mut [u16],
    level_sweep: &'a mut [i32],
    budget: &'a mut [i32],
    trees: &'a mut [SmcTree],
}

impl<'a> SmcAllocatorStorage<'a> {
    pub fn new(
        link_next: &'a mut [u16],
        link_prev: &'a mut [u16],
        used_head_next: &'a mut [u16],
        used_head_prev: &'a mut [u16],
        level_sweep: &'a mut [i32],
        budget: &'a mut [i32],
        trees: &'a mut [SmcTree],
    ) -> Result<Self, SmcCacheError> {
        if link_next.len() < SMC_LINK_N
            || link_prev.len() < SMC_LINK_N
            || trees.len() < SMC_TREE_N
            || used_head_next.len() < SMC_BANK_N
            || used_head_prev.len() < SMC_BANK_N
            || level_sweep.len() < SMC_BANK_N * SMC_CLASS_N
            || budget.len() < SMC_BANK_N * SMC_CLASS_N
        {
            return Err(SmcCacheError::ShortArray);
        }
        Ok(Self {
            link_next,
            link_prev,
            used_head_next,
            used_head_prev,
            level_sweep,
            budget,
            trees,
        })
    }
}

#[derive(Clone, Copy, Debug)]
pub struct SmcStorageLens {
    pub leaf_n: usize,
    pub link_n: usize,
    pub tree_n: usize,
    pub bank_n: usize,
    pub class_n: usize,
}

pub const SMC_STORAGE_LENS: SmcStorageLens = SmcStorageLens {
    leaf_n: SMC_LEAF_N,
    link_n: SMC_LINK_N,
    tree_n: SMC_TREE_N,
    bank_n: SMC_BANK_N,
    class_n: SMC_CLASS_N,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SmcCacheError {
    ShortArray,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CacheStaticModelSurface {
    Hit {
        cache_index: u16,
    },

    Miss {
        cache_index: u16,
        base_vert_index: u32,
        verts: u32,
    },

    Refused,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SmcPatchLock {
    pub byte_offset: u32,
    pub byte_count: u32,
    pub lock_flags: u32,
}

impl SmcPatchLock {
    pub fn accepts_bytes(self, byte_len: usize) -> bool {
        byte_len == self.byte_count as usize && self.lock_flags == SMC_PATCH_LOCK_NOOVERWRITE
    }
}

pub fn rb_patch_static_model_cache_lock(
    base_vert_index: u32,
    class_verts: u32,
) -> Option<SmcPatchLock> {
    let byte_offset = base_vert_index.checked_mul(SMC_VERT_STRIDE)?;
    let byte_count = class_verts.checked_mul(SMC_VERT_STRIDE)?;
    let end = byte_offset.checked_add(byte_count)?;
    if end > SMC_VB_BYTES {
        return None;
    }
    Some(SmcPatchLock {
        byte_offset,
        byte_count,
        lock_flags: SMC_PATCH_LOCK_NOOVERWRITE,
    })
}

pub fn rb_patch_static_model_cache_lock_for_miss(
    miss: CacheStaticModelSurface,
) -> Option<SmcPatchLock> {
    match miss {
        CacheStaticModelSurface::Miss {
            base_vert_index,
            verts,
            ..
        } => rb_patch_static_model_cache_lock(base_vert_index, verts),
        CacheStaticModelSurface::Hit { .. } | CacheStaticModelSurface::Refused => None,
    }
}

impl<'a> StaticModelCache<'a> {
    pub fn new(leaves: SmcLeafStorage<'a>, allocator: SmcAllocatorStorage<'a>) -> Self {
        Self {
            leaf_word0: leaves.leaf_word0,
            leaf_base: leaves.leaf_base,
            leaf_frame: leaves.leaf_frame,
            cache_index: leaves.cache_index,
            leaf_verts: leaves.leaf_verts,
            link_next: allocator.link_next,
            link_prev: allocator.link_prev,
            trees: allocator.trees,
            used_head_next: allocator.used_head_next,
            used_head_prev: allocator.used_head_prev,
            level_sweep: allocator.level_sweep,
            budget: allocator.budget,
            frame: 0,
            patch_surfs: 0,
            patch_verts: 0,
        }
    }

    pub fn clear(&mut self, frame: i32) {
        self.frame = frame;
        self.patch_surfs = 0;
        self.patch_verts = 0;
        let idle = frame.wrapping_sub(SMC_IDLE_FRAMES as i32);
        self.leaf_word0[..SMC_LEAF_N].fill(u32::MAX);
        self.leaf_base[..SMC_LEAF_N].fill(u32::MAX);
        self.leaf_frame[..SMC_LEAF_N].fill(idle);
        self.leaf_verts[..SMC_LEAF_N].fill(0);
        self.link_next[..SMC_LINK_N].fill(0);
        self.link_prev[..SMC_LINK_N].fill(0);
        for bank in 0..SMC_BANK_N {
            let base = bank * SMC_TREES_PER_BANK;
            let last = base + SMC_TREES_PER_BANK - 1;
            self.used_head_next[bank] = last as u16;
            self.used_head_prev[bank] = base as u16;
            for t in 0..SMC_TREES_PER_BANK {
                let i = base + t;
                self.trees[i] = SmcTree {
                    next: if t == 0 {
                        TREE_SENTINEL
                    } else {
                        (i - 1) as u16
                    },
                    prev: if t + 1 == SMC_TREES_PER_BANK {
                        TREE_SENTINEL
                    } else {
                        (i + 1) as u16
                    },
                    level: SMC_TREE_LEVEL_FREE,
                    used_verts: 0,
                    frame: idle,
                };
            }
            for class in 0..SMC_CLASS_N {
                let f = free_sentinel(bank, class);
                let u = used_sentinel(bank, class);
                let fi = usize::from(f);
                let ui = usize::from(u);
                self.link_next[fi] = f;
                self.link_prev[fi] = f;
                self.link_next[ui] = u;
                self.link_prev[ui] = u;
                self.level_sweep[bank * SMC_CLASS_N + class] = idle;
            }
        }
        self.budget[..SMC_BANK_N * SMC_CLASS_N].fill(-1);
    }

    pub fn cache_surface(
        &mut self,
        bank: u8,
        size_class: u8,
        smodel_index: u16,
        lod: u8,
        cache_index: u16,
        verts: u32,
        enabled: bool,
    ) -> CacheStaticModelSurface {
        if !enabled {
            return CacheStaticModelSurface::Refused;
        }
        if cache_index != 0 {
            self.touch_hit(cache_index, bank);
            return CacheStaticModelSurface::Hit { cache_index };
        }
        let Some(class) = (size_class < SMC_CLASS_N as u8)
            .then_some(size_class)
            .or_else(|| smc_size_class_for_verts(verts))
        else {
            return CacheStaticModelSurface::Refused;
        };
        let class_verts = SMC_SIZE_CLASS_VERTS[usize::from(class)];
        if self.patch_surfs >= SMC_PATCH_SURF_MAX
            || self.patch_verts.saturating_add(class_verts) > SMC_PATCH_VERT_MAX
        {
            return CacheStaticModelSurface::Refused;
        }
        let Some(new_index) = self.allocate(bank, class) else {
            return CacheStaticModelSurface::Refused;
        };
        let leaf = usize::from(new_index.wrapping_sub(1));
        self.leaf_word0[leaf] =
            u32::from(smodel_index) | u32::from(lod) << 16 | u32::from(class) << 24;
        self.write_cache_index(smodel_index, lod, new_index);
        self.leaf_verts[leaf] = verts;
        let tree = leaf >> 7;
        self.trees[tree].used_verts = self.trees[tree].used_verts.saturating_add(verts as i32);
        self.patch_surfs = self.patch_surfs.saturating_add(1);
        self.patch_verts = self.patch_verts.saturating_add(class_verts);
        self.touch_hit(new_index, bank);
        CacheStaticModelSurface::Miss {
            cache_index: new_index,
            base_vert_index: self.leaf_base[leaf],
            verts: class_verts,
        }
    }

    pub fn uncache_index(&mut self, cache_index: u16) {
        if cache_index == 0 {
            return;
        }
        let leaf = cache_index.wrapping_sub(1);
        if usize::from(leaf) >= SMC_LEAF_N {
            return;
        }
        let class = (self.leaf_word0[usize::from(leaf)] >> 24) as u8;
        let bank = (usize::from(leaf) >> 7) / SMC_TREES_PER_BANK;
        self.uncache_leaf(leaf);
        self.merge_free(bank, class, leaf);
    }

    fn uncache_leaf(&mut self, leaf: u16) {
        let i = usize::from(leaf);
        let word0 = self.leaf_word0[i];
        let smodel_index = word0 as u16;
        let lod = (word0 >> 16) as u8;
        let class = (word0 >> 24) as u8;
        let tree = i >> 7;
        self.trees[tree].used_verts = self.trees[tree]
            .used_verts
            .saturating_sub(self.leaf_verts[i] as i32);
        self.leaf_verts[i] = 0;
        self.write_cache_index(smodel_index, lod, 0);
        self.leaf_word0[i] = u32::from(class) << 24 | 0xffff;
        self.unlink(leaf);
    }

    fn write_cache_index(&mut self, smodel_index: u16, lod: u8, value: u16) {
        let slot = usize::from(smodel_index) * SMC_CACHE_INDEX_LODS + usize::from(lod);
        assert!(
            usize::from(lod) < SMC_CACHE_INDEX_LODS && slot < self.cache_index.len(),
            "SMC leaf names a DrawInst slot the host did not allocate: smodel={smodel_index} lod={lod} rows={}",
            self.cache_index.len() / SMC_CACHE_INDEX_LODS
        );
        self.cache_index[slot] = value;
    }

    pub fn allocate(&mut self, bank: u8, size_class: u8) -> Option<u16> {
        let bank = usize::from(bank);
        let class = usize::from(size_class);
        if bank >= SMC_BANK_N || class >= SMC_CLASS_N {
            return None;
        }
        let b = &mut self.budget[bank * SMC_CLASS_N + class];
        if *b == 0 {
            return None;
        }
        if *b > 0 {
            *b -= 1;
        }
        let head = free_sentinel(bank, class);
        if self.link_next[usize::from(head)] == head && !self.get_free_block_of_size(bank, class) {
            return None;
        }
        let node = self.link_next[usize::from(head)];
        self.unlink(node);
        self.push_front(used_sentinel(bank, class), node);
        let level = usize::from(SMC_CLASS_LEVEL[class]);
        let leaf = u32::from(node);
        self.leaf_base[usize::from(node)] = (leaf >> 4 & 7) * SMC_LEVEL_VERT_STRIDE[level]
            + smc_base_off(level, (leaf & 0xf) as usize)
            + (leaf >> 7) * 0x800;
        Some(node.wrapping_add(1))
    }

    fn touch_hit(&mut self, cache_index: u16, bank: u8) {
        let leaf = usize::from(cache_index.wrapping_sub(1));
        let tree = leaf >> 7;
        self.leaf_frame[leaf] = self.frame;
        if self.trees[tree].frame != self.frame {
            self.trees[tree].frame = self.frame;
            self.unlink_tree(tree, usize::from(bank));
            self.push_tree_front(tree, usize::from(bank));
        }
    }

    fn get_free_block_of_size(&mut self, bank: usize, class: usize) -> bool {
        self.free_old_blocks(bank, class);
        let head = free_sentinel(bank, class);
        if self.link_next[usize::from(head)] != head {
            return true;
        }
        let parent = SMC_PARENT[class];
        if parent == 0x19 {
            return self.force_free_block(bank, class);
        }
        let parent = usize::from(parent);
        let phead = free_sentinel(bank, parent);
        if self.link_next[usize::from(phead)] == phead && !self.get_free_block_of_size(bank, parent)
        {
            return false;
        }
        let node = self.link_next[usize::from(phead)];
        self.unlink(node);
        let stride = u16::from(SMC_SIBLING_STRIDE[class]);
        let sib = node.wrapping_add(stride);
        let tag = (class as u32) << 24 | 0xffff;
        self.leaf_word0[usize::from(node)] = tag;
        if usize::from(sib) < SMC_LEAF_N {
            self.leaf_word0[usize::from(sib)] = tag;
        }

        if usize::from(sib) < SMC_LEAF_N {
            self.push_front(head, sib);
        }
        self.push_front(head, node);
        true
    }

    fn free_old_blocks(&mut self, bank: usize, class: usize) {
        let slot = bank * SMC_CLASS_N + class;
        if (self.frame.wrapping_sub(self.level_sweep[slot]) as u32) <= 3 {
            return;
        }
        self.level_sweep[slot] = self.frame;
        let child = SMC_CHILD[class];
        if child != 0x19 {
            self.free_old_blocks(bank, usize::from(child));
        }
        let head = used_sentinel(bank, class);
        let mut cur = self.link_next[usize::from(head)];
        let mut walked = 0usize;
        while cur != head {
            assert!(
                walked < SMC_LEAF_N,
                "SMC used-block ring does not reach its sentinel: bank={bank} class={class} head={head} node={cur}"
            );
            walked += 1;
            let next = self.link_next[usize::from(cur)];
            let leaf = usize::from(cur);
            if leaf < SMC_LEAF_N && (self.frame.wrapping_sub(self.leaf_frame[leaf]) as u32) > 3 {
                self.uncache_index(cur.wrapping_add(1));
            }
            cur = next;
        }
    }

    fn force_free_block(&mut self, bank: usize, class: usize) -> bool {
        let lru = self.used_head_prev[bank];
        if lru == TREE_SENTINEL {
            return false;
        }
        let tree = usize::from(lru);
        if (self.frame.wrapping_sub(self.trees[tree].frame) as u32) < SMC_IDLE_FRAMES {
            return false;
        }
        self.unlink_tree(tree, bank);
        let first = (tree as u16) << 7;
        let old_level = self.trees[tree].level;
        if old_level != SMC_TREE_LEVEL_FREE {
            let span = usize::try_from(old_level)
                .ok()
                .and_then(|l| SMC_LEVEL_LEAF_RANGE.get(l).copied());
            let Some(span) = span else {
                panic!("SMC tree carved at an unknown level: tree={tree} level={old_level}");
            };
            self.free_cached_surface_range(tree, first, first.wrapping_add(span));
        }
        let level = usize::from(SMC_CLASS_LEVEL[class]);
        self.trees[tree].level = i32::from(SMC_CLASS_LEVEL[class]);
        let count = SMC_LEVEL_BLOCKS[level] as usize;
        let stride = u16::from(SMC_SIBLING_STRIDE[class]);
        let head = free_sentinel(bank, class);
        let mut leaf = first;
        let tag = (class as u32) << 24 | 0xffff;
        for _ in 0..count {
            let i = usize::from(leaf);
            if i < SMC_LEAF_N {
                self.leaf_word0[i] = tag;
                self.push_back(head, leaf);
            }
            leaf = leaf.wrapping_add(stride);
        }
        true
    }

    fn free_cached_surface_range(&mut self, tree: usize, first: u16, end: u16) {
        let mut cur = first;
        while cur != end {
            let i = usize::from(cur);
            assert!(
                i < SMC_LEAF_N,
                "SMC block range left the leaf array: tree={tree} first={first} end={end} leaf={cur}"
            );
            let word0 = self.leaf_word0[i];
            let class = usize::from((word0 >> 24) as u8);
            assert!(
                class < SMC_CLASS_N,
                "SMC block range hit a leaf with no size class: tree={tree} leaf={cur} word0={word0:#010x}"
            );
            if word0 as u16 == 0xffff {
                self.unlink(cur);
            } else {
                self.uncache_leaf(cur);
            }
            cur = cur.wrapping_add(u16::from(SMC_SIBLING_STRIDE[class]));
        }
        self.trees[tree].level = SMC_TREE_LEVEL_FREE;
    }

    fn merge_free(&mut self, bank: usize, mut class: u8, mut leaf: u16) {
        let mut tag = u32::from(class) << 24 | 0xffff;
        loop {
            let parent = SMC_PARENT[usize::from(class)];
            if parent == 0x19 {
                break;
            }
            let stride = u16::from(SMC_SIBLING_STRIDE[usize::from(class)]);
            let sib = leaf ^ stride;
            if usize::from(sib) >= SMC_LEAF_N || self.leaf_word0[usize::from(sib)] != tag {
                break;
            }
            self.unlink(sib);
            class = parent;
            leaf &= !stride;
            tag = u32::from(class) << 24 | 0xffff;
            self.leaf_word0[usize::from(leaf)] = tag;
        }
        self.push_front(free_sentinel(bank, usize::from(class)), leaf);
    }

    fn unlink(&mut self, node: u16) {
        let i = usize::from(node);
        assert!(
            i < SMC_LEAF_N,
            "SMC tried to unlink a sentinel as a cache leaf: node={node}"
        );
        let next = self.link_next[i];
        let prev = self.link_prev[i];
        self.link_prev[usize::from(next)] = prev;
        self.link_next[usize::from(prev)] = next;
    }

    fn push_front(&mut self, head: u16, node: u16) {
        let h = usize::from(head);
        let n = usize::from(node);
        assert!(
            n < SMC_LEAF_N,
            "SMC tried to insert a sentinel as a cache leaf: head={head} node={node}"
        );
        let first = self.link_next[h];
        self.link_next[n] = first;
        self.link_prev[n] = head;
        self.link_prev[usize::from(first)] = node;
        self.link_next[h] = node;
    }

    fn push_back(&mut self, head: u16, node: u16) {
        let h = usize::from(head);
        let n = usize::from(node);
        assert!(
            n < SMC_LEAF_N,
            "SMC tried to append a sentinel as a cache leaf: head={head} node={node}"
        );
        let last = self.link_prev[h];
        self.link_prev[n] = last;
        self.link_next[n] = head;
        self.link_next[usize::from(last)] = node;
        self.link_prev[h] = node;
    }

    fn unlink_tree(&mut self, tree: usize, bank: usize) {
        let next = self.trees[tree].next;
        let prev = self.trees[tree].prev;
        if next == TREE_SENTINEL {
            self.used_head_prev[bank] = prev;
        } else {
            self.trees[usize::from(next)].prev = prev;
        }
        if prev == TREE_SENTINEL {
            self.used_head_next[bank] = next;
        } else {
            self.trees[usize::from(prev)].next = next;
        }
    }

    fn push_tree_front(&mut self, tree: usize, bank: usize) {
        let first = self.used_head_next[bank];
        self.trees[tree].prev = TREE_SENTINEL;
        self.trees[tree].next = first;
        if first == TREE_SENTINEL {
            self.used_head_prev[bank] = tree as u16;
        } else {
            self.trees[usize::from(first)].prev = tree as u16;
        }
        self.used_head_next[bank] = tree as u16;
    }
}

fn smc_base_off(level: usize, slot: usize) -> u32 {
    const T: [u32; 80] = [
        0, 16, 32, 48, 64, 80, 96, 112, 128, 144, 160, 176, 192, 208, 224, 240, 0, 21, 42, 63, 85,
        106, 127, 148, 170, 191, 212, 233, 255, 276, 297, 318, 0, 51, 102, 153, 204, 255, 306, 357,
        409, 460, 511, 562, 613, 664, 715, 766, 0, 36, 73, 109, 146, 182, 219, 255, 292, 328, 365,
        401, 438, 474, 511, 547, 0, 28, 56, 84, 113, 141, 169, 197, 227, 255, 283, 311, 340, 368,
        396, 424,
    ];
    T[level * 16 + slot]
}
