pub const FX_PERP_VECTOR_UNIT: f64 = 1.0;

#[inline]
pub fn fx_perpendicular_vector(v: [f32; 3]) -> [f32; 3] {
    let sq = [v[0] * v[0], v[1] * v[1], v[2] * v[2]];
    let mut axis = usize::from(sq[1] < sq[0]);
    if sq[2] < sq[axis] {
        axis = 2;
    }
    let neg = -v[axis];
    let mut out = [neg * v[0], neg * v[1], neg * v[2]];
    out[axis] += FX_PERP_VECTOR_UNIT as f32;
    fx_vec3_normalize(out)
}

#[inline]
pub fn fx_vector_vectors(forward: [f32; 3]) -> [[f32; 3]; 3] {
    let f = fx_vec3_normalize(forward);
    if fx_vec3_length_sq(f) < 1e-12 {
        return [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]];
    }
    let up = fx_perpendicular_vector(f);
    let right = [
        f[2] * up[1] - f[1] * up[2],
        f[0] * up[2] - up[0] * f[2],
        f[1] * up[0] - f[0] * up[1],
    ];
    [f, right, up]
}

#[inline]
pub fn fx_vec3_length_sq(v: [f32; 3]) -> f32 {
    v[0] * v[0] + v[1] * v[1] + v[2] * v[2]
}

#[inline]
pub fn fx_vec3_normalize(v: [f32; 3]) -> [f32; 3] {
    let len_sq = fx_vec3_length_sq(v);
    if len_sq <= 1e-12 {
        return [0.0; 3];
    }
    let inv = 1.0 / libm::sqrtf(len_sq);
    [v[0] * inv, v[1] * inv, v[2] * inv]
}

#[inline]
pub fn fx_vec3_distance(a: [f32; 3], b: [f32; 3]) -> f32 {
    libm::sqrtf(fx_vec3_length_sq([b[0] - a[0], b[1] - a[1], b[2] - a[2]]))
}

#[inline]
pub fn fx_effect_orient_arc(last_axis: [[f32; 3]; 3], now_axis: [[f32; 3]; 3]) -> f32 {
    let last = crate::quat::fx_axis_to_quat(last_axis);
    let now = crate::quat::fx_axis_to_quat(now_axis);
    let mut dot = last[0] * now[0] + last[1] * now[1] + last[2] * now[2] + last[3] * now[3];
    if dot > 1.0 {
        dot = 1.0;
    }
    if dot < -1.0 {
        dot = -1.0;
    }
    let a = 2.0 * libm::acosf(dot);
    let wrap = crate::origin::FX_TWO_PI as f32 - a;
    if a <= wrap { a } else { wrap }
}
