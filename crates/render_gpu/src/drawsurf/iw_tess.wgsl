struct VsIn {
    @location(0) xyzw: vec4<f32>,
    @location(1) color: vec4<f32>,
    @location(2) uv: vec2<f32>,
}

struct VsOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) uv: vec2<f32>,
    @location(1) color: vec4<f32>,
}

struct Params {
    set2d: vec4<f32>,
}

@group(0) @binding(0) var<uniform> params: Params;
@group(0) @binding(1) var color_tex: texture_2d<f32>;
@group(0) @binding(2) var color_samp: sampler;

@vertex
fn vs_tess(in: VsIn) -> VsOut {
    var out: VsOut;
    let x = in.xyzw.x * params.set2d.x + params.set2d.z;
    let y = in.xyzw.y * params.set2d.y + params.set2d.w;
    out.clip = vec4<f32>(x, y, 0.0, 1.0);
    out.uv = in.uv;

    out.color = vec4<f32>(in.color.b, in.color.g, in.color.r, in.color.a);
    return out;
}

@fragment
fn fs_tess(in: VsOut) -> @location(0) vec4<f32> {
    return textureSample(color_tex, color_samp, in.uv) * in.color;
}
