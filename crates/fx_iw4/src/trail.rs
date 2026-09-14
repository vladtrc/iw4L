use crate::trail_def::FxTrailVertex;

pub const FX_TRAIL_BASIS_SCALE: f64 = 0.007_874_015_718_698_502;

pub const FX_TRAIL_NORMAL_SCALE: f64 = 127.0;
pub const FX_TRAIL_NORMAL_BIAS: f64 = 127.5;

pub const FX_TRAIL_TANGENT_PACKED: f32 = 0.998_046_76;

pub const FX_CODE_MESH_VERTEX_STRIDE: usize = 32;

pub const FX_CODE_MESH_BINORMAL_SIGN: f32 = -1.0;

#[inline]
pub fn fx_trail_compute_u(
    span: f32,
    repeat_dist: i32,
    scroll_time_msec: i32,
    draw_time_msec: i32,
) -> i32 {
    if scroll_time_msec != 0 {
        return draw_time_msec / scroll_time_msec;
    }
    if repeat_dist == 0 {
        return 0;
    }
    libm::floorf(span / (repeat_dist as f32)) as i32
}

#[inline]
pub fn fx_trail_uncompress_basis(basis_chars: &[i8; 6]) -> [[f32; 3]; 2] {
    let s = FX_TRAIL_BASIS_SCALE as f32;
    [
        [
            (basis_chars[0] as f32) * s,
            (basis_chars[1] as f32) * s,
            (basis_chars[2] as f32) * s,
        ],
        [
            (basis_chars[3] as f32) * s,
            (basis_chars[4] as f32) * s,
            (basis_chars[5] as f32) * s,
        ],
    ]
}

#[inline]
pub fn fx_trail_compress_char(v: f32) -> i8 {
    let i = (v as f64 * FX_TRAIL_NORMAL_SCALE) as i32;
    i.clamp(i32::from(i8::MIN), i32::from(i8::MAX)) as i8
}

#[inline]
pub fn fx_trail_compress_basis(left: [f32; 3], up: [f32; 3]) -> [i8; 6] {
    [
        fx_trail_compress_char(left[0]),
        fx_trail_compress_char(left[1]),
        fx_trail_compress_char(left[2]),
        fx_trail_compress_char(up[0]),
        fx_trail_compress_char(up[1]),
        fx_trail_compress_char(up[2]),
    ]
}

#[inline]
pub fn fx_compress_basis_from_axis(axis: [[f32; 3]; 3]) -> [i8; 6] {
    fx_trail_compress_basis(axis[1], axis[2])
}

#[inline]
pub fn fx_compress_basis_from_quat(quat: [f32; 4]) -> [i8; 6] {
    fx_compress_basis_from_axis(crate::glass::fx_unit_quat_to_axis(quat))
}

#[inline]
pub fn fx_trail_emit_index_quad(a: u16, b: u16, base0: u16, vert_count: u16) -> [[u16; 2]; 3] {
    let base1 = base0.wrapping_add(vert_count);
    let a0 = a.wrapping_add(base0);
    let b0 = b.wrapping_add(base0);
    let a1 = a.wrapping_add(base1);
    let b1 = b.wrapping_add(base1);
    [[a0, b0], [a1, b1], [a1, b0]]
}

