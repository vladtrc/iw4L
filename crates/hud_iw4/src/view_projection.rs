pub const R_INFINITE_PERSPECTIVE_K: f32 = 2047.0 / 2048.0;

pub const R_ZNEAR_DEFAULT: f32 = 4.0;

pub const R_ZNEAR_FLOOR: f32 = 0.01;

#[must_use]
pub fn r_znear_from_refdef(refdef_z_near: f32, r_znear: f32) -> f32 {
    if refdef_z_near > 0.0 {
        return refdef_z_near;
    }
    if r_znear < R_ZNEAR_FLOOR {
        R_ZNEAR_FLOOR
    } else {
        r_znear
    }
}

pub const R_ZNEAR_DEPTHHACK_DEFAULT: f32 = 0.1;

#[must_use]
pub fn r_depth_hack_near_clip(r_znear_depthhack: f32) -> f32 {
    -r_znear_depthhack
}

pub const R_SUBWINDOW_EDGE_EPS: f32 = 1.0e-4;

pub const R_SUBWINDOW_DEFAULT: [f32; 4] = [0.0, 1.0, 0.0, 1.0];

#[must_use]
pub fn r_subwindow_clamp(left: f32, right: f32, top: f32, bottom: f32) -> [f32; 4] {
    let eps = R_SUBWINDOW_EDGE_EPS;
    let right_out = if left + eps > right {
        left + eps
    } else {
        right
    };
    let bottom_out = if top + eps > bottom {
        top + eps
    } else {
        bottom
    };
    [left, right_out, top, bottom_out]
}

#[must_use]
pub fn r_subwindow_to_viewport(
    left: f32,
    right: f32,
    top: f32,
    bottom: f32,
    rt_w: i32,
    rt_h: i32,
) -> crate::GfxViewport {
    let [l, r, t, b] = r_subwindow_clamp(left, right, top, bottom);
    let x0 = (l * rt_w as f32) as i32;
    let x1 = (r * rt_w as f32) as i32;
    let y0 = (t * rt_h as f32) as i32;
    let y1 = (b * rt_h as f32) as i32;
    crate::GfxViewport {
        x: x0,
        y: y0,
        width: x1 - x0,
        height: y1 - y0,
    }
}

#[must_use]
pub fn r_subwindow_is_full(left: f32, right: f32, top: f32, bottom: f32) -> bool {
    let [l, r, t, b] = r_subwindow_clamp(left, right, top, bottom);
    l == 0.0 && r == 1.0 && t == 0.0 && b == 1.0
}

#[must_use]
pub fn r_setup_projection_matrix(tan_half_x: f32, tan_half_y: f32, z_near: f32) -> [f32; 16] {
    let k = R_INFINITE_PERSPECTIVE_K;
    let mut m = [0.0f32; 16];
    m[0] = k / tan_half_x;
    m[5] = k / tan_half_y;
    m[10] = k;
    m[11] = 1.0;
    m[14] = -z_near * k;
    m
}

#[must_use]
pub fn r_setup_finite_projection_matrix(
    tan_half_x: f32,
    tan_half_y: f32,
    z_near: f32,
    z_far: f32,
) -> Option<[f32; 16]> {
    if z_near == z_far {
        return None;
    }
    let den = z_near - z_far;
    let mut m = [0.0f32; 16];
    m[0] = 1.0 / tan_half_x;
    m[5] = 1.0 / tan_half_y;
    m[10] = -z_far / den;
    m[11] = 1.0;
    m[14] = (z_far * z_near) / den;
    Some(m)
}

#[must_use]
pub fn r_matrix_for_viewer(axis: [[f32; 3]; 3]) -> [f32; 16] {
    let mut m = [0.0f32; 16];
    m[0] = -axis[1][0];
    m[1] = axis[2][0];
    m[2] = axis[0][0];
    m[4] = -axis[1][1];
    m[5] = axis[2][1];
    m[6] = axis[0][1];
    m[8] = -axis[1][2];
    m[9] = axis[2][2];
    m[10] = axis[0][2];
    m[15] = 1.0;
    m
}

