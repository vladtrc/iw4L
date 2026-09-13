use bevy::math::Vec3;

pub fn distance_inches(a: [f32; 3], b: [f32; 3]) -> f32 {
    let dx = a[0] - b[0];
    let dy = a[1] - b[1];
    let dz = a[2] - b[2];
    (dx * dx + dy * dy + dz * dz).sqrt()
}

pub fn transform_inches(t: Vec3) -> [f32; 3] {
    t.to_array()
}
