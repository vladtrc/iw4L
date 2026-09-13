use crate::Bounds;

pub const SCENE_ENT_EMPTY_HALF: f32 = -131072.0;

pub const SCENE_ENT_SKINNED_HALF_PAD: f32 = 20000.0;

pub const SCENE_DOBJ_GATE_IDLE: u32 = 0;

pub const SCENE_DOBJ_GATE_UPDATING: u32 = 1;

pub const SCENE_DOBJ_GATE_BOUNDED: u32 = 2;

pub const SCENE_DOBJ_GATE_SKINNING: u32 = 3;

pub const SCENE_DOBJ_GATE_FAILED: u32 = 4;

#[derive(Clone, Copy, Debug)]
pub struct DObjAnimMat {
    pub quat: [f32; 4],
    pub trans: [f32; 3],
    pub trans_weight: f32,
}

#[derive(Clone, Copy, Debug)]
pub struct XBoneInfoBounds {
    pub mid: [f32; 3],
    pub half: [f32; 3],
}

pub const GFX_CFG_ENT_COUNT: u32 = 0x800;

#[must_use]
pub fn scene_index_alloc_bytes(gfx_cfg_ent_count: u32) -> usize {
    (gfx_cfg_ent_count as usize).saturating_mul(2)
}

#[must_use]
pub fn scene_dobj_gate_begin(gate: &mut u32) -> bool {
    if *gate == SCENE_DOBJ_GATE_IDLE {
        *gate = SCENE_DOBJ_GATE_UPDATING;
        true
    } else {
        false
    }
}

#[must_use]
pub fn scene_ent_frustum_hides(bounds: Bounds, planes: &[[f32; 4]]) -> bool {
    let (mid, half) = (bounds.mid(), bounds.half());
    planes.iter().any(|&plane| {
        let dist = plane[0] * mid[0] + plane[1] * mid[1] + plane[2] * mid[2] + plane[3];
        let radius = half[0] * absf(plane[0]) + half[1] * absf(plane[1]) + half[2] * absf(plane[2]);
        dist + radius <= 0.0
    })
}

#[must_use]
pub fn cell_frustum_cmd_plane_begin(frustum_plane_count: usize, plane_count: usize) -> u8 {
    u8::try_from(frustum_plane_count.min(plane_count)).unwrap_or(u8::MAX)
}

#[must_use]
pub fn scene_ent_inner_planes(
    planes: &[[f32; 4]],
    plane_begin: u8,
    plane_count: u8,
) -> &[[f32; 4]] {
    let begin = usize::from(plane_begin);
    let end = usize::from(plane_count).min(planes.len());
    planes.get(begin..end).unwrap_or(&[])
}

#[must_use]
pub fn scene_ent_sphere_hides(origin: [f32; 3], radius: f32, planes: &[[f32; 4]]) -> bool {
    planes.iter().any(|&plane| {
        let dist = plane[0] * origin[0] + plane[1] * origin[1] + plane[2] * origin[2] + plane[3];
        dist + radius <= 0.0
    })
}

#[must_use]
pub fn scene_dobj_initial_bounds(origin: [f32; 3], radius: f32) -> Bounds {
    Bounds::from_mid_half(origin, [radius, radius, radius])
}

#[must_use]
pub fn scene_ent_needs_bound_worker(gate: u32) -> bool {
    gate < SCENE_DOBJ_GATE_BOUNDED
}

pub fn mark_scene_ent_visible(table: &mut [u8], entnum: u32) {
    if let Some(slot) = table.get_mut(entnum as usize) {
        *slot = 1;
    }
}

#[must_use]
pub fn scene_ent_is_visible(table: &[u8], entnum: u32) -> bool {
    table.get(entnum as usize).copied().unwrap_or(0) != 0
}

