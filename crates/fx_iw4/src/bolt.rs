pub const FX_QUAT_TRANSFORM_SCALE: f32 = 2.0;

pub const FX_BOLT_PARENT_IDENTITY_QUAT: [f32; 4] = [0.0, 0.0, 0.0, 1.0];

pub const FX_BOLT_RECORD_STRIDE: usize = 0x20;

pub const FX_BOLT_RECORD_OFF_PACKED: usize = 0;

pub const FX_BOLT_RECORD_OFF_PARENT: usize = 4;

pub const FX_BOLT_PARENT_DWORDS: usize = 7;

pub const FX_BOLT_INIT_LAST: i32 = 0xfe;

pub const FX_BOLT_RECORD_CAPACITY: u32 = 0xff;

pub const FX_BOLT_FREE_NONE: i32 = -1;

pub const FX_BOLT_DOBJ_MASK: u32 = 0xfff;

pub const FX_BOLT_HANDLE_NONE: u32 = 0xfff;

pub const FX_BOLT_TELEPORT_SHIFT: u32 = 12;

pub const FX_BOLT_BONE_SHIFT: u32 = 13;
pub const FX_BOLT_BONE_MASK: u32 = 0x7ff;

pub const FX_BOLT_LOST_OR: u32 = 0xff_efff;

pub const FX_BOLT_CENTITY_LIMIT: u32 = 0x7fe;

pub const FX_BOLT_VIEWMODEL_DOBJ_BASE: u32 = 0x800;

pub const FX_BOLT_CENTITY_STRIDE: usize = 0x204;

