use bevy::math::{Mat4, Vec4};
fn bmodel_axis_from_angles(angles: [f32; 3]) -> [[f32; 3]; 3] {
    const DEG2RAD: f32 = 0.01745329238474369;
    let yaw = angles[1] * DEG2RAD;
    let pitch = angles[0] * DEG2RAD;
    let roll = angles[2] * DEG2RAD;
    let (sy, cy) = yaw.sin_cos();
    let (sp, cp) = pitch.sin_cos();
    let (sr, cr) = roll.sin_cos();
    [
        [cp * cy, cp * sy, -sp],
        [sr * sp * cy + -sy * cr, sr * sp * sy + cr * cy, sr * cp],
        [cr * sp * cy + sr * sy, cr * sp * sy + -sr * cy, cr * cp],
    ]
}

fn mat4_from_origin_axis(origin: [f32; 3], axis: [[f32; 3]; 3]) -> Mat4 {
    Mat4::from_cols(
        Vec4::new(axis[0][0], axis[0][1], axis[0][2], 0.0),
        Vec4::new(axis[1][0], axis[1][1], axis[1][2], 0.0),
        Vec4::new(axis[2][0], axis[2][1], axis[2][2], 0.0),
        Vec4::new(origin[0], origin[1], origin[2], 1.0),
    )
}

pub fn brush_model_pose(origin: [f32; 3], angles: [f32; 3]) -> Mat4 {
    mat4_from_origin_axis(origin, bmodel_axis_from_angles(angles))
}