#[must_use]
pub fn union_scene_ent_bone_aabb(
    mats: &[DObjAnimMat],
    bones: &[XBoneInfoBounds],
    hide_words: &[u32],
    view_org: [f32; 3],
    skinned_half_pad: bool,
) -> Bounds {
    let mut mid = [0.0_f32; 3];
    let mut half = [SCENE_ENT_EMPTY_HALF; 3];
    let n = mats.len().min(bones.len());
    let mut mask = 0x8000_0000u32;
    for i in 0..n {
        let word = hide_words.get(i >> 5).copied().unwrap_or(0);
        if word & mask != 0 {
            expand_bone(&mut mid, &mut half, mats[i], bones[i], view_org);
        }
        mask = mask >> 1 | (mask & 1) << 31;
    }
    if skinned_half_pad {
        half[0] += SCENE_ENT_SKINNED_HALF_PAD;
        half[1] += SCENE_ENT_SKINNED_HALF_PAD;
        half[2] += SCENE_ENT_SKINNED_HALF_PAD;
    }
    Bounds::from_mid_half(mid, half)
}

pub fn or_shift_part_bits(dst: &mut [u32; 6], src: [u32; 6], bone_base: u32) {
    anim_iw4::or_shift_part_bits(dst, src, bone_base);
}

pub fn set_scene_ent_part_bit(words: &mut [u32; 6], bone: usize) {
    anim_iw4::set_hide_part_bit(words, bone);
}

#[must_use]
pub fn part_bits_empty(words: &[u32; 6]) -> bool {
    words.iter().all(|w| *w == 0)
}

fn expand_bone(
    mid: &mut [f32; 3],
    half: &mut [f32; 3],
    mat: DObjAnimMat,
    bone: XBoneInfoBounds,
    view_org: [f32; 3],
) {
    let qx = mat.quat[0];
    let qy = mat.quat[1];
    let qz = mat.quat[2];
    let qw = mat.quat[3];
    let s = mat.trans_weight;
    let f2 = s * qx;
    let f5 = qy * s;
    let f6 = s * qz * qz;
    let f7 = s * qz * qw;
    let m00 = 1.0 - (f5 * qy + f6);
    let f8 = qy * f2 + f7;
    let f9 = qz * f2 - f5 * qw;
    let f7b = qy * f2 - f7;
    let m11 = 1.0 - (f2 * qx + f6);
    let f11 = qz * f5 + f2 * qw;
    let f6b = f5 * qw + qz * f2;
    let f10 = qz * f5 - f2 * qw;
    let m22 = 1.0 - (f5 * qy + f2 * qx);
    let tx = mat.trans[0] + view_org[0];
    let ty = mat.trans[1] + view_org[1];
    let tz = mat.trans[2] + view_org[2];
    let hx = bone.half[0];
    let hy = bone.half[1];
    let hz = bone.half[2];
    let cx = bone.mid[2] * f6b + bone.mid[1] * f7b + bone.mid[0] * m00 + tx;
    let rx = absf(f6b) * hz + absf(m00) * hx + absf(f7b) * hy;
    absorb_axis(&mut mid[0], &mut half[0], cx, rx);
    let cy = bone.mid[2] * f10 + bone.mid[1] * m11 + bone.mid[0] * f8 + ty;
    let ry = absf(f10) * hz + absf(m11) * hy + absf(f8) * hx;
    absorb_axis(&mut mid[1], &mut half[1], cy, ry);
    let cz = bone.mid[2] * m22 + bone.mid[1] * f11 + bone.mid[0] * f9 + tz;
    let rz = absf(m22) * hz + absf(f11) * hy + absf(f9) * hx;
    absorb_axis(&mut mid[2], &mut half[2], cz, rz);
}

fn absorb_axis(mid: &mut f32, half: &mut f32, center: f32, radius: f32) {
    let mut lo = center - radius;
    let old_lo = *mid - *half;
    if old_lo < lo {
        lo = old_lo;
    }
    let mut hi = center + radius;
    let old_hi = *half + *mid;
    if hi < old_hi {
        hi = old_hi;
    }
    *mid = (lo + hi) * 0.5;
    *half = *mid - lo;
}

fn absf(v: f32) -> f32 {
    libm::fabsf(v)
}
