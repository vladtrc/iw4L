use crate::scene_ent_bounds::{DObjAnimMat, SCENE_DOBJ_GATE_BOUNDED, SCENE_DOBJ_GATE_SKINNING};

pub const SCENE_ENT_SKIN_ENTRY_BYTES: u32 = 0x1c;

pub const SCENE_ENT_SKIN_PLACEMENT_BYTES: u32 = 0x20;

pub const SKIN_RIGID_HEADER_SCALE_BITS: u32 = 0x3f80_0000;

pub const SKIN_RIGID_VERT_LIST_STRIDE: u32 = 0xc;

pub const SKIN_RIGID_VERT_LIST_BONE_SHIFT: u32 = 6;

pub const SKIN_DOBJ_ANIM_MAT_STRIDE: u32 = 0x20;

pub const SCENE_ENT_SKIN_HIDDEN_BYTES: u32 = 4;

pub const SCENE_ENT_SKIN_HIDDEN_TAG: i32 = -3;

pub const SCENE_ENT_SKIN_LOCAL_BYTES: u32 = 0x1ee4;

pub const SCENE_ENT_SKIN_FRAME_BYTES: u32 = 0x2_0000;

pub const SCENE_ENT_CULL_LOD_COUNT: usize = 32;

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct PreSkinSurface {
    pub part_bits: [u32; 6],

    pub vert_count: u16,

    pub deformed: bool,

    pub vert_list_count: u32,
}

#[derive(Clone, Copy, Debug)]
pub struct PreSkinModel<'a> {
    pub lod: i8,

    pub bone_count: u8,

    pub surfaces: &'a [PreSkinSurface],
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SceneEntSkinEntry {
    pub model: u16,

    pub surface: u16,

    pub tag: i32,

    pub bone_base: u8,

    pub bone_count: u8,
    pub bytes: u32,
}

impl SceneEntSkinEntry {
    #[must_use]
    pub fn hidden(self) -> bool {
        self.tag == SCENE_ENT_SKIN_HIDDEN_TAG
    }

    #[must_use]
    pub fn stream_draws(entries: &[Self], model: u16, surface: u16) -> bool {
        match entries
            .iter()
            .copied()
            .find(|entry| entry.model == model && entry.surface == surface)
        {
            Some(entry) => !entry.hidden(),
            None => entries.is_empty(),
        }
    }

    #[must_use]
    pub fn skinned_base(self) -> Option<u32> {
        u32::try_from(self.tag).ok()
    }

    #[must_use]
    pub fn rigid_placements(self) -> Option<u32> {
        if self.tag < SCENE_ENT_SKIN_HIDDEN_TAG {
            u32::try_from(SCENE_ENT_SKIN_HIDDEN_TAG - self.tag).ok()
        } else {
            None
        }
    }

    #[must_use]
    pub fn cmd_bytes(self) -> u32 {
        scene_ent_skin_cmd_bytes(self.tag)
    }

    #[must_use]
    pub fn fills_skinned_cache(self) -> bool {
        scene_ent_skin_cmd_fills_cache(self.tag)
    }
}

#[must_use]
pub fn scene_ent_skin_cmd_bytes(tag: i32) -> u32 {
    if tag == SCENE_ENT_SKIN_HIDDEN_TAG {
        SCENE_ENT_SKIN_HIDDEN_BYTES
    } else if tag < SCENE_ENT_SKIN_HIDDEN_TAG {
        let lists = u32::try_from(SCENE_ENT_SKIN_HIDDEN_TAG.saturating_sub(tag)).unwrap_or(0);
        SCENE_ENT_SKIN_ENTRY_BYTES
            .saturating_add(lists.saturating_mul(SCENE_ENT_SKIN_PLACEMENT_BYTES))
    } else {
        SCENE_ENT_SKIN_ENTRY_BYTES
    }
}

#[must_use]
pub fn scene_ent_skin_cmd_fills_cache(tag: i32) -> bool {
    tag >= 0
}

#[must_use]
pub fn skin_skinned_cache_dest(rover: u32, tag: i32) -> Option<u32> {
    scene_ent_skin_cmd_fills_cache(tag).then(|| rover.wrapping_add(tag as u32))
}

#[must_use]
pub fn skin_rigid_vert_list_bone_mat_off(bone_u16: u16) -> u32 {
    (u32::from(bone_u16) >> SKIN_RIGID_VERT_LIST_BONE_SHIFT) * SKIN_DOBJ_ANIM_MAT_STRIDE
}

