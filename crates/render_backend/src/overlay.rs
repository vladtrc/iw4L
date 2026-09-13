use glam::{Mat4, Quat, Vec3};
pub use render_frame::code_math::{
    camera_relative_view_projection, code_transpose_matrix_row4, code_transpose_matrix_rows,
    float4_bits,
};
use render_frame::{LightAttenuationBind, OutdoorLookup, T5LightFalloffPack};
use render_material::{RuntimeCodeSources, RuntimeImageId};

pub const CODE_LIGHT_POSITION: u16 = lighting_iw4::CONST_SRC_CODE_LIGHT_POSITION;

pub const CODE_SPOT_SHADOWMAP_PIXEL_ADJUST: u16 = 0x35;

pub const CODE_LIGHT_DIFFUSE: u16 = lighting_iw4::CONST_SRC_CODE_LIGHT_DIFFUSE;

pub const CODE_LIGHT_SPECULAR: u16 = lighting_iw4::CONST_SRC_CODE_LIGHT_SPECULAR;

pub const CODE_LIGHT_SPOTDIR: u16 = lighting_iw4::CONST_SRC_CODE_LIGHT_SPOTDIR;

pub const CODE_LIGHT_SPOTFACTORS: u16 = lighting_iw4::CONST_SRC_CODE_LIGHT_SPOTFACTORS;

pub const CODE_LIGHT_FALLOFF_PLACEMENT: u16 = lighting_iw4::CONST_SRC_CODE_LIGHT_FALLOFF_PLACEMENT;

pub const CODE_BASE_LIGHTING_COORDS: u16 = 0x3a;

pub const CODE_LIGHT_PROBE_AMBIENT: u16 = 0x3b;

pub const CODE_MESH_ARG_0: u16 = 0x4a;

pub const CODE_MESH_ARG_1: u16 = 0x4b;
pub const CODE_LEFTOVER_T5_LIGHT_ATTENUATION: u16 = 0x200 + 0x5;

pub const CODE_LEFTOVER_T5_LIGHT_FALLOFF_A: u16 = 0x200 + 0x6;

pub const CODE_LEFTOVER_T5_LIGHT_FALLOFF_B: u16 = 0x200 + 0x7;

pub const CODE_LEFTOVER_T5_LIGHT_SPOT_MATRIX0: u16 = 0x200 + 0x8;

pub const CODE_LEFTOVER_T5_LIGHT_SPOT_MATRIX1: u16 = 0x200 + 0x9;

pub const CODE_LEFTOVER_T5_LIGHT_SPOT_MATRIX2: u16 = 0x200 + 0xa;

pub const CODE_LEFTOVER_T5_LIGHT_SPOT_MATRIX3: u16 = 0x200 + 0xb;

pub const CODE_LEFTOVER_T5_LIGHT_SPOT_AABB: u16 = 0x200 + 0xc;

pub const CODE_LEFTOVER_T5_LIGHT_CONE_CONTROL1: u16 = 0x200 + 0xd;

pub const CODE_LEFTOVER_T5_LIGHT_CONE_CONTROL2: u16 = 0x200 + 0xe;

pub const CODE_LEFTOVER_T5_LIGHT_SPOT_COOKIE_SLIDE: u16 = 0x200 + 0xf;
pub const T5_LIGHT_ATTENUATION_EPSILON: f32 = 0.000015287891;

pub const T5_LIGHT_ATTENUATION_DEFAULT: [f32; 4] = [1.0, 0.0, 0.0, 1.0];

pub const T5_LIGHT_FALLOFF_NEAR: f32 = 0.0;

pub const T5_LIGHT_AABB_DEFAULT: [f32; 4] = [0.75, 1.0, 0.75, 1.0];

pub const T5_LIGHT_SPOT_ROLL_DEFAULT: f32 = 0.0;

pub const T5_LIGHT_COOKIE_DEFAULT: [f32; 4] = [0.0, 0.0, 0.0, 0.0];

pub const CODE_TRANSPOSE_VIEW_PROJECTION: u16 = 0x56;

pub const CODE_TRANSPOSE_WORLD0: u16 = 0x62;

pub const CODE_TRANSPOSE_WORLD1: u16 = 0x6e;

