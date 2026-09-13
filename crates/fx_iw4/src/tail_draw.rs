use crate::vec::{fx_vec3_length_sq, fx_vec3_normalize};

#[inline]
pub fn fx_tail_anchor_origin(origin: [f32; 3], vel_dir: [f32; 3], size1: f32) -> [f32; 3] {
    let s = -size1;
    [
        origin[0] + s * vel_dir[0],
        origin[1] + s * vel_dir[1],
        origin[2] + s * vel_dir[2],
    ]
}

#[inline]
pub fn fx_tail_sprite_axes(
    vel_dir: [f32; 3],
    camera_origin: [f32; 3],
    pos_world: [f32; 3],
) -> Option<[[f32; 3]; 3]> {
    let delta = [
        camera_origin[0] - pos_world[0],
        camera_origin[1] - pos_world[1],
        camera_origin[2] - pos_world[2],
    ];

    let raw_t = [
        vel_dir[1] * delta[2] - vel_dir[2] * delta[1],
        vel_dir[2] * delta[0] - vel_dir[0] * delta[2],
        vel_dir[0] * delta[1] - vel_dir[1] * delta[0],
    ];
    if fx_vec3_length_sq(raw_t) < 1e-12 {
        return None;
    }
    let tangent = fx_vec3_normalize(raw_t);

    let normal = [
        tangent[1] * vel_dir[2] - tangent[2] * vel_dir[1],
        tangent[2] * vel_dir[0] - tangent[0] * vel_dir[2],
        tangent[0] * vel_dir[1] - tangent[1] * vel_dir[0],
    ];
    Some([tangent, vel_dir, normal])
}

#[inline]
pub const fn fx_tail_sprite_full_extent(half: f32) -> f32 {
    half * 2.0
}