pub const FX_BOLT_CENTITY_TELEPORT_MASK: u32 = 2;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FxGetBoneOrientationRoute {
    EntityPose,

    DObjBone(u16),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FxGetBoneOrientationRefuse {
    CentityInvalid,
    DObjMissing,
    BoneOutOfRange,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FxUpdateEffectBolt {
    Skip,

    Refresh,

    Lost,
}

#[inline]
pub const fn fx_bolt_dobj(packed: u32) -> u32 {
    packed & FX_BOLT_DOBJ_MASK
}

#[inline]
pub const fn fx_bolt_teleport_bit(packed: u32) -> bool {
    ((packed >> FX_BOLT_TELEPORT_SHIFT) & 1) != 0
}

#[inline]
pub const fn fx_bolt_bone(packed: u32) -> u32 {
    (packed >> FX_BOLT_BONE_SHIFT) & FX_BOLT_BONE_MASK
}

#[inline]
pub const fn fx_bolt_handle_is_none(packed: u32) -> bool {
    fx_bolt_dobj(packed) == FX_BOLT_HANDLE_NONE
}

#[inline]
pub const fn fx_bolt_mark_lost(packed: u32) -> u32 {
    packed | FX_BOLT_LOST_OR
}

#[inline]
pub const fn fx_bolt_pack(dobj: u32, teleport: bool, bone: u32) -> u32 {
    (dobj & FX_BOLT_DOBJ_MASK)
        | ((teleport as u32) << FX_BOLT_TELEPORT_SHIFT)
        | ((bone & FX_BOLT_BONE_MASK) << FX_BOLT_BONE_SHIFT)
}

#[inline]
pub const fn fx_bolt_centity_teleport_for_compare(dobj: u32, teleport: bool) -> bool {
    if dobj < FX_BOLT_CENTITY_LIMIT {
        teleport
    } else {
        false
    }
}

#[inline]
pub const fn fx_bolt_spawn_teleport_bit(dobj: u32, next_state_eflags: u32) -> bool {
    fx_bolt_centity_teleport_for_compare(
        dobj,
        (next_state_eflags & FX_BOLT_CENTITY_TELEPORT_MASK) != 0,
    )
}

#[inline]
pub fn fx_get_bone_orientation_route(
    dobj: u32,
    current_valid: bool,
    bone: i32,
    dobj_bone_count: Option<u8>,
) -> Result<FxGetBoneOrientationRoute, FxGetBoneOrientationRefuse> {
    if dobj < FX_BOLT_CENTITY_LIMIT && !current_valid {
        return Err(FxGetBoneOrientationRefuse::CentityInvalid);
    }
    if bone < 0 {
        return Ok(FxGetBoneOrientationRoute::EntityPose);
    }
    let Some(count) = dobj_bone_count else {
        return Err(FxGetBoneOrientationRefuse::DObjMissing);
    };
    if bone >= i32::from(count) {
        return Err(FxGetBoneOrientationRefuse::BoneOutOfRange);
    }
    Ok(FxGetBoneOrientationRoute::DObjBone(bone as u16))
}

#[inline]
pub const fn fx_stop_effect_non_recursive_allows(status: u32) -> bool {
    (status & crate::status::FX_STATUS_REF_COUNT_MASK_IW4) != 0
}

#[inline]
pub const fn fx_stop_effect_has_owned(status: u32) -> bool {
    (status & crate::status::FX_STATUS_OWNED_EFFECTS_MASK) != 0
}

#[inline]
pub fn fx_begin_iterating_over_effects_exclusive(iterator_count: i32) -> i32 {
    let clamped = if iterator_count < 0 {
        0
    } else {
        iterator_count as u32
    };
    (clamped + 1) as i32
}

#[inline]
pub fn fx_end_iterating_over_effects(iterator_count: i32) -> i32 {
    iterator_count.wrapping_sub(1)
}

#[inline]
pub fn fx_end_iterating_runs_gc(iterator_after_dec: i32, needs_gc: bool) -> bool {
    iterator_after_dec == 0 && needs_gc
}

#[inline]
pub fn fx_quat_mul(a: [f32; 4], b: [f32; 4]) -> [f32; 4] {
    [
        (a[2] * b[1] + b[0] * a[3] + a[0] * b[3]) - a[1] * b[2],
        b[2] * a[0] + b[1] * a[3] + (a[1] * b[3] - a[2] * b[0]),
        b[2] * a[3] + ((a[1] * b[0] + a[2] * b[3]) - a[0] * b[1]),
        ((a[3] * b[3] - a[0] * b[0]) - a[1] * b[1]) - b[2] * a[2],
    ]
}

#[inline]
pub fn fx_quat_transform_vec(q: [f32; 4], v: [f32; 3]) -> [f32; 3] {
    let qx = q[0];
    let qy = q[1];
    let qz = q[2];
    let qw = q[3];
    let two = FX_QUAT_TRANSFORM_SCALE;
    [
        v[0] + ((qw * qy + qx * qz) * v[2]
            + v[1] * (qy * qx - qw * qz)
            + v[0] * (-qy * qy + -qz * qz))
            * two,
        ((qy * qz - qw * qx) * v[2] + v[0] * (qw * qz + qy * qx) + v[1] * (-qx * qx + -qz * qz))
            * two
            + v[1],
        two * (v[0] * (qx * qz - qw * qy)
            + v[1] * (qw * qx + qy * qz)
            + v[2] * (-qy * qy + -qx * qx))
            + v[2],
    ]
}

#[inline]
pub const fn fx_bolt_init_parent_orientation() -> ([f32; 4], [f32; 3]) {
    (FX_BOLT_PARENT_IDENTITY_QUAT, [0.0; 3])
}

#[inline]
pub const fn fx_bolt_record_index_from_byte_delta(delta: i32) -> i32 {
    (delta + ((delta >> 31) & 0x1f)) >> 5
}

#[inline]
pub const fn fx_bolt_init_next_index(i: i32) -> i32 {
    if i < FX_BOLT_INIT_LAST {
        i + 1
    } else {
        FX_BOLT_FREE_NONE
    }
}

#[inline]
pub const fn fx_bolt_alloc(first_free: i32, next_at_taken: i32) -> Option<(i32, i32)> {
    if first_free == FX_BOLT_FREE_NONE {
        None
    } else {
        Some((first_free, next_at_taken))
    }
}

pub fn fx_bolt_compose_orientation(
    parent_quat: [f32; 4],
    parent_origin: [f32; 3],
    bone_quat: [f32; 4],
    bone_origin: [f32; 3],
) -> ([f32; 4], [f32; 3]) {
    let quat = fx_quat_mul(parent_quat, bone_quat);
    let rotated = fx_quat_transform_vec(parent_quat, bone_origin);
    (
        quat,
        [
            parent_origin[0] + rotated[0],
            parent_origin[1] + rotated[1],
            parent_origin[2] + rotated[2],
        ],
    )
}

#[inline]
pub fn fx_update_effect_bolt(
    effect_bolt: u8,
    packed: u32,
    centity_teleport: bool,
    bone_ok: bool,
) -> FxUpdateEffectBolt {
    if effect_bolt == 0xff {
        return FxUpdateEffectBolt::Skip;
    }
    if fx_bolt_handle_is_none(packed) {
        return FxUpdateEffectBolt::Skip;
    }
    let dobj = fx_bolt_dobj(packed);
    let teleport = fx_bolt_centity_teleport_for_compare(dobj, centity_teleport);
    if teleport == fx_bolt_teleport_bit(packed) && bone_ok {
        FxUpdateEffectBolt::Refresh
    } else {
        FxUpdateEffectBolt::Lost
    }
}