pub const CODE_TRANSPOSE_WORLD2: u16 = 0x7a;

pub const CODE_TRANSPOSE_WORLD_VIEW_PROJECTION0: u16 = 0x6a;

pub const CODE_TRANSPOSE_WORLD_VIEW_PROJECTION1: u16 = 0x76;
pub const CODE_TRANSPOSE_WORLD_VIEW_PROJECTION2: u16 = 0x82;

pub const CODE_TRANSPOSE_WORLD_VIEW0: u16 = 0x66;
pub const CODE_TRANSPOSE_WORLD_VIEW1: u16 = 0x72;
pub const CODE_TRANSPOSE_WORLD_VIEW2: u16 = 0x7e;

pub const CODE_INVERSE_TRANSPOSE_WORLD_VIEW0: u16 = 0x67;

pub const CODE_MATERIAL_COLOR: u16 = 0x24;

pub const CODE_TRANSPOSE_PROJECTION: u16 = 0x52;

pub const CODE_TRANSPOSE_WORLD_OUTDOOR_LOOKUP: u16 = 0x5e;

pub const CODE_TEXTURE_OUTDOOR: u32 = 0x0e;

pub const CODE_TEXTURE_OUTDOOR_SAMPLER: u8 = 0x62;

pub const CODE_DEPTH_FROM_CLIP: u16 = 0x49;
pub const CODE_TEXTURE_LIGHT_ATTENUATION: u32 =
    lighting_iw4::TEXTURE_SRC_CODE_LIGHT_ATTENUATION as u32;

pub const CODE_SHADOWMAP_POLYGON_OFFSET: u16 = 0x15;

pub const CODE_SHADOWMAP_LOOKUP_TRANSPOSE: u16 = 0x5a;

pub(crate) fn tess_base_lighting_handle(lighting_handle: u32) -> u16 {
    u16::try_from(lighting_handle)
        .ok()
        .filter(|&handle| handle != 0)
        .unwrap_or(lighting_iw4::model_lighting_handle_from_entry(0))
}

