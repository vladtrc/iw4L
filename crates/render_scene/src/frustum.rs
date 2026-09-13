use bevy::prelude::*;

#[derive(Clone, Copy, Debug)]
pub struct FrustumSpec {
    pub eye: Vec3,
    pub forward: Vec3,
    pub right: Vec3,
    pub up: Vec3,
    pub fov_y_rad: f32,
    pub aspect: f32,
    pub near: f32,
    pub far: f32,
}

pub fn perspective_frustum_planes(spec: FrustumSpec) -> [[f32; 4]; 6] {
    let forward = spec.forward.normalize_or_zero();
    let right = spec.right.normalize_or_zero();
    let up = spec.up.normalize_or_zero();
    let tan_y = (spec.fov_y_rad * 0.5).tan();
    let tan_x = tan_y * spec.aspect.max(1e-6);

    let near_c = spec.eye + forward * spec.near;
    let far_c = spec.eye + forward * spec.far;

    let left_ray = (forward - right * tan_x).normalize_or_zero();
    let right_ray = (forward + right * tan_x).normalize_or_zero();
    let down_ray = (forward - up * tan_y).normalize_or_zero();
    let up_ray = (forward + up * tan_y).normalize_or_zero();

    let mut n_left = left_ray.cross(up).normalize_or_zero();
    let mut n_right = up.cross(right_ray).normalize_or_zero();
    let mut n_bottom = right.cross(down_ray).normalize_or_zero();
    let mut n_top = up_ray.cross(right).normalize_or_zero();

    let ahead = spec.eye + forward * (spec.near + spec.far) * 0.5;
    let flip_if_rejects = |n: &mut Vec3| {
        if n.dot(ahead - spec.eye) < 0.0 {
            *n = -*n;
        }
    };
    flip_if_rejects(&mut n_left);
    flip_if_rejects(&mut n_right);
    flip_if_rejects(&mut n_bottom);
    flip_if_rejects(&mut n_top);

    [
        plane_through(near_c, forward),
        plane_through(far_c, -forward),
        plane_through(spec.eye, n_left),
        plane_through(spec.eye, n_right),
        plane_through(spec.eye, n_bottom),
        plane_through(spec.eye, n_top),
    ]
}

fn plane_through(point: Vec3, normal: Vec3) -> [f32; 4] {
    let n = normal.normalize_or_zero();
    [n.x, n.y, n.z, -n.dot(point)]
}

#[must_use]
pub fn clip_from_world_frustum_planes(clip: Mat4) -> [[f32; 4]; 6] {
    let c0 = clip.row(0);
    let c1 = clip.row(1);
    let c2 = clip.row(2);
    let c3 = clip.row(3);
    let raw = [c3 + c0, c3 - c0, c3 + c1, c3 - c1, c2, c3 - c2];
    raw.map(|p| {
        let len = (p.x * p.x + p.y * p.y + p.z * p.z).sqrt();
        if len > 1e-8 {
            [p.x / len, p.y / len, p.z / len, p.w / len]
        } else {
            [0.0, 0.0, 0.0, 0.0]
        }
    })
}
