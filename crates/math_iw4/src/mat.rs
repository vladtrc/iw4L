pub fn matrix_multiply(a: [[f32; 3]; 3], b: [[f32; 3]; 3]) -> [[f32; 3]; 3] {
    let a = flatten(a);
    let b = flatten(b);
    let mut out = [0.0; 9];
    out[0] = a[2] * b[6] + a[1] * b[3] + a[0] * b[0];
    out[1] = a[2] * b[7] + b[1] * a[0] + b[4] * a[1];
    out[2] = a[2] * b[8] + b[2] * a[0] + b[5] * a[1];
    out[3] = b[6] * a[5] + a[3] * b[0] + a[4] * b[3];
    out[4] = a[5] * b[7] + a[3] * b[1] + a[4] * b[4];
    out[5] = a[5] * b[8] + a[3] * b[2] + a[4] * b[5];
    out[6] = b[6] * a[8] + a[6] * b[0] + a[7] * b[3];
    out[7] = a[8] * b[7] + a[6] * b[1] + a[7] * b[4];
    out[8] = a[8] * b[8] + a[6] * b[2] + a[7] * b[5];
    unflatten(out)
}

pub fn matrix_transform_vector43(v: [f32; 3], axis: [[f32; 3]; 3], origin: [f32; 3]) -> [f32; 3] {
    let m = flatten(axis);
    [
        m[6] * v[2] + v[0] * m[0] + m[3] * v[1] + origin[0],
        m[7] * v[2] + m[4] * v[1] + m[1] * v[0] + origin[1],
        m[8] * v[2] + m[5] * v[1] + m[2] * v[0] + origin[2],
    ]
}

pub fn matrix_multiply43(
    a_axis: [[f32; 3]; 3],
    a_origin: [f32; 3],
    b_axis: [[f32; 3]; 3],
    b_origin: [f32; 3],
) -> ([[f32; 3]; 3], [f32; 3]) {
    let out_axis = matrix_multiply(a_axis, b_axis);
    let b = flatten(b_axis);
    let origin = [
        b[6] * a_origin[2] + a_origin[0] * b[0] + a_origin[1] * b[3] + b_origin[0],
        a_origin[2] * b[7] + a_origin[0] * b[1] + a_origin[1] * b[4] + b_origin[1],
        a_origin[2] * b[8] + a_origin[0] * b[2] + a_origin[1] * b[5] + b_origin[2],
    ];
    (out_axis, origin)
}

fn flatten(axis: [[f32; 3]; 3]) -> [f32; 9] {
    [
        axis[0][0], axis[0][1], axis[0][2], axis[1][0], axis[1][1], axis[1][2], axis[2][0],
        axis[2][1], axis[2][2],
    ]
}

fn unflatten(m: [f32; 9]) -> [[f32; 3]; 3] {
    [[m[0], m[1], m[2]], [m[3], m[4], m[5]], [m[6], m[7], m[8]]]
}
