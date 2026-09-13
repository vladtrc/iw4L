pub type Quat = [f32; 4];

pub type Vec3 = [f32; 3];

pub const QUAT_IDENTITY: Quat = [0.0, 0.0, 0.0, 1.0];

pub const VEC3_ZERO: Vec3 = [0.0, 0.0, 0.0];

pub fn normalize(q: Quat) -> Quat {
    let len = libm::sqrtf(q[0] * q[0] + q[1] * q[1] + q[2] * q[2] + q[3] * q[3]);
    if len > 1e-6 {
        [q[0] / len, q[1] / len, q[2] / len, q[3] / len]
    } else {
        QUAT_IDENTITY
    }
}

pub fn quat_dot(a: Quat, b: Quat) -> f32 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2] + a[3] * b[3]
}

pub fn quat_neg(q: Quat) -> Quat {
    [-q[0], -q[1], -q[2], -q[3]]
}

pub fn quat_add_weighted(acc: Quat, q: Quat, weight: f32) -> Quat {
    [
        acc[0] + q[0] * weight,
        acc[1] + q[1] * weight,
        acc[2] + q[2] * weight,
        acc[3] + q[3] * weight,
    ]
}

pub fn slerp(a: Quat, b: Quat, t: f32) -> Quat {
    let mut b = b;
    let mut dot = quat_dot(a, b);
    if dot < 0.0 {
        b = quat_neg(b);
        dot = -dot;
    }
    if dot > 0.9995 {
        return normalize([
            a[0] + t * (b[0] - a[0]),
            a[1] + t * (b[1] - a[1]),
            a[2] + t * (b[2] - a[2]),
            a[3] + t * (b[3] - a[3]),
        ]);
    }
    let theta_0 = libm::acosf(dot.clamp(-1.0, 1.0));
    let theta = theta_0 * t;
    let sin_theta = libm::sinf(theta);
    let sin_theta_0 = libm::sinf(theta_0);
    let s0 = libm::cosf(theta) - dot * sin_theta / sin_theta_0;
    let s1 = sin_theta / sin_theta_0;
    [
        s0 * a[0] + s1 * b[0],
        s0 * a[1] + s1 * b[1],
        s0 * a[2] + s1 * b[2],
        s0 * a[3] + s1 * b[3],
    ]
}

pub fn lerp_vec3(a: Vec3, b: Vec3, t: f32) -> Vec3 {
    [
        a[0] + t * (b[0] - a[0]),
        a[1] + t * (b[1] - a[1]),
        a[2] + t * (b[2] - a[2]),
    ]
}

pub fn vec3_add_scaled(a: Vec3, b: Vec3, s: f32) -> Vec3 {
    [a[0] + b[0] * s, a[1] + b[1] * s, a[2] + b[2] * s]
}

#[must_use]
pub fn quat_mul(a: Quat, b: Quat) -> Quat {
    [
        (a[2] * b[1] + b[0] * a[3] + a[0] * b[3]) - a[1] * b[2],
        b[2] * a[0] + b[1] * a[3] + (a[1] * b[3] - a[2] * b[0]),
        b[2] * a[3] + ((a[1] * b[0] + a[2] * b[3]) - a[0] * b[1]),
        ((a[3] * b[3] - a[0] * b[0]) - a[1] * b[1]) - b[2] * a[2],
    ]
}

#[must_use]
pub fn xanim_apply_additive(
    dest_rot: Quat,
    dest_trans: Vec3,
    add_rot: Quat,
    add_trans: Vec3,
    weight: f32,
) -> (Quat, Vec3) {
    let trans = vec3_add_scaled(dest_trans, add_trans, weight);
    let len_sq = add_rot[0] * add_rot[0]
        + add_rot[1] * add_rot[1]
        + add_rot[2] * add_rot[2]
        + add_rot[3] * add_rot[3];
    if len_sq == 0.0 {
        return (dest_rot, trans);
    }
    let len = libm::sqrtf(len_sq);
    let inv = 1.0 / len;
    let mut aq = [
        add_rot[0] * inv,
        add_rot[1] * inv,
        add_rot[2] * inv,
        add_rot[3] * inv,
    ];
    let s = weight * len;
    aq[0] *= s;
    aq[1] *= s;
    aq[2] *= s;
    aq[3] = (1.0 - s) + aq[3] * s;
    (quat_mul(aq, dest_rot), trans)
}
