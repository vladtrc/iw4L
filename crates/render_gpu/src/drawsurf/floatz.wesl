struct VsOut { @builtin(position) position: vec4<f32> }
@vertex
fn vs_fullscreen(@builtin(vertex_index) vertex_index: u32) -> VsOut {
    let uv = vec2<f32>(f32((vertex_index << 1u) & 2u), f32(vertex_index & 2u));
    return VsOut(vec4<f32>(uv * vec2<f32>(2.0, -2.0) + vec2<f32>(-1.0, 1.0), 0.0, 1.0));
}
@if(MULTISAMPLED)
@group(0) @binding(0) var effect_depth: texture_multisampled_2d<f32>;
@else
@group(0) @binding(0) var effect_depth: texture_2d<f32>;

@fragment
fn fs_floatz(in: VsOut) -> @location(0) f32 {
    let coords = vec2<i32>(floor(in.position.xy));
    var depth = textureLoad(effect_depth, coords, 0).x;
    @if(MULTISAMPLED) {
        for (var sample = 1u; sample < textureNumSamples(effect_depth); sample += 1u) {
            let next = textureLoad(effect_depth, coords, i32(sample)).x;
            if abs(next) < abs(depth) { depth = next; }
        }
    }
    return depth;
}