#[inline]
pub fn fx_trail_index_quad_tris(pairs: [[u16; 2]; 3]) -> [[u16; 3]; 2] {
    let [[a0, b0], [a1, b1], _] = pairs;
    [[a0, b0, a1], [b0, b1, a1]]
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FxTrailSegmentDrawState {
    pub pos_world: [f32; 3],

    pub basis: [[f32; 3]; 2],
    pub rotation: f32,
    pub size: [f32; 2],
    pub u_coord: f32,
    pub color_rgba: [u8; 4],
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FxTrailEmittedVert {
    pub xyz: [f32; 3],
    pub color_rgba: [u8; 4],
    pub u: f32,
    pub v: f32,

    pub texcoord_packed: u32,

    pub normal_packed: u32,
    pub tangent_packed: f32,
}

#[inline]
pub fn fx_trail_pack_texcoord(u: f32, v: f32) -> u32 {
    let pack_one = |f: f32| -> u32 {
        let bits = f.to_bits() as i32;
        let mut q = (bits.wrapping_mul(2) ^ (0x8000_0000u32 as i32)) >> 14;
        if q >= 0x3fff {
            q = 0x3fff;
        } else if q < -0x3fff {
            q = 0;
        }
        (q as u32 & 0x3fff) | ((bits as u32 >> 16) & 0xc000)
    };
    pack_one(u).wrapping_mul(0x1_0000).wrapping_add(pack_one(v))
}

#[inline]
pub fn fx_trail_pack_normal(n: [f32; 3]) -> u32 {
    let enc = |c: f32| -> u8 {
        let v = (c as f64) * FX_TRAIL_NORMAL_SCALE + FX_TRAIL_NORMAL_BIAS;
        libm::round(v) as i32 as u8
    };
    u32::from_le_bytes([enc(n[0]), enc(n[1]), enc(n[2]), 0x3f])
}

#[inline]
pub fn fx_pack_code_mesh_vertex(
    xyz: [f32; 3],
    color_rgba: [u8; 4],
    texcoord_packed: u32,
    normal_packed: u32,
    tangent_packed: u32,
) -> [u8; FX_CODE_MESH_VERTEX_STRIDE] {
    fx_pack_code_mesh_vertex_signed(
        xyz,
        color_rgba,
        texcoord_packed,
        normal_packed,
        tangent_packed,
        FX_CODE_MESH_BINORMAL_SIGN,
    )
}

#[inline]
pub fn fx_pack_code_mesh_vertex_signed(
    xyz: [f32; 3],
    color_rgba: [u8; 4],
    texcoord_packed: u32,
    normal_packed: u32,
    tangent_packed: u32,
    binormal_sign: f32,
) -> [u8; FX_CODE_MESH_VERTEX_STRIDE] {
    let mut row = [0u8; FX_CODE_MESH_VERTEX_STRIDE];
    row[0..4].copy_from_slice(&xyz[0].to_le_bytes());
    row[4..8].copy_from_slice(&xyz[1].to_le_bytes());
    row[8..12].copy_from_slice(&xyz[2].to_le_bytes());
    row[12..16].copy_from_slice(&binormal_sign.to_le_bytes());
    let [r, g, b, a] = color_rgba;
    row[16] = b;
    row[17] = g;
    row[18] = r;
    row[19] = a;
    row[20..24].copy_from_slice(&texcoord_packed.to_le_bytes());
    row[24..28].copy_from_slice(&normal_packed.to_le_bytes());
    row[28..32].copy_from_slice(&tangent_packed.to_le_bytes());
    row
}

#[inline]
pub fn fx_trail_emit_segment_vert(
    trail_vert: &FxTrailVertex,
    state: &FxTrailSegmentDrawState,
) -> FxTrailEmittedVert {
    let cos_r = libm::cosf(state.rotation);
    let sin_r = libm::sinf(state.rotation);
    let b0 = state.basis[0];
    let b1 = state.basis[1];

    let left = [
        cos_r * b0[0] + sin_r * b1[0],
        cos_r * b0[1] + sin_r * b1[1],
        cos_r * b0[2] + sin_r * b1[2],
    ];

    let up = [
        sin_r * b0[0] + (-cos_r) * b1[0],
        sin_r * b0[1] + (-cos_r) * b1[1],
        sin_r * b0[2] + (-cos_r) * b1[2],
    ];
    let sx = trail_vert.pos[0] * state.size[0];
    let sy = trail_vert.pos[1] * state.size[1];
    let xyz = [
        sx * left[0] + state.pos_world[0] + sy * up[0],
        sx * left[1] + state.pos_world[1] + sy * up[1],
        sx * left[2] + state.pos_world[2] + sy * up[2],
    ];
    let nx = trail_vert.normal[0];
    let ny = trail_vert.normal[1];
    let n_world = [
        ny * up[0] + nx * left[0],
        ny * up[1] + nx * left[1],
        ny * up[2] + nx * left[2],
    ];
    FxTrailEmittedVert {
        xyz,
        color_rgba: state.color_rgba,
        u: state.u_coord,
        v: trail_vert.tex_coord,
        texcoord_packed: fx_trail_pack_texcoord(state.u_coord, trail_vert.tex_coord),
        normal_packed: fx_trail_pack_normal(n_world),
        tangent_packed: FX_TRAIL_TANGENT_PACKED,
    }
}

#[inline]
pub fn fx_trail_emit_segment_verts(
    verts: &[FxTrailVertex],
    state: &FxTrailSegmentDrawState,
    out: &mut [FxTrailEmittedVert],
) -> usize {
    let n = verts.len().min(out.len());
    for i in 0..n {
        out[i] = fx_trail_emit_segment_vert(&verts[i], state);
    }
    n
}
