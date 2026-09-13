struct VsOut {
    @builtin(position) position: vec4<f32>,
}

struct Params {
    znear: vec4<f32>,
}

@vertex
fn vs_fullscreen(@builtin(vertex_index) vertex_index: u32) -> VsOut {
    var out: VsOut;
    let uv = vec2<f32>(f32((vertex_index << 1u) & 2u), f32(vertex_index & 2u));
    out.position = vec4<f32>(uv * vec2<f32>(2.0, -2.0) + vec2<f32>(-1.0, 1.0), 0.0, 1.0);
    return out;
}

#ifdef MULTISAMPLED
@group(0) @binding(0) var depth_tex: texture_depth_multisampled_2d;
#else
@group(0) @binding(0) var depth_tex: texture_depth_2d;
#endif
@group(0) @binding(1) var<uniform> params: Params;

@fragment
fn fs_floatz(in: VsOut) -> @location(0) f32 {
    let coords = vec2<i32>(floor(in.position.xy));
    let depth = textureLoad(depth_tex, coords, 0);
    if depth > params.znear.z {
        let ndc = (depth - params.znear.z) / params.znear.w;
        return -params.znear.y / ndc;
    }

    let ndc = depth / params.znear.z;
    return params.znear.x / max(ndc, 1.0e-7);
}
