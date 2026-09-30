@group(0) @binding(0) var scene: texture_2d<f32>;
@group(0) @binding(1) var scene_sampler: sampler;
@group(0) @binding(2) var<uniform> grade: vec4<f32>;
struct Vertex { @builtin(position) position: vec4<f32>, @location(0) uv: vec2<f32> }
@vertex fn vertex(@builtin(vertex_index) i: u32) -> Vertex {
    let uv = vec2<f32>(f32((i << 1u) & 2u), f32(i & 2u));
    return Vertex(vec4<f32>(uv * vec2<f32>(2.0, -2.0) + vec2<f32>(-1.0, 1.0), 0.0, 1.0), uv);
}
@fragment fn fragment(v: Vertex) -> @location(0) vec4<f32> {
    let sampled = textureSample(scene, scene_sampler, v.uv);
    // Rotate chroma in YIQ while preserving luminance.
    let y = dot(sampled.rgb, vec3<f32>(0.299, 0.587, 0.114));
    let i = dot(sampled.rgb, vec3<f32>(0.596, -0.274, -0.322));
    let q = dot(sampled.rgb, vec3<f32>(0.211, -0.523, 0.312));
    let ri = i * cos(grade.x) - q * sin(grade.x);
    let rq = i * sin(grade.x) + q * cos(grade.x);
    let rotated = vec3<f32>(y + 0.956 * ri + 0.621 * rq, y - 0.272 * ri - 0.647 * rq, y - 1.106 * ri + 1.703 * rq);
    let original = vec3<f32>(y + 0.956 * i + 0.621 * q, y - 0.272 * i - 0.647 * q, y - 1.106 * i + 1.703 * q);
    let hue = sampled.rgb + rotated - original;
    let saturated = mix(vec3<f32>(y), hue, grade.w);
    let rgb = pow(max(saturated * exp2(grade.z), vec3<f32>(0.0)), vec3<f32>(1.0 / grade.y));
    return vec4<f32>(rgb, sampled.a);
}