#[must_use]
pub fn r_matrix_multiply44(a: &[f32; 16], b: &[f32; 16]) -> [f32; 16] {
    let mut o = [0.0f32; 16];
    o[0] = b[12] * a[3] + b[8] * a[2] + a[1] * b[4] + a[0] * b[0];
    o[1] = a[3] * b[13] + b[9] * a[2] + a[0] * b[1] + b[5] * a[1];
    o[2] = a[3] * b[14] + a[2] * b[10] + b[2] * a[0] + b[6] * a[1];
    o[3] = a[3] * b[15] + b[11] * a[2] + a[0] * b[3] + b[7] * a[1];
    o[4] = b[12] * a[7] + b[8] * a[6] + a[4] * b[0] + a[5] * b[4];
    o[5] = a[7] * b[13] + b[9] * a[6] + b[5] * a[5] + a[4] * b[1];
    o[6] = b[14] * a[7] + a[6] * b[10] + a[4] * b[2] + a[5] * b[6];
    o[7] = a[7] * b[15] + b[11] * a[6] + b[7] * a[5] + a[4] * b[3];
    o[8] = b[12] * a[11] + b[8] * a[10] + a[8] * b[0] + a[9] * b[4];
    o[9] = a[11] * b[13] + b[9] * a[10] + b[5] * a[9] + a[8] * b[1];
    o[10] = b[14] * a[11] + a[10] * b[10] + a[8] * b[2] + a[9] * b[6];
    o[11] = a[11] * b[15] + b[11] * a[10] + b[7] * a[9] + a[8] * b[3];
    o[12] = b[12] * a[15] + a[14] * b[8] + a[13] * b[4] + b[0] * a[12];
    o[13] = a[15] * b[13] + a[14] * b[9] + a[12] * b[1] + b[5] * a[13];
    o[14] = b[14] * a[15] + a[14] * b[10] + b[6] * a[13] + b[2] * a[12];
    o[15] = a[15] * b[15] + a[14] * b[11] + a[12] * b[3] + b[7] * a[13];
    o
}

#[must_use]
pub fn r_compose_view_projection(
    view: &[f32; 16],
    proj: &[f32; 16],
    origin: [f32; 3],
) -> Option<([f32; 16], [f32; 16])> {
    let vp = r_matrix_multiply44(view, proj);
    let inv = r_matrix_inverse44(&vp)?;
    let mut t = IDENTITY44;
    t[12] = origin[0];
    t[13] = origin[1];
    t[14] = origin[2];
    let inv_vp = r_matrix_multiply44(&inv, &t);
    Some((vp, inv_vp))
}

#[must_use]
pub fn r_set_view_parms_matrices(
    origin: [f32; 3],
    axis: [[f32; 3]; 3],
    tan_half_x: f32,
    tan_half_y: f32,
    z_near: f32,
) -> Option<([f32; 16], [f32; 16])> {
    let view = r_matrix_for_viewer(axis);
    let proj = r_setup_projection_matrix(tan_half_x, tan_half_y, z_near);
    r_compose_view_projection(&view, &proj, origin)
}

const IDENTITY44: [f32; 16] = [
    1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0,
];

fn r_matrix_inverse44(m: &[f32; 16]) -> Option<[f32; 16]> {
    let mut a = [[0.0f32; 8]; 4];
    for r in 0..4 {
        for c in 0..4 {
            a[r][c] = m[r * 4 + c];
            a[r][c + 4] = if r == c { 1.0 } else { 0.0 };
        }
    }
    for col in 0..4 {
        let mut pivot = col;
        let mut best = libm::fabsf(a[col][col]);
        for r in (col + 1)..4 {
            let v = libm::fabsf(a[r][col]);
            if v > best {
                best = v;
                pivot = r;
            }
        }
        if best == 0.0 {
            return None;
        }
        if pivot != col {
            a.swap(col, pivot);
        }
        let inv_p = 1.0 / a[col][col];
        for c in 0..8 {
            a[col][c] *= inv_p;
        }
        for r in 0..4 {
            if r == col {
                continue;
            }
            let f = a[r][col];
            for c in 0..8 {
                a[r][c] -= f * a[col][c];
            }
        }
    }
    let mut out = [0.0f32; 16];
    for r in 0..4 {
        for c in 0..4 {
            out[r * 4 + c] = a[r][c + 4];
        }
    }
    Some(out)
}
