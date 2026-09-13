#[inline]
pub fn fx_orientation_pos_to_world(
    origin: [f32; 3],
    axis: [[f32; 3]; 3],
    local: [f32; 3],
) -> [f32; 3] {
    [
        origin[0] + axis[0][0] * local[0] + axis[1][0] * local[1] + axis[2][0] * local[2],
        origin[1] + axis[0][1] * local[0] + axis[1][1] * local[1] + axis[2][1] * local[2],
        origin[2] + axis[0][2] * local[0] + axis[1][2] * local[1] + axis[2][2] * local[2],
    ]
}

#[inline]
pub fn fx_orientation_pos_from_world(
    origin: [f32; 3],
    axis: [[f32; 3]; 3],
    world: [f32; 3],
) -> [f32; 3] {
    let dx = world[0] - origin[0];
    let dy = world[1] - origin[1];
    let dz = world[2] - origin[2];
    [
        dx * axis[0][0] + dy * axis[0][1] + dz * axis[0][2],
        dx * axis[1][0] + dy * axis[1][1] + dz * axis[1][2],
        dx * axis[2][0] + dy * axis[2][1] + dz * axis[2][2],
    ]
}
