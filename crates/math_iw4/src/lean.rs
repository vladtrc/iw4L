use crate::angle_vectors;

const LEAN_FRACTION_TWO: f32 = 2.0;

#[inline]
pub fn get_lean_fraction(x: f32) -> f32 {
    (LEAN_FRACTION_TWO - x.abs()) * x
}

pub fn add_lean_to_position(
    position: [f32; 3],
    view_yaw: f32,
    lean_frac: f32,
    view_roll: f32,
    lean_dist: f32,
) -> [f32; 3] {
    if lean_frac == 0.0 {
        return position;
    }
    let lean = get_lean_fraction(lean_frac);
    let (_forward, right, _up) = angle_vectors([0.0, view_yaw, view_roll * lean]);
    let scale = lean * lean_dist;
    [
        position[0] + scale * right[0],
        position[1] + scale * right[1],
        position[2] + scale * right[2],
    ]
}