#[must_use]
pub fn skin_rigid_vert_list_packed_bytes(run: u16) -> u32 {
    u32::from(run).saturating_mul(SKIN_VERT_INFO_PACKED_STRIDE)
}

#[must_use]
pub fn skin_quat_normalize(q: [f32; 4]) -> [f32; 4] {
    let len_sq = q[0] * q[0] + q[1] * q[1] + q[2] * q[2] + q[3] * q[3];
    if len_sq == 0.0 {
        return q;
    }
    let inv = 1.0 / libm::sqrtf(len_sq);
    [q[0] * inv, q[1] * inv, q[2] * inv, q[3] * inv]
}

#[must_use]
pub fn skin_rigid_scaled_placement(
    bind: DObjAnimMat,
    posed: DObjAnimMat,
    view_org: [f32; 3],
) -> ([f32; 4], [f32; 3]) {
    let one = SKIN_BLEND_WEIGHT_ONE as f32;
    let bq = bind.quat;
    let pq = posed.quat;
    let quat = skin_quat_normalize([
        bq[1] * pq[2] + ((pq[0] * bq[3] - pq[3] * bq[0]) - bq[2] * pq[1]),
        (bq[3] * pq[1] + (pq[0] * bq[2] - pq[3] * bq[1])) - bq[0] * pq[2],
        pq[2] * bq[3] + (bq[0] * pq[1] - (pq[3] * bq[2] + pq[0] * bq[1])),
        pq[2] * bq[2] + bq[1] * pq[1] + pq[0] * bq[0] + pq[3] * bq[3],
    ]);

    let mut tw = bind.trans_weight;
    let b4 = tw * bq[0];
    let b0 = bq[1] * tw;
    tw *= bq[2];
    let m90 = one - (b0 * bq[1] + tw * bq[2]);
    let m8c = bq[1] * b4 - tw * bq[3];
    let m88 = b0 * bq[3] + bq[2] * b4;
    let m80 = tw * bq[3] + bq[1] * b4;
    let m7c = one - (b4 * bq[0] + tw * bq[2]);
    let m78 = bq[2] * b0 - b4 * bq[3];
    let m70 = bq[2] * b4 - b0 * bq[3];
    let m6c = bq[2] * b0 + b4 * bq[3];
    let m68 = one - (b0 * bq[1] + b4 * bq[0]);
    let inv_x = -(bind.trans[2] * m70 + bind.trans[1] * m80 + bind.trans[0] * m90);
    let inv_y = -(bind.trans[2] * m6c + bind.trans[1] * m7c + bind.trans[0] * m8c);
    let inv_z = -(bind.trans[2] * m68 + bind.trans[1] * m78 + bind.trans[0] * m88);

    let mut ptw = posed.trans_weight;
    let a8 = ptw * pq[0];
    let a4 = ptw * pq[1];
    ptw *= pq[2];
    let f5 = a8 * pq[0];
    let mut c2c = a8 * pq[3];
    let mut c30 = a4 * pq[3];
    let mut c40 = ptw * pq[3];
    let m50 = one - (a4 * pq[1] + ptw * pq[2]);
    let m4c = a8 * pq[1] + c40;
    let m48 = pq[2] * a8 - c30;
    c40 = a8 * pq[1] - c40;
    let m3c = one - (f5 + ptw * pq[2]);
    let m38 = c2c + pq[2] * a4;
    c30 += pq[2] * a8;
    c2c = pq[2] * a4 - c2c;
    let m28 = one - (a4 * pq[1] + f5);

    let origin = [
        inv_x * m50 + inv_y * c40 + inv_z * c30 + posed.trans[0] + view_org[0],
        c2c * inv_z + m3c * inv_y + m4c * inv_x + posed.trans[1] + view_org[1],
        m48 * inv_x + m38 * inv_y + m28 * inv_z + posed.trans[2] + view_org[2],
    ];
    (quat, origin)
}

pub const SKIN_VERT_INFO_PACKED_STRIDE: u32 = 0x20;

pub const SKIN_VERT_INFO_BLEND_STRIDE: [u32; 4] = [2, 6, 10, 14];

pub const SKIN_DUAL_DVAR_SIDECAR_STRIDE: u32 = 8;

pub const SKIN_DUAL_ESI_EDI_SRC_CURSOR: u32 = 0x10;

pub const SKIN_DUAL_ESI_SRC_CURSOR: u32 = 0x18;

pub const SKIN_DUAL_DVAR_SRC_CURSOR: u32 = 0x18;

pub const SKIN_DUAL_BLEND_WEIGHT_SCALE: f32 = f32::from_bits(0x3780_0000);

