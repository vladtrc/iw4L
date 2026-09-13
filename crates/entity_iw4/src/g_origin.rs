use crate::entity_state::EntityState;
use crate::trajectory::{TR_INTERPOLATE, TR_STATIONARY};

pub fn g_set_origin(es: &mut EntityState, origin: [f32; 3]) {
    es.tr_base = origin;
    es.tr_type = TR_STATIONARY;
    es.tr_time = 0;
    es.tr_duration = 0;
    es.tr_delta = [0.0; 3];
}

pub fn g_set_angle(es: &mut EntityState, angles: [f32; 3]) {
    es.apos_tr_base = angles;
    es.apos_tr_type = TR_STATIONARY;
    es.apos_tr_time = 0;
    es.apos_tr_duration = 0;
    es.apos_tr_delta = [0.0; 3];
}

pub const PARENT_LINK_AXIS_IDENTITY: [[f32; 3]; 3] =
    [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]];

pub fn g_parent_link_pose(es: &mut EntityState, origin: [f32; 3], angles: [f32; 3]) {
    g_set_origin(es, origin);
    g_set_angle(es, angles);
    es.tr_type = TR_INTERPOLATE;
    es.apos_tr_type = TR_INTERPOLATE;
}

pub fn g_dobj_anim_mat_axis(quat: [f32; 4], trans_weight: f32) -> [[f32; 3]; 3] {
    let x = quat[0];
    let y = quat[1];
    let z = quat[2];
    let w = quat[3];
    let tw_x = trans_weight * x;
    let tw_y = y * trans_weight;
    let tw_zz = trans_weight * z * z;
    let tw_zw = trans_weight * z * w;
    [
        [
            1.0 - (tw_y * y + tw_zz),
            y * tw_x + tw_zw,
            z * tw_x - tw_y * w,
        ],
        [
            y * tw_x - tw_zw,
            1.0 - (tw_x * x + tw_zz),
            tw_x * w + z * tw_y,
        ],
        [
            tw_y * w + z * tw_x,
            z * tw_y - tw_x * w,
            1.0 - (tw_y * y + tw_x * x),
        ],
    ]
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DObjAnimMat {
    pub quat: [f32; 4],

    pub trans: [f32; 3],

    pub trans_weight: f32,
}

impl DObjAnimMat {
    pub const IDENTITY: Self = Self {
        quat: [0.0, 0.0, 0.0, 1.0],
        trans: [0.0; 3],
        trans_weight: 0.0,
    };
}

pub fn g_parent_link_world_from_tag(
    parent_origin: [f32; 3],
    parent_angles: [f32; 3],
    mat: DObjAnimMat,
) -> ([f32; 3], [f32; 3]) {
    let parent_axis = math_iw4::angles_to_axis(parent_angles);
    let bone_axis = g_dobj_anim_mat_axis(mat.quat, mat.trans_weight);
    let world_axis = math_iw4::matrix_multiply(bone_axis, parent_axis);
    let world_origin = math_iw4::matrix_transform_vector43(mat.trans, parent_axis, parent_origin);
    (world_origin, math_iw4::axis_to_angles(world_axis))
}

pub fn g_parent_link_apply_local(
    world_origin: [f32; 3],
    world_angles: [f32; 3],
    local_axis: [[f32; 3]; 3],
    local_origin: [f32; 3],
) -> ([f32; 3], [f32; 3]) {
    let parent_axis = math_iw4::angles_to_axis(world_angles);
    let (axis, origin) =
        math_iw4::matrix_multiply43(local_axis, local_origin, parent_axis, world_origin);
    (origin, math_iw4::axis_to_angles(axis))
}
