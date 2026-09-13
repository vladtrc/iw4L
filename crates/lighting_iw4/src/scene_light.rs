pub const GFX_LIGHT_TYPE_DIR: u8 = 1;

pub const GFX_LIGHT_TYPE_SPOT: u8 = 2;

pub const GFX_LIGHT_TYPE_OMNI: u8 = 3;

pub const R_COLOR_SCALE_DEFAULT: f32 = 1.0;

pub const CONST_SRC_CODE_LIGHT_POSITION: u16 = 0;

pub const CONST_SRC_CODE_LIGHT_DIFFUSE: u16 = 1;

pub const CONST_SRC_CODE_LIGHT_SPECULAR: u16 = 2;

pub const CONST_SRC_CODE_LIGHT_SPOTDIR: u16 = 3;

pub const CONST_SRC_CODE_LIGHT_SPOTFACTORS: u16 = 4;

pub const CONST_SRC_CODE_LIGHT_FALLOFF_PLACEMENT: u16 = 5;

pub const LIGHT_PACK_ONE: f32 = 1.0;

pub const COLOR_SRGB_THRESHOLD: f32 = f32::from_bits(0x3d20_e411);

pub const COLOR_SRGB_LINEAR_SCALE: f32 = f32::from_bits(0x3d9e_8391);

pub const COLOR_SRGB_OFFSET: f32 = f32::from_bits(0x3d61_47ae);

pub const COLOR_SRGB_POW_SCALE: f32 = f32::from_bits(0x3f72_a76f);

pub const COLOR_SRGB_POW_EXP: f32 = f32::from_bits(0x4019_999a);

pub fn color_srgb_to_linear(x: f32) -> f32 {
    if x > COLOR_SRGB_THRESHOLD {
        libm::powf(
            (x + COLOR_SRGB_OFFSET) * COLOR_SRGB_POW_SCALE,
            COLOR_SRGB_POW_EXP,
        )
    } else {
        x * COLOR_SRGB_LINEAR_SCALE
    }
}

