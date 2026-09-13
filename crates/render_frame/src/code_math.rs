use glam::{Mat4, Vec3};

pub fn float4_bits(v: [f32; 4]) -> [u32; 4] {
    [
        v[0].to_bits(),
        v[1].to_bits(),
        v[2].to_bits(),
        v[3].to_bits(),
    ]
}

pub fn code_transpose_matrix_rows(m: Mat4) -> Vec<[u32; 4]> {
    code_transpose_matrix_row4(m).to_vec()
}

pub fn code_transpose_matrix_row4(m: Mat4) -> [[u32; 4]; 4] {
    let cols = m.to_cols_array_2d();
    [0usize, 1, 2, 3]
        .map(|row| float4_bits([cols[0][row], cols[1][row], cols[2][row], cols[3][row]]))
}

pub fn camera_relative_view_projection(clip_from_world: Mat4, view_origin: Vec3) -> Mat4 {
    clip_from_world * Mat4::from_translation(view_origin)
}
