use bevy::prelude::*;
use render_backend::overlay::CODE_TRANSPOSE_WORLD_VIEW_PROJECTION0;
use render_material::{CodeSourceError, RuntimeCodeSources};

const FILM_DESAT_MIN: f32 = 1.0 / 4096.0;
pub const POSTFX_MATERIALS: &[&str] = &[
    "postfx_color2",
    "dof_downsample",
    "dof_near_coc",
    "small_blur",
    "postfx_dof_color2",
    "filter_symmetric_1",
    "filter_symmetric_2",
    "filter_symmetric_3",
    "filter_symmetric_4",
    "filter_symmetric_5",
    "filter_symmetric_6",
    "filter_symmetric_7",
    "filter_symmetric_8",
];
pub const POSTFX_VERTEX_TYPE: u8 = 0;
pub const GLOW_SETUP_MATERIAL: &str = "glow_consistent_setup_color2";
pub const GLOW_APPLY_MATERIAL: &str = "glow_apply_bloom";
const CODE_TEXTURE_RESOLVED_SCENE: u32 = 10;
const CODE_TEXTURE_FEEDBACK: u32 = 8;
const RESOLVED_SCENE_SAMPLER: u8 = 0x62;
const CODE_COLOR_BIAS: u16 = 46;
const CODE_COLOR_TINT_BASE: u16 = 47;
const CODE_COLOR_TINT_DELTA: u16 = 48;
const CODE_COLOR_TINT_QUADRATIC_DELTA: u16 = 49;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PostFxSourceRefusal {
    InvalidDimensions { width: u32, height: u32 },
    CodeSource(CodeSourceError),
}

fn float4_bits(row: [f32; 4]) -> [u32; 4] {
    row.map(f32::to_bits)
}

fn code_transpose_matrix_rows(m: Mat4) -> Vec<[u32; 4]> {
    let cols = m.to_cols_array_2d();
    [0usize, 1, 2, 3]
        .map(|row| float4_bits([cols[0][row], cols[1][row], cols[2][row], cols[3][row]]))
        .to_vec()
}

pub fn hud_2d_sources(width: f32, height: f32) -> Option<RuntimeCodeSources> {
    let projection = hud_iw4::r_cmd_buf_set_2d_projection(width as i32, height as i32)?;
    let mut sources = RuntimeCodeSources::default();
    sources.set_constant(
        CODE_TRANSPOSE_WORLD_VIEW_PROJECTION0,
        code_transpose_matrix_rows(Mat4::from_cols_array(&projection)),
    );
    Some(sources)
}

pub fn film_sources(
    width: u32,
    height: u32,
    vision: Option<assets::FilmVision>,
) -> Result<RuntimeCodeSources, PostFxSourceRefusal> {
    let width_i32 = i32::try_from(width)
        .map_err(|_| PostFxSourceRefusal::InvalidDimensions { width, height })?;
    let height_i32 = i32::try_from(height)
        .map_err(|_| PostFxSourceRefusal::InvalidDimensions { width, height })?;
    let mut projection = hud_iw4::r_cmd_buf_set_2d_projection(width_i32, height_i32)
        .ok_or(PostFxSourceRefusal::InvalidDimensions { width, height })?;
    // Fullscreen quads must cover wgpu's half-integer pixel centers. The D3D9
    // projection shifts the right/bottom edges onto the last pixel centers,
    // leaving clear texels that subsequent blur passes spread into the image.
    projection[12] = -1.0;
    projection[13] = 1.0;
    let mut sources = RuntimeCodeSources::default();
    sources.set_constant(
        CODE_TRANSPOSE_WORLD_VIEW_PROJECTION0,
        code_transpose_matrix_rows(Mat4::from_cols_array(&projection)),
    );
    let authored = vision.unwrap_or_default();
    let vision = if authored.enable {
        authored
    } else {
        assets::FilmVision::default()
    };
    let desaturation = vision.desaturation.max(FILM_DESAT_MIN);
    let mut contrast = vision.contrast;
    let mut offset = vision.brightness + 0.5 - 0.5 * contrast;
    if vision.invert {
        contrast = -contrast;
        offset += 1.0;
    }
    let base = vision.dark_tint.map(|value| value * contrast);
    let delta: [f32; 3] = std::array::from_fn(|channel| {
        2.0 * contrast * (vision.medium_tint[channel] - vision.dark_tint[channel])
    });
    let quadratic: [f32; 3] = std::array::from_fn(|channel| {
        contrast
            * (vision.light_tint[channel] - 2.0 * vision.medium_tint[channel]
                + vision.dark_tint[channel])
    });
    sources.set_constant_rows(
        CODE_COLOR_BIAS,
        &[float4_bits([
            offset,
            offset,
            offset,
            1.0 / desaturation - 1.0,
        ])],
    );
    sources.set_constant_rows(
        CODE_COLOR_TINT_BASE,
        &[float4_bits([
            base[0],
            base[1],
            base[2],
            vision.desaturation_dark,
        ])],
    );
    sources.set_constant_rows(
        CODE_COLOR_TINT_DELTA,
        &[float4_bits([
            delta[0],
            delta[1],
            delta[2],
            vision.desaturation - vision.desaturation_dark,
        ])],
    );
    sources.set_constant_rows(
        CODE_COLOR_TINT_QUADRATIC_DELTA,
        &[float4_bits([quadratic[0], quadratic[1], quadratic[2], 0.0])],
    );
    apply_glow_consts(&mut sources, authored);
    sources
        .set_texture(CODE_TEXTURE_RESOLVED_SCENE, RESOLVED_SCENE_SAMPLER)
        .map_err(PostFxSourceRefusal::CodeSource)?;
    sources
        .set_texture(CODE_TEXTURE_FEEDBACK, RESOLVED_SCENE_SAMPLER)
        .map_err(PostFxSourceRefusal::CodeSource)?;
    Ok(sources)
}