pub const SKIN_DUAL_BLEND_WEIGHT_ONE: f32 = f32::from_bits(0x3f80_0000);

pub const GFX_D3DERR_DEVICELOST: i32 = -0x7789_f798;

pub const GFX_D3DERR_DEVICENOTRESET: i32 = -0x7789_f797;

pub const SKIN_WORKER_CMD: u8 = 0x12;

#[must_use]
pub fn skin_cmd_uses_dual_dvar(dvar_a_current: u8, dvar_b_current: u8) -> bool {
    dvar_a_current != 0 && dvar_b_current != 0
}

#[must_use]
pub fn skin_cmd_walks(device_lost: bool) -> bool {
    !device_lost
}

#[must_use]
pub fn skin_cmd_fills_cache(tag: i32, device_lost: bool) -> bool {
    skin_cmd_walks(device_lost) && scene_ent_skin_cmd_fills_cache(tag)
}

#[must_use]
pub fn gfx_testcoop_keeps_device(hr: i32, already_lost: bool) -> bool {
    !already_lost && hr != GFX_D3DERR_DEVICELOST && hr != GFX_D3DERR_DEVICENOTRESET
}

#[must_use]
pub fn skin_dual_dvar_vert_info_blend_bytes(counts: [i16; 4]) -> u32 {
    skin_vert_info_blend_bytes(counts)
}

#[must_use]
pub fn skin_dual_dvar_sidecar_bytes(vert_count: u32) -> u32 {
    vert_count.saturating_mul(SKIN_DUAL_DVAR_SIDECAR_STRIDE)
}

#[must_use]
pub fn skin_dual_dvar_rigid_bone_mat_off(bone_u16: u16) -> u32 {
    u32::from(bone_u16)
}

#[must_use]
pub fn skin_dual_dvar_blend_weight(raw: u16) -> f32 {
    (raw as f32) * SKIN_DUAL_BLEND_WEIGHT_SCALE
}

#[must_use]
pub fn skin_dual_dvar_mad_one_extra(primary: [f32; 3], extra: [f32; 3], raw: u16) -> [f32; 3] {
    let w = skin_dual_dvar_blend_weight(raw);
    [
        w * (extra[0] - primary[0]) + primary[0],
        w * (extra[1] - primary[1]) + primary[1],
        w * (extra[2] - primary[2]) + primary[2],
    ]
}

#[must_use]
pub fn skin_dual_dvar_mad_extras(
    dest: [f32; 3],
    p: [f32; 3],
    extras: &[(&[f32; 16], u16)],
) -> [f32; 3] {
    let mut wsum = 0.0f32;
    let mut extra = [0.0f32; 3];
    for &(mat, raw) in extras {
        let w = skin_dual_dvar_blend_weight(raw);
        wsum += w;
        let t = skin_packed_transform_point(p, mat);
        extra[0] += w * t[0];
        extra[1] += w * t[1];
        extra[2] += w * t[2];
    }
    let rem = SKIN_DUAL_BLEND_WEIGHT_ONE - wsum;
    [
        rem * dest[0] + extra[0],
        rem * dest[1] + extra[1],
        rem * dest[2] + extra[2],
    ]
}

pub const SKIN_PACKED_SRC_CURSOR: u32 = 0x18;

pub const SKIN_PACKED_UNIT_VEC_W: u8 = 0x3f;

pub const SKIN_PACKED_UNIT_VEC_SCALE: f32 = 127.0;

pub const SKIN_PACKED_UNIT_VEC_BIAS: f32 = 127.5;

pub const SKIN_DUAL_UNIT_VEC_BIAS: f32 = f32::from_bits(0x42fe_0000);

pub const SKIN_DUAL_UNIT_VEC_SCALE: f32 = f32::from_bits(0x42fe_0000);

pub const SKIN_UNIT_VEC_W_BIAS: f32 = f32::from_bits(0xc340_0000);

pub const SKIN_DUAL_UNIT_VEC_W_SCALE: f32 = f32::from_bits(0x437f_0000);

pub const SKIN_UNIT_VEC_DECODE: f32 = 127.0 * 255.0;

pub const SKIN_BLEND_WEIGHT_SCALE: f64 = f64::from_bits(0x3EF0_0000_0000_0000);

pub const SKIN_BLEND_WEIGHT_ONE: f64 = 1.0;

pub const SKIN_WEIGHTED_BUCKET_EXTRAS: [u8; 4] = [0, 1, 2, 3];

