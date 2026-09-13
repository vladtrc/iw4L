#![no_std]
#![forbid(unsafe_code)]

mod angles;
mod lean;
mod mat;
mod vec;

pub use angles::{
    angle_normalize_360, angle_subtract, angle_vectors, angles_to_axis, axis_to_angles,
    pitch_for_yaw_on_normal, snap_angles, track, track_angle, vec_to_yaw, vect_to_angles,
    yaw_vectors_2d,
};
pub use lean::{add_lean_to_position, get_lean_fraction};
pub use mat::{matrix_multiply, matrix_multiply43, matrix_transform_vector43};
pub use vec::vec3_length;