fn apply_glow_consts(sources: &mut RuntimeCodeSources, authored: assets::FilmVision) {
    let bits = |row: [f32; 4]| row.map(f32::to_bits);
    match lighting_iw4::r_set_glow_info(
        authored.glow_bloom_cutoff,
        authored.glow_bloom_desaturation,
        authored.glow_bloom_intensity,
    ) {
        Some(consts) => {
            sources.set_constant_rows(
                lighting_iw4::CONST_SRC_CODE_GLOW_SETUP,
                &[bits(consts.setup)],
            );
            sources.set_constant_rows(
                lighting_iw4::CONST_SRC_CODE_GLOW_APPLY,
                &[bits(consts.apply)],
            );
        }
        None => {
            sources.set_constant_rows(lighting_iw4::CONST_SRC_CODE_GLOW_SETUP, &[[0; 4]]);
            sources.set_constant_rows(lighting_iw4::CONST_SRC_CODE_GLOW_APPLY, &[[0; 4]]);
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct DepthOfField {
    pub view_model_start: f32,
    pub view_model_end: f32,
    pub near_start: f32,
    pub near_end: f32,
    pub far_start: f32,
    pub far_end: f32,
    pub near_blur: f32,
    pub far_blur: f32,
}
impl DepthOfField {
    pub fn active(self) -> bool {
        self.view_model_end > self.view_model_start + 1.0
            || self.near_end > self.near_start + 1.0
            || (self.far_end > self.far_start + 1.0 && self.far_blur > 0.0)
    }
}

#[derive(Resource, Clone, Copy, Debug, Default)]
pub struct DofFrame {
    pub dof: DepthOfField,
    pub bias: f32,
    pub scene_near: f32,
    pub view_model_near: f32,
    pub glow: GlowFrame,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct GlowFrame {
    pub enable: bool,
    pub radius: f32,
    pub intensity: f32,

    pub r_glow: bool,

    pub r_fullbright: bool,
}

impl Default for GlowFrame {
    fn default() -> Self {
        Self {
            enable: false,
            radius: 0.0,
            intensity: 0.0,
            r_glow: lighting_iw4::R_GLOW_DEFAULT,
            r_fullbright: lighting_iw4::R_FULLBRIGHT_DEFAULT,
        }
    }
}

impl GlowFrame {
    pub fn from_vision(vision: Option<assets::FilmVision>) -> Self {
        let vision = vision.unwrap_or_default();
        Self {
            enable: vision.glow_enable,
            radius: vision.glow_radius,
            intensity: vision.glow_bloom_intensity,
            r_glow: lighting_iw4::R_GLOW_DEFAULT,
            r_fullbright: lighting_iw4::R_FULLBRIGHT_DEFAULT,
        }
    }

    pub fn using(self) -> bool {
        lighting_iw4::r_using_glow(
            self.enable,
            self.intensity,
            self.radius,
            self.r_glow,
            self.r_fullbright,
        )
    }
}

fn near_equation(mut start: f32, mut end: f32, near: f32) -> [f32; 2] {
    if start.max(near) >= end {
        start = 0.0;
        end = near * 0.5;
    }
    [1.0 / (start - end), end / (end - start)]
}

#[derive(Clone, Debug, PartialEq)]
pub enum DofPlanRefusal {
    InvalidDimensions,
    InvalidFocus,
    BlurFractions { small: f32, medium: f32 },
    CodeSource(PostFxSourceRefusal),
}

pub fn sources(
    width: u32,
    height: u32,
    scene_height: u32,
    vision: Option<assets::FilmVision>,
    frame: DofFrame,
) -> Result<RuntimeCodeSources, DofPlanRefusal> {
    if width == 0 || height == 0 || scene_height == 0 {
        return Err(DofPlanRefusal::InvalidDimensions);
    }
    let d = frame.dof;
    if d.near_blur < 4.0 || frame.scene_near <= 0.0 || frame.view_model_near <= 0.0 {
        return Err(DofPlanRefusal::InvalidFocus);
    }
    let mut s = film_sources(width, height, vision).map_err(DofPlanRefusal::CodeSource)?;
    let near = near_equation(d.near_start, d.near_end, frame.scene_near);
    let far = if d.far_start.max(frame.scene_near) < d.far_end {
        [
            1.0 / (d.far_end - d.far_start),
            d.far_start / (d.far_start - d.far_end),
        ]
    } else {
        [0.0; 2]
    };
    let vm = near_equation(d.view_model_start, d.view_model_end, frame.view_model_near);
    let fraction =
        |radius: f64| ((radius / scene_height as f64) as f32 / d.near_blur).powf(frame.bias);
    let small = fraction(671.9999885559082);
    let medium = fraction(1727.9999542236328);
    if !(0.0 < small && small < medium && medium < 1.0) {
        return Err(DofPlanRefusal::BlurFractions { small, medium });
    }
    for (index, row) in [
        (
            22,
            [
                width as f32,
                height as f32,
                1.0 / width as f32,
                1.0 / height as f32,
            ],
        ),
        (
            23,
            [
                vm[0],
                0.0,
                vm[1],
                (d.far_blur / d.near_blur).powf(frame.bias),
            ],
        ),
        (24, [near[0], far[0], near[1], far[1]]),
        (
            25,
            [
                -1.0 / small,
                -1.0 / (medium - small),
                -1.0 / (1.0 - medium),
                1.0 / (1.0 - medium),
            ],
        ),
        (
            26,
            [
                1.0,
                medium / (medium - small),
                1.0 / (1.0 - medium),
                -medium / (1.0 - medium),
            ],
        ),
        (27, [0.0, 1.0 / scene_height as f32, 0.0, 0.0]),
    ] {
        s.set_constant_rows(index, &[row.map(f32::to_bits)]);
    }
    for index in [8, 11, 12, 15] {
        s.set_texture(index, if index == 15 { 0x61 } else { 0x62 })
            .map_err(|e| DofPlanRefusal::CodeSource(PostFxSourceRefusal::CodeSource(e)))?;
    }
    Ok(s)
}

#[derive(Clone, Debug)]
pub struct GaussianPass {
    pub half_taps: usize,
    pub taps: [[f32; 4]; 8],
}

fn gaussian_points(radius: f32, size: u32, limit: usize) -> (usize, [[f32; 2]; 8]) {
    let exponent = -0.5 / (radius * radius);
    let mut taps = [[0.0; 2]; 8];
    let mut total = 0.0;
    for (i, tap) in taps.iter_mut().enumerate().take(limit) {
        let a = (2 * i) as f32;
        let b = a + 1.0;
        let wa = (a * (a * exponent)).exp() * if i == 0 { 0.5 } else { 1.0 };
        let wb = (b * (b * exponent)).exp();
        let w = wa + wb;
        *tap = [
            if w == 0.0 {
                (a + b) * 0.5 / size as f32
            } else {
                (a * wa + b * wb) / (w * size as f32)
            },
            w,
        ];
        total += w;
    }
    let mut count = limit;
    if total > 0.001 {
        for i in (0..limit).rev() {
            taps[i][1] *= 0.5 / total;
            if taps[i][1] < 0.01 {
                count = i + 1;
            }
        }
    } else {
        taps[0][1] = 0.5;
        count = 1;
    }
    (count, taps)
}

pub fn gaussian_chain(radius: f32, width: u32, height: u32) -> Vec<GaussianPass> {
    const MIN: f32 = 0.32950512;
    const MAX_2D: f32 = 1.3895605;
    const MAX_1D: f32 = 6.4977503;
    let mut radii = [radius; 2];
    let mut passes = Vec::new();
    while (radii[0] >= MIN || radii[1] >= MIN) && passes.len() < 32 {
        if (radii[0] - radii[1]).abs() < MIN && (radii[0] + radii[1]) * 0.5 <= MAX_2D {
            let r = (radii[0] + radii[1]) * 0.5;
            let (_, x) = gaussian_points(r, width, 2);
            let (_, y) = gaussian_points(r, height, 2);
            let mut taps = [[0.0; 4]; 8];
            for j in 0..2 {
                for i in 0..2 {
                    let k = j * 4 + i * 2;
                    taps[k] = [-x[i][0], y[j][0], 0.0, x[i][1] * y[j][1]];
                    taps[k + 1] = [x[i][0], y[j][0], 0.0, x[i][1] * y[j][1]];
                }
            }
            passes.push(GaussianPass { half_taps: 8, taps });
            break;
        }
        let axis = usize::from(radii[0] <= radii[1]);
        let r = radii[axis];
        let pass_radius = if r >= MAX_1D { MAX_1D } else { r };
        radii[axis] = if r >= MAX_1D {
            (r * r - 42.22076).sqrt()
        } else {
            0.0
        };
        let (half_taps, points) = gaussian_points(pass_radius, [width, height][axis], 8);
        let mut taps = [[0.0; 4]; 8];
        for i in 0..8 {
            taps[i][axis] = points[i][0];
            taps[i][3] = points[i][1];
        }
        passes.push(GaussianPass { half_taps, taps });
    }
    passes
}