#[must_use]
pub fn skin_packed_transform_point(p: [f32; 3], esi: &[f32; 16]) -> [f32; 3] {
    [
        p[0] * esi[0] + p[1] * esi[4] + p[2] * esi[8] + esi[12],
        p[0] * esi[1] + p[1] * esi[5] + p[2] * esi[9] + esi[13],
        p[0] * esi[2] + p[1] * esi[6] + p[2] * esi[10] + esi[14],
    ]
}

#[must_use]
pub fn skin_packed_transform_vector(v: [f32; 3], esi: &[f32; 16]) -> [f32; 3] {
    [
        v[0] * esi[0] + v[1] * esi[4] + v[2] * esi[8],
        v[0] * esi[1] + v[1] * esi[5] + v[2] * esi[9],
        v[0] * esi[2] + v[1] * esi[6] + v[2] * esi[10],
    ]
}

#[must_use]
pub fn skin_unpack_unit_vec(packed: u32) -> [f32; 3] {
    let b = packed.to_le_bytes();
    let scale = (f32::from(b[3]) - SKIN_UNIT_VEC_W_BIAS) / SKIN_UNIT_VEC_DECODE;
    [
        (f32::from(b[0]) - SKIN_DUAL_UNIT_VEC_BIAS) * scale,
        (f32::from(b[1]) - SKIN_DUAL_UNIT_VEC_BIAS) * scale,
        (f32::from(b[2]) - SKIN_DUAL_UNIT_VEC_BIAS) * scale,
    ]
}

fn dual_pack_byte(value: f32, scale: f32, bias: f32) -> u8 {
    let rounded = libm::roundf(value * scale + bias);
    rounded.clamp(0.0, 255.0) as u8
}

#[must_use]
pub fn skin_dual_dvar_pack_unit_vec(n: [f32; 3], nw: f32) -> u32 {
    u32::from_le_bytes([
        dual_pack_byte(n[0], SKIN_DUAL_UNIT_VEC_SCALE, SKIN_DUAL_UNIT_VEC_BIAS),
        dual_pack_byte(n[1], SKIN_DUAL_UNIT_VEC_SCALE, SKIN_DUAL_UNIT_VEC_BIAS),
        dual_pack_byte(n[2], SKIN_DUAL_UNIT_VEC_SCALE, SKIN_DUAL_UNIT_VEC_BIAS),
        dual_pack_byte(nw, SKIN_DUAL_UNIT_VEC_W_SCALE, SKIN_UNIT_VEC_W_BIAS),
    ])
}

#[must_use]
pub fn skin_blend_weight(raw: u16) -> f32 {
    (raw as f32) * (SKIN_BLEND_WEIGHT_SCALE as f32)
}

#[must_use]
pub fn skin_packed_mad_extras(
    dest: [f32; 3],
    p: [f32; 3],
    extras: &[(&[f32; 16], u16)],
) -> [f32; 3] {
    let mut wsum = 0.0f32;
    let mut extra = [0.0f32; 3];
    for &(mat, raw) in extras {
        let w = skin_blend_weight(raw);
        wsum += w;
        let t = skin_packed_transform_point(p, mat);
        extra[0] += w * t[0];
        extra[1] += w * t[1];
        extra[2] += w * t[2];
    }
    let rem = (SKIN_BLEND_WEIGHT_ONE as f32) - wsum;
    [
        rem * dest[0] + extra[0],
        rem * dest[1] + extra[1],
        rem * dest[2] + extra[2],
    ]
}

#[must_use]
pub fn skin_packed_weighted_point(
    p: [f32; 3],
    primary: &[f32; 16],
    extras: &[(&[f32; 16], u16)],
) -> [f32; 3] {
    skin_packed_mad_extras(skin_packed_transform_point(p, primary), p, extras)
}

#[must_use]
fn vert_info_count(count: i16) -> u32 {
    u32::try_from(count.max(0)).unwrap_or(0)
}

#[must_use]
pub fn skin_vert_info_blend_bytes(counts: [i16; 4]) -> u32 {
    counts
        .iter()
        .zip(SKIN_VERT_INFO_BLEND_STRIDE)
        .fold(0u32, |sum, (count, stride)| {
            sum.saturating_add(vert_info_count(*count).saturating_mul(stride))
        })
}

#[must_use]
pub fn skin_vert_info_packed_bytes(counts: [i16; 4]) -> u32 {
    counts
        .iter()
        .fold(0u32, |sum, count| {
            sum.saturating_add(vert_info_count(*count))
        })
        .saturating_mul(SKIN_VERT_INFO_PACKED_STRIDE)
}

