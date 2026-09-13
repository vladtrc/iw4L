use crate::trail::fx_pack_code_mesh_vertex;
use crate::vec::{fx_perpendicular_vector, fx_vec3_length_sq, fx_vec3_normalize};

pub const FX_POST_LIGHT_DRAW_NAME: &str = "PostLight";

pub const FX_POST_LIGHT_ADD_CAP: usize = 0x60;

pub const FX_POST_LIGHT_STRIDE: usize = 0x24;

pub const FX_POST_LIGHT_VERT_COUNT: u32 = 16;
pub const FX_POST_LIGHT_INDEX_COUNT: u32 = 0x54;
pub const FX_POST_LIGHT_ARG_COUNT: u32 = 2;

pub const FX_POST_LIGHT_MIN_DELTA_SQ: f32 = 1.0e-4;

pub const FX_POST_LIGHT_POLYGON_RADIUS_GROW: f32 = 1.4142135;

pub const FX_POST_LIGHT_ANGLE_STEP: f32 = core::f32::consts::PI * 0.125;

#[derive(Clone, Copy, Debug)]
pub struct FxPostLight {
    pub begin: [f32; 3],
    pub end: [f32; 3],
    pub radius: f32,
    pub color_packed: u32,
    pub material_name: &'static str,
}

#[inline]
pub fn fx_post_light_add_allows(live: u32) -> bool {
    live != FX_POST_LIGHT_ADD_CAP as u32
}

#[derive(Clone, Copy, Debug)]
pub struct FxPostLightTess {
    pub verts: [[f32; 3]; 16],
    pub color_packed: u32,
    pub indices: [u16; 84],
    pub args: [[f32; 4]; 2],
}

#[inline]
pub fn fx_post_light_pack_vert(xyz: [f32; 3], color_packed: u32) -> [u8; 32] {
    let [b, g, r, a] = color_packed.to_le_bytes();
    fx_pack_code_mesh_vertex(xyz, [r, g, b, a], 0, 0, 0)
}

pub fn fx_post_light_generate_verts(light: &FxPostLight, eye: [f32; 3]) -> Option<FxPostLightTess> {
    let delta = [
        light.end[0] - light.begin[0],
        light.end[1] - light.begin[1],
        light.end[2] - light.begin[2],
    ];
    let len_sq = fx_vec3_length_sq(delta);
    if len_sq < FX_POST_LIGHT_MIN_DELTA_SQ {
        return None;
    }
    let inv_radius = 1.0 / light.radius;
    let inv_len_sq = 1.0 / len_sq;
    let args = [
        [
            (light.begin[0] - eye[0]) * inv_radius,
            (light.begin[1] - eye[1]) * inv_radius,
            (light.begin[2] - eye[2]) * inv_radius,
            inv_radius,
        ],
        [
            delta[0] * inv_len_sq,
            delta[1] * inv_len_sq,
            delta[2] * inv_len_sq,
            inv_len_sq,
        ],
    ];
    let dir = fx_vec3_normalize(delta);
    let ortho0 = fx_perpendicular_vector(dir);
    let ortho1 = [
        dir[1] * ortho0[2] - dir[2] * ortho0[1],
        dir[2] * ortho0[0] - dir[0] * ortho0[2],
        dir[0] * ortho0[1] - dir[1] * ortho0[0],
    ];
    let grow = light.radius * FX_POST_LIGHT_POLYGON_RADIUS_GROW;
    let mut verts = [[0.0f32; 3]; 16];
    for i in 0..8 {
        let ang = (i + i) as f32 * FX_POST_LIGHT_ANGLE_STEP;
        let c = libm::cosf(ang);
        let s = libm::sinf(ang);
        let ox = (s * ortho0[0] + c * ortho1[0]) * grow;
        let oy = (s * ortho0[1] + c * ortho1[1]) * grow;
        let oz = (s * ortho0[2] + c * ortho1[2]) * grow;
        let rx = dir[0] * light.radius;
        let ry = dir[1] * light.radius;
        let rz = dir[2] * light.radius;
        verts[i] = [
            light.begin[0] + ox - rx,
            light.begin[1] + oy - ry,
            light.begin[2] + oz - rz,
        ];
        verts[i + 8] = [
            light.end[0] + ox + rx,
            light.end[1] + oy + ry,
            light.end[2] + oz + rz,
        ];
    }
    Some(FxPostLightTess {
        verts,
        color_packed: light.color_packed,
        indices: fx_post_light_indices(),
        args,
    })
}

fn fx_post_light_indices() -> [u16; 84] {
    let mut out = [0u16; 84];
    let mut w = 0usize;
    for around in 0..8u16 {
        let next = (around + 1) & 7;
        out[w] = around;
        out[w + 1] = next;
        out[w + 2] = around + 8;
        out[w + 3] = around + 8;
        out[w + 4] = next;
        out[w + 5] = next + 8;
        w += 6;
    }
    for around in (0..6u16).step_by(2) {
        out[w] = around + 2;
        out[w + 1] = around + 1;
        out[w + 2] = around + 3;
        out[w + 3] = 0;
        out[w + 4] = around + 2;
        out[w + 5] = 0;
        w += 6;
    }
    for around in (0..6u16).step_by(2) {
        out[w] = 8;
        out[w + 1] = around + 9;
        out[w + 2] = around + 10;
        out[w + 3] = 8;
        out[w + 4] = around + 10;
        out[w + 5] = around + 11;
        w += 6;
    }
    debug_assert_eq!(w, 84);
    out
}
