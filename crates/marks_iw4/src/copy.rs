use crate::{FX_POINT_GROUP_STRIDE, FX_TRI_GROUP_STRIDE, FxMarkStagingPoint, FxMarkStagingTri};

const _: () = assert!(core::mem::size_of::<FxTriGroup>() == FX_TRI_GROUP_STRIDE);
const _: () = assert!(core::mem::size_of::<FxPointGroup>() == FX_POINT_GROUP_STRIDE);
const _: () = assert!(FX_MARK_CONTEXT_SIZE == 7);
const _: () = assert!(FX_MARK_CONTEXT_SIZE != 6);

pub const FX_MARK_CONTEXT_SIZE: usize = 7;

pub const FX_TRI_GROUP_CHAIN_NONE: u16 = 0xffff;
pub const FX_POINT_GROUP_CHAIN_NONE: u16 = 0xffff;

#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FxTriGroup {
    pub indices: [[u16; 3]; 2],
    pub context: [u8; 7],
    pub tri_count: u8,
    pub next: u16,
    pub _pad: u16,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FxPointGroup {
    pub points: [FxMarkStagingPoint; 2],
    pub next: u16,
    pub _pad: [u8; 2],
}

impl FxTriGroup {
    pub const ZERO: Self = Self {
        indices: [[0; 3]; 2],
        context: [0; 7],
        tri_count: 0,
        next: FX_TRI_GROUP_CHAIN_NONE,
        _pad: 0,
    };
}

impl FxPointGroup {
    pub const ZERO: Self = Self {
        points: [FxMarkStagingPoint::ZERO; 2],
        next: FX_POINT_GROUP_CHAIN_NONE,
        _pad: [0; 2],
    };
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FxMarkCopyCensus {
    pub copied_tri: u32,
    pub copied_point: u32,
    pub tri_groups: u32,
    pub point_groups: u32,

    pub first_xyz: [f32; 3],
}

#[inline]
pub fn fx_mark_contexts_equal(a: &[u8; 7], b: &[u8; 7]) -> bool {
    a == b
}

#[inline]
pub fn fx_mark_tri_pack_count(tris: &[FxMarkStagingTri]) -> u32 {
    if tris.len() >= 2 && fx_mark_contexts_equal(&tris[0].context, &tris[1].context) {
        2
    } else if tris.is_empty() {
        0
    } else {
        1
    }
}

pub fn fx_mark_tri_groups_for_staging(tris: &[FxMarkStagingTri]) -> u32 {
    let mut n = 0u32;
    let mut i = 0usize;
    while i < tris.len() {
        let pack = fx_mark_tri_pack_count(&tris[i..]);
        if pack == 0 {
            break;
        }
        n = n.saturating_add(1);
        i += pack as usize;
    }
    n
}

pub fn fx_link_scratch_tri_groups(groups: &mut [FxTriGroup]) -> u16 {
    if groups.is_empty() {
        return FX_TRI_GROUP_CHAIN_NONE;
    }
    groups[0].next = FX_TRI_GROUP_CHAIN_NONE;
    let mut i = 1usize;
    while i < groups.len() {
        groups[i].next = (i - 1) as u16;
        i += 1;
    }
    (groups.len() - 1) as u16
}

pub fn fx_link_scratch_point_groups(groups: &mut [FxPointGroup]) -> u16 {
    if groups.is_empty() {
        return FX_POINT_GROUP_CHAIN_NONE;
    }
    groups[0].next = FX_POINT_GROUP_CHAIN_NONE;
    let mut i = 1usize;
    while i < groups.len() {
        groups[i].next = (i - 1) as u16;
        i += 1;
    }
    (groups.len() - 1) as u16
}

pub fn fx_copy_mark_tris(
    groups: &mut [FxTriGroup],
    mut handle: u16,
    mut staging: &[FxMarkStagingTri],
) -> u32 {
    let mut copied = 0u32;
    while !staging.is_empty() {
        if handle == FX_TRI_GROUP_CHAIN_NONE {
            break;
        }
        let idx = handle as usize;
        if idx >= groups.len() {
            break;
        }
        let cap = if staging.len() < 2 { 1 } else { 2 };
        let first_ctx = staging[0].context;
        groups[idx].context = first_ctx;
        let mut packed = 0u8;
        while (packed as usize) < cap {
            let si = packed as usize;
            if si >= staging.len() {
                break;
            }
            if si > 0 && !fx_mark_contexts_equal(&staging[si].context, &first_ctx) {
                break;
            }
            groups[idx].indices[si] = staging[si].indices;
            packed = packed.saturating_add(1);
        }
        groups[idx].tri_count = packed;
        copied = copied.saturating_add(u32::from(packed));
        staging = &staging[packed as usize..];
        handle = groups[idx].next;
    }
    copied
}

pub fn fx_copy_mark_points(
    groups: &mut [FxPointGroup],
    mut handle: u16,
    mut staging: &[FxMarkStagingPoint],
) -> u32 {
    let mut copied = 0u32;
    while !staging.is_empty() {
        if handle == FX_POINT_GROUP_CHAIN_NONE {
            break;
        }
        let idx = handle as usize;
        if idx >= groups.len() {
            break;
        }
        let n = core::cmp::min(staging.len(), 2);
        let mut i = 0usize;
        while i < n {
            groups[idx].points[i] = staging[i];
            i += 1;
        }
        copied = copied.saturating_add(n as u32);
        staging = &staging[n..];
        handle = groups[idx].next;
    }
    copied
}

pub fn fx_copy_staging_into_scratch(
    tris: &[FxMarkStagingTri],
    points: &[FxMarkStagingPoint],
    tri_groups: &mut [FxTriGroup],
    point_groups: &mut [FxPointGroup],
) -> Option<FxMarkCopyCensus> {
    if tris.is_empty() {
        return None;
    }
    let need_t = fx_mark_tri_groups_for_staging(tris) as usize;
    let need_p = crate::fx_mark_point_groups_for_count(points.len() as u32) as usize;
    if tri_groups.len() < need_t || point_groups.len() < need_p {
        return None;
    }
    let tri_head = fx_link_scratch_tri_groups(&mut tri_groups[..need_t]);
    let point_head = fx_link_scratch_point_groups(&mut point_groups[..need_p]);
    let copied_tri = fx_copy_mark_tris(&mut tri_groups[..need_t], tri_head, tris);
    let copied_point = fx_copy_mark_points(&mut point_groups[..need_p], point_head, points);
    let first_xyz = if (tri_head as usize) < need_t && (point_head as usize) < need_p {
        point_groups[point_head as usize].points[0].xyz
    } else {
        [0.0; 3]
    };
    Some(FxMarkCopyCensus {
        copied_tri,
        copied_point,
        tri_groups: need_t as u32,
        point_groups: need_p as u32,
        first_xyz,
    })
}