#[must_use]
pub fn skin_vert_info_bucket_packed_off(counts: [i16; 4], bucket: usize) -> Option<u32> {
    if bucket > 3 {
        return None;
    }
    let verts = counts[..bucket].iter().fold(0u32, |sum, count| {
        sum.saturating_add(vert_info_count(*count))
    });
    Some(verts.saturating_mul(SKIN_VERT_INFO_PACKED_STRIDE))
}

#[must_use]
pub fn skin_vert_info_bucket_blend_off(counts: [i16; 4], bucket: usize) -> Option<u32> {
    if bucket > 3 {
        return None;
    }
    Some(
        counts[..bucket]
            .iter()
            .zip(SKIN_VERT_INFO_BLEND_STRIDE)
            .fold(0u32, |sum, (count, stride)| {
                sum.saturating_add(vert_info_count(*count).saturating_mul(stride))
            }),
    )
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct PreSkinSummary {
    pub surface_count: u32,

    pub skinned_vert_count: u32,

    pub bytes: u32,

    pub local_overflow: bool,

    pub frame_overflow: bool,
}

#[must_use]
pub fn scene_ent_surface_hidden(part_bits: &[u32; 6], hide: &[u32; 6], bone_base: u32) -> bool {
    anim_iw4::dobj_surface_hidden(part_bits, hide, bone_base)
}

pub fn pre_skin_scene_ent(
    models: &[PreSkinModel<'_>],
    hide: &[u32; 6],
    frame_bytes_used: u32,
    mut emit: impl FnMut(SceneEntSkinEntry),
) -> PreSkinSummary {
    let mut out = PreSkinSummary::default();
    let mut bone_base = 0u32;
    for (model_index, model) in models.iter().enumerate() {
        if model.lod < 0 {
            bone_base = bone_base.saturating_add(u32::from(model.bone_count));
            continue;
        }
        out.surface_count = out
            .surface_count
            .saturating_add(model.surfaces.len() as u32);
        for (surface_index, surface) in model.surfaces.iter().enumerate() {
            let hidden = scene_ent_surface_hidden(&surface.part_bits, hide, bone_base);

            let overflowed = out.bytes >= SCENE_ENT_SKIN_LOCAL_BYTES;
            if overflowed {
                out.local_overflow = true;
            }
            let (tag, bytes) = if hidden || overflowed {
                (SCENE_ENT_SKIN_HIDDEN_TAG, SCENE_ENT_SKIN_HIDDEN_BYTES)
            } else if surface.deformed {
                let base = out.skinned_vert_count;
                out.skinned_vert_count = out
                    .skinned_vert_count
                    .saturating_add(u32::from(surface.vert_count));
                (
                    i32::try_from(base).unwrap_or(i32::MAX),
                    SCENE_ENT_SKIN_ENTRY_BYTES,
                )
            } else {
                let lists = surface.vert_list_count;
                (
                    SCENE_ENT_SKIN_HIDDEN_TAG
                        .saturating_sub(i32::try_from(lists).unwrap_or(i32::MAX)),
                    SCENE_ENT_SKIN_ENTRY_BYTES
                        .saturating_add(lists.saturating_mul(SCENE_ENT_SKIN_PLACEMENT_BYTES)),
                )
            };
            out.bytes = out.bytes.saturating_add(bytes);
            emit(SceneEntSkinEntry {
                model: model_index as u16,
                surface: surface_index as u16,
                tag,
                bone_base: bone_base as u8,
                bone_count: model.bone_count,
                bytes,
            });
        }
        bone_base = bone_base.saturating_add(u32::from(model.bone_count));
    }
    if frame_bytes_used.saturating_add(out.bytes) > SCENE_ENT_SKIN_FRAME_BYTES {
        out.frame_overflow = true;
        out.surface_count = 0;
    }
    out
}

#[must_use]
pub fn scene_dobj_gate_skin_begin(gate: &mut u32) -> bool {
    if *gate >= SCENE_DOBJ_GATE_SKINNED_BASE {
        return false;
    }
    if *gate == SCENE_DOBJ_GATE_BOUNDED {
        *gate = SCENE_DOBJ_GATE_SKINNING;
        true
    } else {
        false
    }
}

pub fn scene_dobj_gate_skinned(gate: &mut u32, surface_count: u32) {
    *gate = surface_count.saturating_add(SCENE_DOBJ_GATE_SKINNED_BASE);
}

pub const SCENE_DOBJ_GATE_SKINNED_BASE: u32 = 4;

#[must_use]
pub fn scene_dobj_surface_count(gate: u32) -> Option<u32> {
    gate.checked_sub(SCENE_DOBJ_GATE_SKINNED_BASE)
}
