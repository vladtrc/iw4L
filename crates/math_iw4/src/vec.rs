#[inline]
pub fn vec3_length(v: [f32; 3]) -> f32 {
    let [x, y, z] = v;
    libm::sqrtf(z * z + x * x + y * y)
}
