pub const MODEL_LIGHTING_SAMPLER_STAGE: u8 = 4;

pub const MODEL_LIGHTING_SAMPLER_DCL_USAGE: u32 = 0xa000_0000;

pub const BASE_LIGHTING_COORDS_TEXCOORD: u8 = 6;

pub const VERTEX_COLOR_DCL_USAGE: u32 = 0x8000_000a;

pub fn model_lighting_lookup_coords(
    normal: [f32; 3],
    lighting_lookup_scale: [f32; 3],
    base_lighting_coords: [f32; 3],
) -> [f32; 3] {
    let dominant = max_component(normal);
    let inverse = 1.0 / dominant;
    [
        normal[0] * lighting_lookup_scale[0] * inverse + base_lighting_coords[0],
        normal[1] * lighting_lookup_scale[1] * inverse + base_lighting_coords[1],
        normal[2] * lighting_lookup_scale[2] * inverse + base_lighting_coords[2],
    ]
}

fn max_component(normal: [f32; 3]) -> f32 {
    let yz = if normal[1] > normal[2] {
        normal[1]
    } else {
        normal[2]
    };
    if normal[0] > yz { normal[0] } else { yz }
}

pub fn decode_model_lighting_sample(sample: [f32; 3]) -> [f32; 3] {
    let scaled = [
        sample[0] + sample[0],
        sample[1] + sample[1],
        sample[2] + sample[2],
    ];
    [
        scaled[0] * scaled[0],
        scaled[1] * scaled[1],
        scaled[2] * scaled[2],
    ]
}

pub fn lit_albedo(color_map: [f32; 3], vertex_color: [f32; 3]) -> [f32; 3] {
    let tinted = [
        color_map[0] * vertex_color[0],
        color_map[1] * vertex_color[1],
        color_map[2] * vertex_color[2],
    ];
    [
        tinted[0] * tinted[0],
        tinted[1] * tinted[1],
        tinted[2] * tinted[2],
    ]
}

pub const SPECULAR_MAP_SAMPLER_STAGE: u8 = 6;

pub const REFLECTION_PROBE_SAMPLER_STAGE: u8 = 1;

pub const ENV_MAP_LOD_SCALE: f32 = 8.0;

pub const ENV_MAP_LOD_BIAS: f32 = 6.0;

pub fn model_lighting_reflect(view_dir: [f32; 3], normal: [f32; 3]) -> [f32; 3] {
    let vn = view_dir[0] * normal[0] + view_dir[1] * normal[1] + view_dir[2] * normal[2];
    let two = vn + vn;
    [
        view_dir[0] - normal[0] * two,
        view_dir[1] - normal[1] * two,
        view_dir[2] - normal[2] * two,
    ]
}

pub fn model_lighting_env_lod(specular_alpha: f32) -> f32 {
    ENV_MAP_LOD_BIAS - ENV_MAP_LOD_SCALE * specular_alpha
}

pub fn model_lighting_env_intensity(view_dot_normal: f32, env_map_parms: [f32; 4]) -> f32 {
    let mut base = 1.0 + view_dot_normal;
    if base < 0.0 {
        base = 0.0;
    } else if base > 1.0 {
        base = 1.0;
    }
    let t = libm::powf(base, env_map_parms[2]);
    env_map_parms[0] + (env_map_parms[1] - env_map_parms[0]) * t
}

pub fn model_lighting_specular_term(
    probe_rgb: [f32; 3],
    specular_rgb: [f32; 3],
    intensity: f32,
    probe_exponent: f32,
) -> [f32; 3] {
    let spec2 = [
        specular_rgb[0] * specular_rgb[0],
        specular_rgb[1] * specular_rgb[1],
        specular_rgb[2] * specular_rgb[2],
    ];
    let probe_pow = [
        libm::powf(probe_rgb[0].max(0.0), probe_exponent),
        libm::powf(probe_rgb[1].max(0.0), probe_exponent),
        libm::powf(probe_rgb[2].max(0.0), probe_exponent),
    ];
    [
        probe_pow[0] * spec2[0] * intensity,
        probe_pow[1] * spec2[1] * intensity,
        probe_pow[2] * spec2[2] * intensity,
    ]
}

pub fn model_lighting_specular_term_square(
    probe_rgb: [f32; 3],
    specular_rgb: [f32; 3],
    intensity: f32,
) -> [f32; 3] {
    let spec2 = [
        specular_rgb[0] * specular_rgb[0],
        specular_rgb[1] * specular_rgb[1],
        specular_rgb[2] * specular_rgb[2],
    ];
    let probe2 = [
        probe_rgb[0] * probe_rgb[0],
        probe_rgb[1] * probe_rgb[1],
        probe_rgb[2] * probe_rgb[2],
    ];
    [
        probe2[0] * spec2[0] * intensity,
        probe2[1] * spec2[1] * intensity,
        probe2[2] * spec2[2] * intensity,
    ]
}

pub fn lit_sun_lighting(
    decoded_ambient: [f32; 3],
    sample_alpha: f32,
    n_dot_l_sat: f32,
    light_diffuse_rgb: [f32; 3],
) -> [f32; 3] {
    [
        decoded_ambient[0] + sample_alpha * n_dot_l_sat * light_diffuse_rgb[0],
        decoded_ambient[1] + sample_alpha * n_dot_l_sat * light_diffuse_rgb[1],
        decoded_ambient[2] + sample_alpha * n_dot_l_sat * light_diffuse_rgb[2],
    ]
}

pub const MODEL_LIGHTING_LOCAL_TILE_LOOKUP_SCALE: f32 =
    crate::expand::MODEL_LIGHTING_LOOKUP_SCALE_W;

pub fn model_lighting_local_tile_uvw(normal: [f32; 3]) -> Option<[f32; 3]> {
    let dominant = max_component(normal);
    if dominant == 0.0 {
        return None;
    }
    let inverse = 1.0 / dominant;
    let scale = MODEL_LIGHTING_LOCAL_TILE_LOOKUP_SCALE;
    Some([
        normal[0] * scale * inverse + 0.5,
        normal[1] * scale * inverse + 0.5,
        normal[2] * scale * inverse + 0.5,
    ])
}

pub fn model_lighting_local_tile_nearest_index(normal: [f32; 3]) -> Option<usize> {
    let uvw = model_lighting_local_tile_uvw(normal)?;
    let coord = |u: f32| -> usize {
        let p = libm::roundf(u * (crate::expand::MODEL_LIGHTING_TILE_DIM as f32) - 0.5);
        p.clamp(0.0, 3.0) as usize
    };
    let x = coord(uvw[0]);
    let y = coord(uvw[1]);
    let z = coord(uvw[2]);
    Some(x + crate::expand::MODEL_LIGHTING_TILE_DIM * y + 16 * z)
}

pub fn lit_fragment_color(
    lit_albedo: [f32; 3],
    decoded_lighting: [f32; 3],
    specular_term: [f32; 3],
) -> [f32; 3] {
    [
        lit_albedo[0] * decoded_lighting[0] + specular_term[0],
        lit_albedo[1] * decoded_lighting[1] + specular_term[1],
        lit_albedo[2] * decoded_lighting[2] + specular_term[2],
    ]
}
