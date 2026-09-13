#import bevy_render::view::View

@group(0) @binding(0) var<uniform> view: View;

struct VertexIn {
    @location(0) position: vec3<f32>,
    @location(1) normal: vec3<f32>,
    @location(3) color: vec4<f32>,
    @location(4) uv: vec2<f32>,
}

struct SmodelVertexIn {
    @location(0) position: vec3<f32>,
    @location(1) normal: vec3<f32>,
    @location(2) color: vec4<f32>,
    @location(3) uv: vec2<f32>,
    @location(6) world_from_local_0: vec4<f32>,
    @location(7) world_from_local_1: vec4<f32>,
    @location(8) world_from_local_2: vec4<f32>,
    @location(9) world_from_local_3: vec4<f32>,
}

struct VertexOut {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) normal: vec3<f32>,
    @location(1) color: vec4<f32>,
    @location(2) uv: vec2<f32>,
}

@vertex
fn vertex(in: VertexIn) -> VertexOut {
    var out: VertexOut;
    out.clip_position = view.clip_from_world * vec4(in.position, 1.0);
    out.normal = in.normal;
    out.color = in.color;
    out.uv = in.uv;
    return out;
}

@vertex
fn vertex_smodel(in: SmodelVertexIn) -> VertexOut {
    let world_from_local = mat4x4<f32>(
        in.world_from_local_0,
        in.world_from_local_1,
        in.world_from_local_2,
        in.world_from_local_3,
    );
    var out: VertexOut;
    out.clip_position = view.clip_from_world * world_from_local * vec4(in.position, 1.0);
    out.normal = (world_from_local * vec4(in.normal, 0.0)).xyz;
    out.color = in.color;
    out.uv = in.uv;
    return out;
}

@fragment
fn fragment(in: VertexOut) -> @location(0) vec4<f32> {
    let normal_colour = abs(normalize(in.normal)) * 0.72 + vec3(0.08, 0.02, 0.10);
    let cell = i32(floor(in.uv.x * 16.0) + floor(in.uv.y * 16.0)) & 1;
    let diagnostic_tint = select(vec3(0.72, 0.22, 0.78), vec3(0.22, 0.72, 0.78), cell == 0);
    return vec4(mix(normal_colour, diagnostic_tint, 0.28) * max(in.color.rgb, vec3(0.35)), 1.0);
}
