#[inline]
pub fn fx_axis_to_quat(axis: [[f32; 3]; 3]) -> [f32; 4] {
    let m00 = axis[0][0];
    let m01 = axis[0][1];
    let m02 = axis[0][2];
    let m10 = axis[1][0];
    let m11 = axis[1][1];
    let m12 = axis[1][2];
    let m20 = axis[2][0];
    let m21 = axis[2][1];
    let m22 = axis[2][2];

    let mut test = [[0.0f32; 4]; 4];
    test[0][0] = m12 - m21;
    test[0][1] = m20 - m02;
    test[0][2] = m01 - m10;
    test[0][3] = m11 + m00 + m22 + 1.0;
    let mut len_sq = vec4_len_sq(test[0]);
    let best = if len_sq < 1.0 {
        test[1][0] = m20 + m02;
        test[1][1] = m21 + m12;
        test[1][2] = (m22 - m11 - m00) + 1.0;
        test[1][3] = test[0][2];
        len_sq = vec4_len_sq(test[1]);
        if len_sq < 1.0 {
            test[2][0] = (m00 - m11 - m22) + 1.0;
            test[2][1] = m10 + m01;
            test[2][2] = test[1][0];
            test[2][3] = test[0][0];
            len_sq = vec4_len_sq(test[2]);
            if len_sq < 1.0 {
                test[3][0] = test[2][1];
                test[3][1] = (m11 - m00 - m22) + 1.0;
                test[3][2] = test[1][1];
                test[3][3] = test[0][1];
                len_sq = vec4_len_sq(test[3]);
                3
            } else {
                2
            }
        } else {
            1
        }
    } else {
        0
    };

    if len_sq <= 0.0 {
        return [0.0, 0.0, 0.0, 1.0];
    }
    let inv = 1.0 / libm::sqrtf(len_sq);
    [
        test[best][0] * inv,
        test[best][1] * inv,
        test[best][2] * inv,
        test[best][3] * inv,
    ]
}

#[inline]
pub fn fx_quat_nlerp(a: [f32; 4], b: [f32; 4], t: f32) -> [f32; 4] {
    let dot = a[0] * b[0] + a[1] * b[1] + a[2] * b[2] + a[3] * b[3];
    let sign = if dot < 0.0 { -1.0 } else { 1.0 };
    let s = t * sign;
    let one_minus = 1.0 - t;
    fx_quat_normalize([
        one_minus * a[0] + s * b[0],
        one_minus * a[1] + s * b[1],
        one_minus * a[2] + s * b[2],
        one_minus * a[3] + s * b[3],
    ])
}

#[inline]
pub fn fx_quat_normalize(q: [f32; 4]) -> [f32; 4] {
    let len_sq = vec4_len_sq(q);
    if len_sq <= 0.0 {
        return q;
    }
    let inv = 1.0 / libm::sqrtf(len_sq);
    [q[0] * inv, q[1] * inv, q[2] * inv, q[3] * inv]
}

#[inline]
fn vec4_len_sq(v: [f32; 4]) -> f32 {
    v[0] * v[0] + v[1] * v[1] + v[2] * v[2] + v[3] * v[3]
}
