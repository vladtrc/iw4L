struct HistoryOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) uv: vec2<f32>,
}

struct BillboardOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) uv: vec2<f32>,
    @location(1) fade: f32,
}

struct BlindOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) uv: vec2<f32>,
}

struct SunParams {
    clip: vec4<f32>,
    viewport: vec4<f32>,
    fade: vec4<f32>,
    blind: vec4<f32>,
    glare: vec4<f32>,
}

@group(0) @binding(0) var<uniform> params: SunParams;
@group(0) @binding(1) var depth_tex: texture_depth_2d;
@group(0) @binding(2) var prev_history: texture_2d<f32>;
@group(0) @binding(3) var color_tex: texture_2d<f32>;
@group(0) @binding(4) var color_samp: sampler;
@group(0) @binding(5) var scene_tex: texture_2d<f32>;

fn update_over_time(current: f32, goal: f32, fade_in_ms: f32, fade_out_ms: f32, dt_ms: f32) -> f32 {
    if goal > current {
        if fade_in_ms <= 0.0 {
            return goal;
        }
        return min(goal, current + dt_ms / fade_in_ms);
    }
    if goal < current {
        if fade_out_ms <= 0.0 {
            return goal;
        }
        return max(goal, current - dt_ms / fade_out_ms);
    }
    return current;
}

fn sun_uv() -> vec2<f32> {
    let w = max(params.clip.w, 1.0e-6);
    let ndc = params.clip.xy / w;
    return vec2<f32>(ndc.x * 0.5 + 0.5, ndc.y * -0.5 + 0.5);
}

fn probe_visibility() -> f32 {
    if params.glare.z > 0.5 {
        return 0.0;
    }
    if params.glare.w > 0.5 {
        return 0.0;
    }
    let uv = sun_uv();
    if uv.x < 0.0 || uv.x > 1.0 || uv.y < 0.0 || uv.y > 1.0 {
        return 0.0;
    }
    let size = vec2<f32>(params.viewport.x, params.viewport.y);
    let center = uv * size;
    let half = vec2<f32>(8.0, 8.0);
    var visible = 0.0;
    var counted = 0.0;
    for (var y = 0; y < 4; y = y + 1) {
        for (var x = 0; x < 4; x = x + 1) {
            let offset = vec2<f32>(f32(x) - 1.5, f32(y) - 1.5) * (half / 1.5);
            let pixel = center + offset;
            if pixel.x < 0.0 || pixel.y < 0.0 || pixel.x >= size.x || pixel.y >= size.y {
                continue;
            }
            let depth = textureLoad(depth_tex, vec2<i32>(pixel), 0);
            counted = counted + 1.0;
            if depth <= 0.002 {
                visible = visible + 1.0;
            }
        }
    }
    if counted <= 0.0 {
        return 0.0;
    }
    return visible / counted;
}

@vertex
fn vs_history(@builtin(vertex_index) vertex_index: u32) -> HistoryOut {
    var out: HistoryOut;
    let uv = vec2<f32>(f32((vertex_index << 1u) & 2u), f32(vertex_index & 2u));
    out.clip = vec4<f32>(uv * vec2<f32>(2.0, -2.0) + vec2<f32>(-1.0, 1.0), 0.0, 1.0);
    out.uv = uv;
    return out;
}

@fragment
fn fs_history(_in: HistoryOut) -> @location(0) vec4<f32> {
    var vis = probe_visibility();
    var prev = textureLoad(prev_history, vec2<i32>(0, 0), 0);
    if params.glare.z > 0.5 {
        prev = vec4<f32>(0.0);
        vis = 0.0;
    }
    let dt = max(params.fade.y, 0.0);
    let flare = update_over_time(prev.g, vis, params.fade.z, params.fade.w, dt);
    let blind = update_over_time(prev.b, vis * params.blind.x, params.blind.y, params.blind.z, dt);
    let glare = update_over_time(prev.a, vis * params.blind.w, params.glare.x, params.glare.y, dt);
    return vec4<f32>(vis, flare, blind, glare);
}

@vertex
fn vs_billboard(@builtin(vertex_index) vertex_index: u32) -> BillboardOut {
    var out: BillboardOut;
    let corners = array<vec2<f32>, 6>(
        vec2<f32>(1.0, 1.0),
        vec2<f32>(1.0, -1.0),
        vec2<f32>(-1.0, -1.0),
        vec2<f32>(1.0, 1.0),
        vec2<f32>(-1.0, -1.0),
        vec2<f32>(-1.0, 1.0),
    );
    let o = corners[vertex_index];
#ifdef SUN_FLARE
    let size = params.viewport.w;
#else
    let size = params.viewport.z;
#endif
    var clip = params.clip;
    clip.x = clip.x + o.x * clip.w * (size / 640.0);
    clip.y = clip.y + o.y * clip.w * (size / 480.0);
    out.clip = clip;
    out.uv = vec2<f32>(o.x * 0.5 + 0.5, -o.y * 0.5 + 0.5);
    out.fade = params.fade.x;
    return out;
}

@fragment
fn fs_sprite(in: BillboardOut) -> @location(0) vec4<f32> {
    let history = textureLoad(prev_history, vec2<i32>(0, 0), 0);
    let color = textureSample(color_tex, color_samp, in.uv);
    return color * history.r;
}

@fragment
fn fs_flare(in: BillboardOut) -> @location(0) vec4<f32> {
    let history = textureLoad(prev_history, vec2<i32>(0, 0), 0);
    let color = textureSample(color_tex, color_samp, in.uv);
    return color * (history.g * in.fade);
}

@vertex
fn vs_blind(@builtin(vertex_index) vertex_index: u32) -> BlindOut {
    var out: BlindOut;
    let uv = vec2<f32>(f32((vertex_index << 1u) & 2u), f32(vertex_index & 2u));
    out.clip = vec4<f32>(uv * vec2<f32>(2.0, -2.0) + vec2<f32>(-1.0, 1.0), 0.0, 1.0);
    out.uv = uv;
    return out;
}

@fragment
fn fs_blind(in: BlindOut) -> @location(0) vec4<f32> {
    let history = textureLoad(prev_history, vec2<i32>(0, 0), 0);
    let scene = textureSample(scene_tex, color_samp, in.uv);
    let darkened = scene.rgb * (1.0 - clamp(history.b, 0.0, 1.0));
    return vec4<f32>(darkened + vec3<f32>(clamp(history.a, 0.0, 1.0)), scene.a);
}