pub const LIGHT_FALLOFF_PLACEMENT_SCALE: f32 = 0.001953125;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct GfxLightPack {
    pub light_type: u8,
    pub color: [f32; 3],
    pub direction: [f32; 3],
    pub origin: [f32; 3],
    pub radius: f32,
    pub cos_outer: f32,
    pub cos_inner: f32,
    pub exponent: u8,

    pub falloff_image_width: Option<u16>,

    pub lmap_lookup_start: i32,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum ShadowableLightPack {
    Unchanged,
    Dir {
        position: [f32; 4],
        diffuse: [f32; 4],
        specular: [f32; 4],
    },
    Omni {
        position: [f32; 4],
        diffuse: [f32; 4],
        specular: [f32; 4],
        spot_dir: [f32; 4],

        falloff_placement: Option<[f32; 4]>,
    },
    Spot {
        position: [f32; 4],
        diffuse: [f32; 4],
        specular: [f32; 4],
        spot_dir: [f32; 4],
        spot_factors: [f32; 4],

        falloff_placement: Option<[f32; 4]>,
    },
}

pub const fn dir_light_position(direction: [f32; 3]) -> [f32; 4] {
    [direction[0], direction[1], direction[2], 0.0]
}

pub fn omni_spot_light_position(origin: [f32; 3], eye: [f32; 3], radius: f32) -> [f32; 4] {
    [
        origin[0] - eye[0],
        origin[1] - eye[1],
        origin[2] - eye[2],
        1.0 / radius,
    ]
}

pub fn light_diffuse(color: [f32; 3], diffuse_color_scale: f32) -> [f32; 4] {
    [
        color_srgb_to_linear(color[0] * diffuse_color_scale),
        color_srgb_to_linear(color[1] * diffuse_color_scale),
        color_srgb_to_linear(color[2] * diffuse_color_scale),
        1.0,
    ]
}

pub fn light_specular(color: [f32; 3], specular_color_scale: f32) -> [f32; 4] {
    [
        color_srgb_to_linear(color[0] * specular_color_scale),
        color_srgb_to_linear(color[1] * specular_color_scale),
        color_srgb_to_linear(color[2] * specular_color_scale),
        1.0,
    ]
}

pub const fn light_spot_dir(direction: [f32; 3]) -> [f32; 4] {
    [direction[0], direction[1], direction[2], 0.0]
}

pub fn light_spot_factors(cos_outer: f32, cos_inner: f32, exponent: u8) -> [f32; 4] {
    let scale = LIGHT_PACK_ONE / (cos_inner - cos_outer);
    [scale, -scale * cos_outer, f32::from(exponent), 0.0]
}

pub fn light_falloff_placement(image_width: u16, lmap_lookup_start: i32) -> [f32; 4] {
    [
        f32::from(image_width) * LIGHT_FALLOFF_PLACEMENT_SCALE,
        0.0,
        lmap_lookup_start as f32 * LIGHT_FALLOFF_PLACEMENT_SCALE,
        0.0,
    ]
}

fn falloff_placement_of(light: &GfxLightPack) -> Option<[f32; 4]> {
    light
        .falloff_image_width
        .map(|width| light_falloff_placement(width, light.lmap_lookup_start))
}

pub fn pack_shadowable_light(
    scene_light_index: u8,
    light: Option<&GfxLightPack>,
    eye: [f32; 3],
    diffuse_color_scale: f32,
    specular_color_scale: f32,
) -> ShadowableLightPack {
    if scene_light_index == 0 {
        return ShadowableLightPack::Unchanged;
    }
    let Some(light) = light else {
        return ShadowableLightPack::Unchanged;
    };
    let diffuse = light_diffuse(light.color, diffuse_color_scale);
    let specular = light_specular(light.color, specular_color_scale);
    if light.light_type == GFX_LIGHT_TYPE_DIR {
        return ShadowableLightPack::Dir {
            position: dir_light_position(light.direction),
            diffuse,
            specular,
        };
    }
    let position = omni_spot_light_position(light.origin, eye, light.radius);
    let spot_dir = light_spot_dir(light.direction);
    let falloff_placement = falloff_placement_of(light);
    if light.light_type == GFX_LIGHT_TYPE_SPOT {
        ShadowableLightPack::Spot {
            position,
            diffuse,
            specular,
            spot_dir,
            spot_factors: light_spot_factors(light.cos_outer, light.cos_inner, light.exponent),
            falloff_placement,
        }
    } else {
        ShadowableLightPack::Omni {
            position,
            diffuse,
            specular,
            spot_dir,
            falloff_placement,
        }
    }
}

pub const R_DLIGHT_SCENE_CAP: u32 = 0x20;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AddOmniLightRefuse {
    NoWorld,
    NonPositiveRadius,
    Cap,
}

pub fn r_add_omni_light_to_scene_allows(
    world_present: bool,
    radius: f32,
    live_count: u32,
) -> Result<(), AddOmniLightRefuse> {
    if !world_present {
        return Err(AddOmniLightRefuse::NoWorld);
    }
    if radius <= 0.0 {
        return Err(AddOmniLightRefuse::NonPositiveRadius);
    }
    if live_count > 0x1f {
        return Err(AddOmniLightRefuse::Cap);
    }
    Ok(())
}

pub fn r_omni_light_pack(origin: [f32; 3], radius: f32, color_bgr: [f32; 3]) -> GfxLightPack {
    GfxLightPack {
        light_type: GFX_LIGHT_TYPE_OMNI,
        color: color_bgr,
        direction: [0.0; 3],
        origin,
        radius,
        cos_outer: 0.0,
        cos_inner: 0.0,
        exponent: 0,
        falloff_image_width: None,
        lmap_lookup_start: 0,
    }
}

pub const R_SPOT_LIGHT_EPS: f32 = 0.1;

pub const R_SPOT_LIGHT_DIR_SIGN: f32 = -1.0;

pub const R_SPOT_LIGHT_START_RADIUS_DEFAULT: f32 = 36.0;

pub const R_SPOT_LIGHT_END_RADIUS_DEFAULT: f32 = 196.0;

pub const R_SPOT_LIGHT_FOV_INNER_FRACTION_DEFAULT: f32 = 0.5;

pub const R_SPOT_LIGHT_BRIGHTNESS_DEFAULT: f32 = 14.0;

pub const R_SPOT_LIGHT_EXPONENT_DEFAULT: u8 = 2;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SpotLightConeDvars {
    pub start_radius: f32,
    pub end_radius: f32,
    pub fov_inner_fraction: f32,
    pub brightness: f32,
    pub exponent: u8,
}

impl SpotLightConeDvars {
    pub const fn register_defaults() -> Self {
        Self {
            start_radius: R_SPOT_LIGHT_START_RADIUS_DEFAULT,
            end_radius: R_SPOT_LIGHT_END_RADIUS_DEFAULT,
            fov_inner_fraction: R_SPOT_LIGHT_FOV_INNER_FRACTION_DEFAULT,
            brightness: R_SPOT_LIGHT_BRIGHTNESS_DEFAULT,
            exponent: R_SPOT_LIGHT_EXPONENT_DEFAULT,
        }
    }
}

pub fn r_spot_light_clamp_end(start_radius: f32, end_radius: f32, radius: f32) -> f32 {
    let mut end = end_radius;
    if end <= start_radius {
        end = start_radius + R_SPOT_LIGHT_EPS;
    }
    if start_radius + radius <= end {
        end = start_radius + radius - R_SPOT_LIGHT_EPS;
    }
    end
}

pub fn r_spot_light_offset(start_radius: f32, end_radius: f32, radius: f32) -> f32 {
    start_radius / ((end_radius - start_radius) / radius)
}

pub fn r_spot_light_pack(
    origin: [f32; 3],
    forward: [f32; 3],
    radius: f32,
    color_bgr: [f32; 3],
    cone: SpotLightConeDvars,
) -> GfxLightPack {
    let end = r_spot_light_clamp_end(cone.start_radius, cone.end_radius, radius);
    let offset = r_spot_light_offset(cone.start_radius, end, radius);
    let dir = [
        forward[0] * R_SPOT_LIGHT_DIR_SIGN,
        forward[1] * R_SPOT_LIGHT_DIR_SIGN,
        forward[2] * R_SPOT_LIGHT_DIR_SIGN,
    ];
    let origin = [
        origin[0] + offset * dir[0],
        origin[1] + offset * dir[1],
        origin[2] + offset * dir[2],
    ];
    let outer = libm::atanf((end - cone.start_radius) / radius);
    let inner = outer * cone.fov_inner_fraction;
    GfxLightPack {
        light_type: GFX_LIGHT_TYPE_SPOT,
        color: [
            color_bgr[0] * cone.brightness,
            color_bgr[1] * cone.brightness,
            color_bgr[2] * cone.brightness,
        ],
        direction: dir,
        origin,
        radius: radius + offset,
        cos_outer: libm::cosf(outer),
        cos_inner: libm::cosf(inner),
        exponent: cone.exponent,
        falloff_image_width: None,
        lmap_lookup_start: 0,
    }
}