pub fn overlay_smodel_tess_code_constants(
    sources: &mut RuntimeCodeSources,
    lighting_handle: u32,
    inv_image_height: Option<f32>,
    packed_lighting: Option<[u8; 4]>,
    need: OverlayCodeNeed,
) {
    if need.lighting_coords
        && let Some(inv_h) = inv_image_height
        && let Some(coords) = lighting_iw4::model_lighting_coords_from_handle(
            tess_base_lighting_handle(lighting_handle),
            inv_h,
        )
    {
        sources.set_constant_rows(CODE_BASE_LIGHTING_COORDS, &[float4_bits(coords.as_array())]);
    }
    if need.light_probe
        && let Some(packed) = packed_lighting
    {
        sources.set_constant_rows(
            CODE_LIGHT_PROBE_AMBIENT,
            &[float4_bits(lighting_iw4::light_probe_ambient_from_packed(
                packed,
            ))],
        );
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct OverlayCodeNeed {
    pub world0: bool,
    pub world_view: bool,
    pub world_view_proj: bool,
    pub inverse_world_view: bool,
    pub lighting_coords: bool,
    pub light_probe: bool,
}

impl OverlayCodeNeed {
    pub const ALL: Self = Self {
        world0: true,
        world_view: true,
        world_view_proj: true,
        inverse_world_view: true,
        lighting_coords: true,
        light_probe: true,
    };
    pub const NONE: Self = Self {
        world0: false,
        world_view: false,
        world_view_proj: false,
        inverse_world_view: false,
        lighting_coords: false,
        light_probe: false,
    };

    pub fn note(&mut self, index: u16) {
        match index {
            CODE_TRANSPOSE_WORLD0 => self.world0 = true,
            CODE_TRANSPOSE_WORLD_VIEW0 => self.world_view = true,
            CODE_TRANSPOSE_WORLD_VIEW_PROJECTION0 => self.world_view_proj = true,
            CODE_INVERSE_TRANSPOSE_WORLD_VIEW0 => self.inverse_world_view = true,
            CODE_BASE_LIGHTING_COORDS => self.lighting_coords = true,
            CODE_LIGHT_PROBE_AMBIENT => self.light_probe = true,
            _ => {}
        }
    }
}

pub fn overlay_smodel_world_matrix(
    sources: &mut RuntimeCodeSources,
    view_origin: Vec3,
    world_from_local: Mat4,
    clip_from_world: Option<Mat4>,
    view_from_world: Option<Mat4>,
    need: OverlayCodeNeed,
) {
    if need.world0 {
        sources.set_constant_rows(
            CODE_TRANSPOSE_WORLD0,
            &code_transpose_matrix_row4(Mat4::from_translation(-view_origin) * world_from_local),
        );
    }
    if need.world_view_proj
        && let Some(clip) = clip_from_world
    {
        sources.set_constant_rows(
            CODE_TRANSPOSE_WORLD_VIEW_PROJECTION0,
            &code_transpose_matrix_row4(clip * world_from_local),
        );
    }
    if (need.world_view || need.inverse_world_view)
        && let Some(view) = view_from_world
    {
        let world_view = view * world_from_local;
        if need.world_view {
            sources.set_constant_rows(
                CODE_TRANSPOSE_WORLD_VIEW0,
                &code_transpose_matrix_row4(world_view),
            );
        }
        if need.inverse_world_view {
            sources.set_constant_rows(
                CODE_INVERSE_TRANSPOSE_WORLD_VIEW0,
                &code_transpose_matrix_row4(world_view.inverse()),
            );
        }
    }
}

pub fn overlay_particle_cloud_tess_code_constants(
    sources: &mut RuntimeCodeSources,
    view_origin: Vec3,
    clouds: &[fx_iw4::GfxParticleCloud; 3],
    clip_from_world: Option<Mat4>,
    view_from_world: Option<Mat4>,
    clip_from_view: Option<Mat4>,
    outdoor: Option<&OutdoorLookup>,
) {
    let world_indices = [
        CODE_TRANSPOSE_WORLD0,
        CODE_TRANSPOSE_WORLD1,
        CODE_TRANSPOSE_WORLD2,
    ];
    let world_view_indices = [
        CODE_TRANSPOSE_WORLD_VIEW0,
        CODE_TRANSPOSE_WORLD_VIEW1,
        CODE_TRANSPOSE_WORLD_VIEW2,
    ];
    let wvp_indices = [
        CODE_TRANSPOSE_WORLD_VIEW_PROJECTION0,
        CODE_TRANSPOSE_WORLD_VIEW_PROJECTION1,
        CODE_TRANSPOSE_WORLD_VIEW_PROJECTION2,
    ];
    if let Some(projection) = clip_from_view {
        sources.set_constant_rows(
            CODE_TRANSPOSE_PROJECTION,
            &code_transpose_matrix_row4(projection),
        );
    }
    if let Some(outdoor) = outdoor {
        if let Some(image) = outdoor.image {
            let _ = sources.set_texture_with_image(
                CODE_TEXTURE_OUTDOOR,
                CODE_TEXTURE_OUTDOOR_SAMPLER,
                image,
            );
        }
    }
    for (slot, cloud) in clouds.iter().enumerate() {
        let world_from_local = particle_cloud_world_from_local(cloud);
        sources.set_constant_rows(
            world_indices[slot],
            &code_transpose_matrix_row4(Mat4::from_translation(-view_origin) * world_from_local),
        );
        if let Some(view) = view_from_world {
            sources.set_constant_rows(
                world_view_indices[slot],
                &code_transpose_matrix_row4(view * world_from_local),
            );
        }
        if let Some(clip) = clip_from_world {
            sources.set_constant_rows(
                wvp_indices[slot],
                &code_transpose_matrix_row4(clip * world_from_local),
            );
        }
        let matrix = fx_iw4::fx_particle_cloud_matrix_diag(cloud.size0, cloud.size1);
        sources.set_constant_rows(
            u16::from(fx_iw4::FX_CODE_PARTICLE_CLOUD_MATRIX0) + slot as u16,
            &[float4_bits(matrix)],
        );
        let color = fx_iw4::fx_particle_cloud_color_const(cloud.color);
        sources.set_constant_rows(
            u16::from(fx_iw4::FX_CODE_SPARK_COLOR0) + slot as u16,
            &[float4_bits(color)],
        );
        if slot == 0 {
            sources.set_constant_rows(
                u16::from(fx_iw4::FX_CODE_PARTICLE_CLOUD_COLOR),
                &[float4_bits(color)],
            );
            sources.set_constant_rows(
                u16::from(fx_iw4::FX_CODE_FOUNTAIN_PARM0),
                &[float4_bits(fx_iw4::fx_particle_fountain_parm0(cloud.scale))],
            );
            sources.set_constant_rows(
                u16::from(fx_iw4::FX_CODE_FOUNTAIN_PARM1),
                &[float4_bits([0.0, 0.0, -1.0, view_origin.z])],
            );
            if let Some(outdoor) = outdoor {
                let lookup = d3d_row_major_bits_to_mat4(outdoor.lookup);

                let derived = lookup * world_from_local;
                sources.set_constant_rows(
                    CODE_TRANSPOSE_WORLD_OUTDOOR_LOOKUP,
                    &code_transpose_matrix_row4(derived)[..3],
                );
            }
        }
    }
}

fn d3d_row_major_bits_to_mat4(m: [u32; 16]) -> Mat4 {
    Mat4::from_cols_array(&m.map(f32::from_bits))
}

fn particle_cloud_world_from_local(cloud: &fx_iw4::GfxParticleCloud) -> Mat4 {
    Mat4::from_scale_rotation_translation(
        Vec3::splat(cloud.placement_scale),
        Quat::from_xyzw(cloud.quat[0], cloud.quat[1], cloud.quat[2], cloud.quat[3]),
        Vec3::from_array(cloud.pos),
    )
}

pub fn overlay_code_mesh_tess_code_constants(sources: &mut RuntimeCodeSources, args: &[[f32; 4]]) {
    for (i, row) in args.iter().take(2).enumerate() {
        sources.set_constant_rows(CODE_MESH_ARG_0 + i as u16, &[float4_bits(*row)]);
    }
}

fn overlay_shadow_lookup(sources: &mut RuntimeCodeSources, lookup: &[f32; 16]) {
    sources.set_constant_rows(
        CODE_SHADOWMAP_LOOKUP_TRANSPOSE,
        &[
            float4_bits([lookup[0], lookup[1], lookup[2], lookup[3]]),
            float4_bits([lookup[4], lookup[5], lookup[6], lookup[7]]),
            float4_bits([lookup[8], lookup[9], lookup[10], lookup[11]]),
            float4_bits([lookup[12], lookup[13], lookup[14], lookup[15]]),
        ],
    );
}

pub fn apply_shadowable_light(
    sources: &mut RuntimeCodeSources,
    packed_drawsurf: u64,
    lights: &[lighting_iw4::GfxLightPack],
    attenuation: &[LightAttenuationBind],
    t5_falloff: &[T5LightFalloffPack],
    eye: Vec3,
    float_time: f32,
    spot_receivers: &[Option<render_frame::SpotShadowReceiver>],
) {
    let index = dpvs_iw4::GfxDrawSurf {
        packed: packed_drawsurf,
    }
    .scene_light_index();
    let packed = lighting_iw4::pack_shadowable_light(
        index,
        lights.get(usize::from(index)),
        [eye.x, eye.y, eye.z],
        lighting_iw4::R_COLOR_SCALE_DEFAULT,
        lighting_iw4::R_COLOR_SCALE_DEFAULT,
    );
    match packed {
        lighting_iw4::ShadowableLightPack::Unchanged => {}
        lighting_iw4::ShadowableLightPack::Dir {
            position,
            diffuse,
            specular,
        } => {
            sources.set_constant_rows(CODE_LIGHT_POSITION, &[float4_bits(position)]);
            sources.set_constant_rows(CODE_LIGHT_DIFFUSE, &[float4_bits(diffuse)]);
            sources.set_constant_rows(CODE_LIGHT_SPECULAR, &[float4_bits(specular)]);
        }
        lighting_iw4::ShadowableLightPack::Omni {
            position,
            diffuse,
            specular,
            spot_dir,
            falloff_placement,
        } => {
            sources.set_constant_rows(CODE_LIGHT_POSITION, &[float4_bits(position)]);
            sources.set_constant_rows(CODE_LIGHT_DIFFUSE, &[float4_bits(diffuse)]);
            sources.set_constant_rows(CODE_LIGHT_SPECULAR, &[float4_bits(specular)]);
            sources.set_constant_rows(CODE_LIGHT_SPOTDIR, &[float4_bits(spot_dir)]);
            overlay_falloff_placement(sources, falloff_placement);
            overlay_attenuation(sources, attenuation.get(usize::from(index)));
        }
        lighting_iw4::ShadowableLightPack::Spot {
            position,
            diffuse,
            specular,
            spot_dir,
            spot_factors,
            falloff_placement,
        } => {
            sources.set_constant_rows(CODE_LIGHT_POSITION, &[float4_bits(position)]);
            sources.set_constant_rows(CODE_LIGHT_DIFFUSE, &[float4_bits(diffuse)]);
            sources.set_constant_rows(CODE_LIGHT_SPECULAR, &[float4_bits(specular)]);
            sources.set_constant_rows(CODE_LIGHT_SPOTDIR, &[float4_bits(spot_dir)]);
            sources.set_constant_rows(CODE_LIGHT_SPOTFACTORS, &[float4_bits(spot_factors)]);
            overlay_falloff_placement(sources, falloff_placement);
            overlay_attenuation(sources, attenuation.get(usize::from(index)));
        }
    }

    if matches!(
        packed,
        lighting_iw4::ShadowableLightPack::Omni { .. }
            | lighting_iw4::ShadowableLightPack::Spot { .. }
    ) && let Some(receiver) = spot_receivers.get(usize::from(index)).copied().flatten()
    {
        overlay_shadow_lookup(sources, &receiver.lookup);
        sources.set_constant_rows(
            CODE_SPOT_SHADOWMAP_PIXEL_ADJUST,
            &[float4_bits(receiver.pixel_adjust)],
        );
    }
    if !matches!(packed, lighting_iw4::ShadowableLightPack::Unchanged) {
        overlay_t5_shadowable_light(
            sources,
            lights.get(usize::from(index)),
            t5_falloff.get(usize::from(index)),
            eye,
            float_time,
        );
    }
}

fn overlay_t5_shadowable_light(
    sources: &mut RuntimeCodeSources,
    light: Option<&lighting_iw4::GfxLightPack>,
    pack: Option<&T5LightFalloffPack>,
    eye: Vec3,
    float_time: f32,
) {
    for (index, value) in [
        (CODE_LIGHT_DIFFUSE, pack.and_then(|p| p.diffuse)),
        (CODE_LIGHT_SPECULAR, pack.and_then(|p| p.specular)),
    ] {
        if let Some(v) = value {
            sources.set_constant_rows(index, &[float4_bits([v[0], v[1], v[2], 1.0])]);
        }
    }
    let omni_or_spot = light.is_some_and(|light| {
        light.light_type == lighting_iw4::GFX_LIGHT_TYPE_OMNI
            || light.light_type == lighting_iw4::GFX_LIGHT_TYPE_SPOT
    });
    if let Some(attenuation) = pack.and_then(|pack| pack.attenuation) {
        sources.set_constant_rows(
            CODE_LEFTOVER_T5_LIGHT_ATTENUATION,
            &[float4_bits(t5_light_attenuation_row(attenuation))],
        );
    } else if omni_or_spot {
        sources.set_constant_rows(
            CODE_LEFTOVER_T5_LIGHT_ATTENUATION,
            &[float4_bits(t5_light_attenuation_row(
                T5_LIGHT_ATTENUATION_DEFAULT,
            ))],
        );
    }
    let Some(light) = light else {
        return;
    };
    if light.light_type != lighting_iw4::GFX_LIGHT_TYPE_OMNI
        && light.light_type != lighting_iw4::GFX_LIGHT_TYPE_SPOT
    {
        return;
    }
    let falloff = pack.and_then(|pack| pack.falloff).unwrap_or([
        T5_LIGHT_FALLOFF_NEAR,
        light.radius,
        0.0,
        0.0,
    ]);
    let edges = t5_light_falloff_edges(falloff);
    if light.light_type == lighting_iw4::GFX_LIGHT_TYPE_OMNI {
        sources.set_constant_rows(
            CODE_LEFTOVER_T5_LIGHT_FALLOFF_A,
            &[float4_bits([
                edges.s_add,
                edges.e_add,
                edges.e_mul,
                edges.rs,
            ])],
        );
        return;
    }
    let a_ab_b = pack
        .and_then(|pack| pack.a_ab_b)
        .unwrap_or(T5_LIGHT_AABB_DEFAULT);
    if let Some((falloff_a, falloff_b)) = t5_spot_falloff_rows(edges, a_ab_b) {
        sources.set_constant_rows(CODE_LEFTOVER_T5_LIGHT_FALLOFF_A, &[float4_bits(falloff_a)]);
        sources.set_constant_rows(CODE_LEFTOVER_T5_LIGHT_FALLOFF_B, &[float4_bits(falloff_b)]);
    }
    sources.set_constant_rows(CODE_LEFTOVER_T5_LIGHT_SPOT_AABB, &[float4_bits(a_ab_b)]);
    sources.set_constant_rows(
        CODE_LEFTOVER_T5_LIGHT_CONE_CONTROL2,
        &[float4_bits(t5_spot_cone_control2(a_ab_b))],
    );
    let attenuation = pack
        .and_then(|pack| pack.attenuation)
        .unwrap_or(T5_LIGHT_ATTENUATION_DEFAULT);
    sources.set_constant_rows(
        CODE_LEFTOVER_T5_LIGHT_CONE_CONTROL1,
        &[float4_bits(t5_spot_cone_control1(attenuation))],
    );
    let (cookie0, cookie1, cookie2) = match pack {
        Some(pack)
            if pack.cookie0.is_some() && pack.cookie1.is_some() && pack.cookie2.is_some() =>
        {
            (
                pack.cookie0.unwrap(),
                pack.cookie1.unwrap(),
                pack.cookie2.unwrap(),
            )
        }
        _ => (
            T5_LIGHT_COOKIE_DEFAULT,
            T5_LIGHT_COOKIE_DEFAULT,
            T5_LIGHT_COOKIE_DEFAULT,
        ),
    };
    sources.set_constant_rows(
        CODE_LEFTOVER_T5_LIGHT_SPOT_COOKIE_SLIDE,
        &[float4_bits(t5_spot_cookie_slide(
            cookie0, cookie1, cookie2, float_time,
        ))],
    );
    let angle_z = pack
        .and_then(|pack| pack.angle_z)
        .unwrap_or(T5_LIGHT_SPOT_ROLL_DEFAULT);
    let columns = t5_spot_matrix_columns(
        light.direction,
        angle_z,
        light.cos_outer,
        falloff[0],
        falloff[1],
        [
            light.origin[0] - eye.x,
            light.origin[1] - eye.y,
            light.origin[2] - eye.z,
        ],
    );
    sources.set_constant_rows(
        CODE_LEFTOVER_T5_LIGHT_SPOT_MATRIX0,
        &[float4_bits(columns[0])],
    );
    sources.set_constant_rows(
        CODE_LEFTOVER_T5_LIGHT_SPOT_MATRIX1,
        &[float4_bits(columns[1])],
    );
    sources.set_constant_rows(
        CODE_LEFTOVER_T5_LIGHT_SPOT_MATRIX2,
        &[float4_bits(columns[2])],
    );
    sources.set_constant_rows(
        CODE_LEFTOVER_T5_LIGHT_SPOT_MATRIX3,
        &[float4_bits(columns[3])],
    );
}

fn t5_spot_cone_control1(attenuation: [f32; 4]) -> [f32; 4] {
    let v44 = if attenuation[3] == 0.0 {
        1.0
    } else {
        2.0 / attenuation[3]
    };
    [v44, -1.0 / v44, 1.0, attenuation[3]]
}

fn t5_spot_cone_control2(a_ab_b: [f32; 4]) -> [f32; 4] {
    [a_ab_b[0] * a_ab_b[2], a_ab_b[1] * a_ab_b[3], -2.0, 3.0]
}

fn t5_spot_cookie_slide(
    cookie0: [f32; 4],
    cookie1: [f32; 4],
    cookie2: [f32; 4],
    float_time: f32,
) -> [f32; 4] {
    let rc = cookie2[3] * float_time + cookie2[2];
    let (v63, v9) = rc.sin_cos();
    let sy = (cookie2[0] - v9 * cookie2[0]) + v63 * cookie2[1];
    let mx00 = (cookie2[1] - v63 * cookie2[0]) - v9 * cookie2[1];
    let mx10 = cookie1[2] * float_time + cookie1[0];
    let mx20 = cookie1[3] * float_time + cookie1[0];
    let mut mx01 = v9 * cookie0[2];
    let mut mx11 = (-v63) * cookie0[2];
    let mx21 = sy * cookie0[2] + mx10;
    let mut v53 = v63 * cookie0[3];
    let mut v52 = v9 * cookie0[3];
    let mut a_a_mul = mx00 * cookie0[3] + mx20;
    v53 = mx01 * cookie0[1] + v53;
    v52 = mx11 * cookie0[1] + v52;
    a_a_mul = mx21 * cookie0[1] + a_a_mul;
    mx01 = v53 * cookie0[0] + mx01;
    mx11 = v52 * cookie0[0] + mx11;
    let _mx21 = a_a_mul * cookie0[0] + mx21;
    [mx01 * 0.5, mx11 * 0.5, v53 * 0.5, v52 * 0.5]
}

fn t5_spot_matrix_columns(
    direction: [f32; 3],
    rotation: f32,
    cos_fov: f32,
    z_near: f32,
    z_far: f32,
    relative_origin: [f32; 3],
) -> [[f32; 4]; 4] {
    let mut view = spot_light_view_matrix(direction, rotation);
    view[3][0] = -(relative_origin[0] * view[0][0]
        + relative_origin[1] * view[1][0]
        + relative_origin[2] * view[2][0]);
    view[3][1] = -(relative_origin[0] * view[0][1]
        + relative_origin[1] * view[1][1]
        + relative_origin[2] * view[2][1]);
    view[3][2] = -(relative_origin[0] * view[0][2]
        + relative_origin[1] * view[1][2]
        + relative_origin[2] * view[2][2]);
    let proj = spot_light_projection_matrix(cos_fov, z_near, z_far);
    let product = mat4_mul_row_major(view, proj);
    [
        [product[0][0], product[1][0], product[2][0], product[3][0]],
        [product[0][1], product[1][1], product[2][1], product[3][1]],
        [product[0][2], product[1][2], product[2][2], product[3][2]],
        [product[0][3], product[1][3], product[2][3], product[3][3]],
    ]
}

fn spot_light_projection_matrix(cos_fov: f32, z_near: f32, z_far: f32) -> [[f32; 4]; 4] {
    let mut matrix = [[0.0f32; 4]; 4];
    let near = if z_near >= 0.001 { z_near } else { 0.001 };
    let q = z_far / (z_far - near);
    let cotan = 1.0 / ((1.0 - cos_fov * cos_fov).sqrt() / cos_fov);
    matrix[0][0] = cotan;
    matrix[1][1] = cotan;
    matrix[2][2] = q;
    matrix[2][3] = 1.0;
    matrix[3][2] = -q * near;
    matrix
}

fn spot_light_view_matrix(direction: [f32; 3], rotation: f32) -> [[f32; 4]; 4] {
    let forward = Vec3::new(-direction[0], -direction[1], -direction[2]).normalize_or_zero();
    let up = perpendicular_vector(forward);
    let right = up.cross(forward).normalize_or_zero();
    let up = up.normalize_or_zero();
    let (sin, cos) = rotation.sin_cos();
    let rotated_right = right * cos - up * sin;
    let rotated_up = right * sin + up * cos;
    [
        [rotated_right.x, rotated_up.x, forward.x, 0.0],
        [rotated_right.y, rotated_up.y, forward.y, 0.0],
        [rotated_right.z, rotated_up.z, forward.z, 0.0],
        [0.0, 0.0, 0.0, 1.0],
    ]
}

fn perpendicular_vector(src: Vec3) -> Vec3 {
    let src_sq = [src.x * src.x, src.y * src.y, src.z * src.z];
    let mut pos = usize::from(src_sq[0] > src_sq[1]);
    if src_sq[pos] > src_sq[2] {
        pos = 2;
    }
    let d = -src[pos];
    let mut dst = src * d;
    dst[pos] += 1.0;
    dst.normalize_or_zero()
}

fn mat4_mul_row_major(left: [[f32; 4]; 4], right: [[f32; 4]; 4]) -> [[f32; 4]; 4] {
    let mut out = [[0.0f32; 4]; 4];
    for row in 0..4 {
        for col in 0..4 {
            out[row][col] = left[row][0] * right[0][col]
                + left[row][1] * right[1][col]
                + left[row][2] * right[2][col]
                + left[row][3] * right[3][col];
        }
    }
    out
}

fn t5_light_attenuation_row(attenuation: [f32; 4]) -> [f32; 4] {
    [
        attenuation[0] + T5_LIGHT_ATTENUATION_EPSILON,
        attenuation[1],
        attenuation[2],
        attenuation[3],
    ]
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct T5FalloffEdges {
    s_add: f32,
    e_mul: f32,
    e_add: f32,
    rs: f32,
}

fn t5_light_falloff_edges(falloff: [f32; 4]) -> T5FalloffEdges {
    let far_edge = falloff[0];
    let v75 = falloff[1];
    let s_mul = falloff[2];
    let v73 = falloff[3];
    let (s_add, e_mul) = if far_edge == s_mul {
        (1.0, -far_edge)
    } else {
        let inv = 1.0 / (s_mul - far_edge);
        (inv, -far_edge * inv)
    };
    let (e_add, rs) = if v73 == v75 {
        (-1.0, v75)
    } else {
        let inv = 1.0 / (v73 - v75);
        (inv, -v75 * inv)
    };
    T5FalloffEdges {
        s_add,
        e_mul,
        e_add,
        rs,
    }
}

fn t5_spot_falloff_rows(edges: T5FalloffEdges, a_ab_b: [f32; 4]) -> Option<([f32; 4], [f32; 4])> {
    let den0 = a_ab_b[0] - a_ab_b[1];
    let den1 = a_ab_b[2] - a_ab_b[3];
    if den0 == 0.0 || den1 == 0.0 {
        return None;
    }
    let bb_add = 1.0 / den0;
    let v47 = -a_ab_b[1] * bb_add;
    let re = 1.0 / den1;
    let v45 = -a_ab_b[3] * re;
    Some((
        [edges.s_add, re, bb_add, edges.e_add],
        [edges.e_mul, v45, v47, edges.rs],
    ))
}

fn overlay_falloff_placement(sources: &mut RuntimeCodeSources, row: Option<[f32; 4]>) {
    let Some(row) = row else {
        return;
    };
    sources.set_constant_rows(CODE_LIGHT_FALLOFF_PLACEMENT, &[float4_bits(row)]);
}

fn overlay_attenuation(sources: &mut RuntimeCodeSources, bind: Option<&LightAttenuationBind>) {
    let Some(bind) = bind else {
        return;
    };
    let Some(id) = bind.image else {
        return;
    };
    let _ = sources.set_texture_with_image(
        CODE_TEXTURE_LIGHT_ATTENUATION,
        bind.sampler,
        RuntimeImageId(id),
    );
}

pub fn produce_depth_from_clip(sources: &mut RuntimeCodeSources, viewmodel: bool) {
    let w = if viewmodel { -1.0 } else { 1.0 };
    sources.set_constant_rows(CODE_DEPTH_FROM_CLIP, &[float4_bits([0.0, 0.0, 0.0, w])]);
}

pub fn overlay_viewmodel_depth_hack(
    sources: &mut RuntimeCodeSources,
    viewmodel_clip_from_world: Mat4,
    view_origin: Vec3,
) {
    produce_depth_from_clip(sources, true);
    sources.set_constant_rows(
        CODE_TRANSPOSE_VIEW_PROJECTION,
        &code_transpose_matrix_row4(camera_relative_view_projection(
            viewmodel_clip_from_world,
            view_origin,
        )),
    );
    sources.set_constant_rows(
        CODE_TRANSPOSE_WORLD_VIEW_PROJECTION0,
        &code_transpose_matrix_row4(viewmodel_clip_from_world),
    );
}
