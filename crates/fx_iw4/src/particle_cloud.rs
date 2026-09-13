use crate::integrate::fx_sparkcloud_history_lookback_ms;
use crate::quat::{fx_axis_to_quat, fx_quat_nlerp};
use crate::vec::fx_vec3_normalize;

pub const GFX_POS_TEX_VERTEX_STRIDE: usize = 0x14;

pub const GFX_PARTICLE_CLOUD_STRIDE: usize = 0x40;

pub const FX_SPARK_CLOUD_HISTORY_STRIDE: usize = 0x810;

pub const FX_SPARK_CLOUD_HISTORY_CAPACITY: u32 = 0x20;

pub const FX_SPARK_CLOUD_SAMPLE_RING: u32 = 0x20;
pub const FX_SPARK_CLOUD_SAMPLE_MASK: u32 = 0x1f;

pub const FX_SPARK_CLOUD_HANDLE_NONE: u16 = 0xffff;

pub const FX_PARTICLE_CLOUD_GRID_X: u32 = 8;
pub const FX_PARTICLE_CLOUD_GRID_Y: u32 = 8;
pub const FX_PARTICLE_CLOUD_GRID_Z: u32 = 16;
pub const FX_PARTICLE_CLOUD_TEMPLATE_CELLS: usize = 1024;

pub const FX_PARTICLE_CLOUD_VERTS_PER_CELL: u32 = 4;
pub const FX_PARTICLE_CLOUD_PRIMS_PER_CELL: u32 = 2;
pub const FX_PARTICLE_CLOUD_INDICES_PER_CELL: u32 = 6;
pub const FX_PARTICLE_SPARK_VERTS_PER_CELL: u32 = 8;
pub const FX_PARTICLE_SPARK_PRIMS_PER_CELL: u32 = 6;
pub const FX_PARTICLE_SPARK_INDICES_PER_CELL: u32 = 18;

pub const FX_PARTICLE_CLOUD_FLAG_SPARK: u32 = 4;

pub const FX_PARTICLE_CLOUD_VERT_DECL_TYPE: u8 = 0xe;

pub const FX_PARTICLE_CLOUD_PRIM_TYPE: u32 = 4;

pub const FX_SPARKCLOUD_UV_V_1_3: f32 = f32::from_le_bytes([0x4c, 0xa6, 0xaa, 0x3e]);
pub const FX_SPARKCLOUD_UV_V_2_3: f32 = f32::from_le_bytes([0xda, 0xac, 0x2a, 0x3f]);

pub const FX_SPARKCLOUD_HISTORY_MIN_DT_MS: f32 = 16.666;

pub const FX_PARTICLE_CLOUD_QUAD_INDICES: [u16; 6] = [0, 1, 2, 2, 1, 3];

pub const FX_PARTICLE_CLOUD_UV: [[f32; 2]; 4] = [[0.0, 0.0], [1.0, 0.0], [0.0, 1.0], [1.0, 1.0]];

pub const FX_PARTICLE_SPARK_INDICES: [u16; 18] =
    [0, 2, 1, 1, 2, 3, 2, 4, 3, 3, 4, 5, 4, 6, 5, 5, 6, 7];

pub const FX_CODE_PARTICLE_CLOUD_MATRIX0: u8 = 0x41;

pub const FX_CODE_SPARK_COLOR0: u8 = 0x44;

pub const FX_CODE_PARTICLE_CLOUD_COLOR: u8 = 0x06;

pub const FX_CODE_FOUNTAIN_PARM0: u8 = 0x47;

pub const FX_CODE_FOUNTAIN_PARM1: u8 = 0x48;

pub const FX_PARTICLE_CLOUD_CRT_RAND_MAX: f32 = 32767.0;

pub const FX_PARTICLE_CLOUD_CELL_SCALE_XY: f32 = 0.25;

pub const FX_PARTICLE_CLOUD_CELL_SCALE_Z: f32 = 0.125;

pub const FX_PARTICLE_CLOUD_CELL_ORIGIN: f32 = 1.0;

pub const FX_PARTICLE_FOUNTAIN_AGE_BIAS_MS: f32 = 500.0;

pub const FX_PARTICLE_FOUNTAIN_AGE_SCALE: f32 = 0.001;

pub const FX_PARTICLE_SPARK_UV: [[f32; 2]; 8] = [
    [0.0, 0.0],
    [1.0, 0.0],
    [0.0, FX_SPARKCLOUD_UV_V_1_3],
    [1.0, FX_SPARKCLOUD_UV_V_1_3],
    [0.0, FX_SPARKCLOUD_UV_V_2_3],
    [1.0, FX_SPARKCLOUD_UV_V_2_3],
    [0.0, 1.0],
    [1.0, 1.0],
];

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct GfxPosTexVertex {
    pub xyz: [f32; 3],
    pub tex_coord: [f32; 2],
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct GfxParticleCloud {
    pub quat: [f32; 4],
    pub pos: [f32; 3],
    pub placement_scale: f32,
    pub axis_or_vel: [f32; 3],
    pub color: u32,
    pub size0: f32,
    pub size1: f32,
    pub flags: u32,
    pub scale: f32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FxSparkCloudHistory {
    pub read_idx: u32,
    pub write_idx: u32,
    pub last_time: i32,
}

#[inline]
pub const fn fx_spark_cloud_handle_from_ptr_delta(byte_delta: u32) -> u16 {
    (byte_delta >> 4) as u16
}

#[inline]
pub const fn fx_spark_cloud_addr(history_base: u32, handle: u16) -> u32 {
    history_base.wrapping_add((handle as u32).wrapping_mul(0x10))
}

#[inline]
pub const fn fx_spark_cloud_handle_for_slot(slot: u32) -> u16 {
    fx_spark_cloud_handle_from_ptr_delta(slot.wrapping_mul(FX_SPARK_CLOUD_HISTORY_STRIDE as u32))
}

#[inline]
pub const fn fx_particle_cloud_draw_cell_count(cloud_flags: u32) -> usize {
    1024usize >> (cloud_flags & 3)
}

#[inline]
pub const fn fx_particle_cloud_draw_counts(spark: bool, cloud_flags: u32) -> (u32, u32) {
    let cells = fx_particle_cloud_draw_cell_count(cloud_flags) as u32;
    if spark {
        (
            cells.wrapping_mul(FX_PARTICLE_SPARK_VERTS_PER_CELL),
            cells.wrapping_mul(FX_PARTICLE_SPARK_PRIMS_PER_CELL),
        )
    } else {
        (
            cells.wrapping_mul(FX_PARTICLE_CLOUD_VERTS_PER_CELL),
            cells.wrapping_mul(FX_PARTICLE_CLOUD_PRIMS_PER_CELL),
        )
    }
}

#[inline]
pub fn fx_sparkcloud_tent_weights(v: f32) -> [f32; 3] {
    let t = v * 2.0 - 1.0;
    let w0 = sat(t);
    let w2 = sat(-t);
    [w0, 1.0 - w0 - w2, w2]
}

#[inline]
pub const fn fx_particle_cloud_particle_id(x: u32, y: u32, z: u32) -> u32 {
    z.wrapping_add(x.wrapping_shl(7))
        .wrapping_add(y.wrapping_mul(16))
}

pub const MSVCRT_HOLDRAND_DEFAULT: u32 = 1;

#[inline]
pub fn msvcrt_rand(holdrand: &mut u32) -> i32 {
    let next = holdrand.wrapping_mul(0x343fd).wrapping_add(0x269ec3);
    *holdrand = next;
    ((next >> 16) & 0x7fff) as i32
}

#[inline]
pub fn msvcrt_rand01(holdrand: &mut u32) -> f32 {
    msvcrt_rand(holdrand) as f32 / FX_PARTICLE_CLOUD_CRT_RAND_MAX
}

#[inline]
pub fn fx_particle_cloud_cell_xyz(x: u32, y: u32, z: u32, rand01: [f32; 3]) -> [f32; 3] {
    [
        (rand01[0] + x as f32) * FX_PARTICLE_CLOUD_CELL_SCALE_XY - FX_PARTICLE_CLOUD_CELL_ORIGIN,
        (rand01[1] + y as f32) * FX_PARTICLE_CLOUD_CELL_SCALE_XY - FX_PARTICLE_CLOUD_CELL_ORIGIN,
        (rand01[2] + z as f32) * FX_PARTICLE_CLOUD_CELL_SCALE_Z - FX_PARTICLE_CLOUD_CELL_ORIGIN,
    ]
}

#[inline]
pub fn fx_particle_cloud_cell_radius_sq(xyz: [f32; 3]) -> f32 {
    xyz[0] * xyz[0] + xyz[1] * xyz[1] + xyz[2] * xyz[2]
}

#[inline]
pub fn fx_particle_cloud_compare_cell_radius(a: [f32; 3], b: [f32; 3]) -> i32 {
    let ra = fx_particle_cloud_cell_radius_sq(a);
    let rb = fx_particle_cloud_cell_radius_sq(b);
    if ra < rb {
        -1
    } else if ra > rb {
        1
    } else {
        0
    }
}

#[inline]
pub fn fx_particle_cloud_cell_verts(xyz: [f32; 3]) -> [GfxPosTexVertex; 4] {
    let mut verts = [GfxPosTexVertex {
        xyz: [0.0; 3],
        tex_coord: [0.0; 2],
    }; 4];
    let mut i = 0;
    while i < 4 {
        verts[i] = GfxPosTexVertex {
            xyz,
            tex_coord: FX_PARTICLE_CLOUD_UV[i],
        };
        i += 1;
    }
    verts
}

#[inline]
pub fn fx_particle_cloud_cell_indices(cell: u32) -> [u16; 6] {
    let base = (cell.wrapping_mul(FX_PARTICLE_CLOUD_VERTS_PER_CELL)) as u16;
    let mut out = [0u16; 6];
    let mut i = 0;
    while i < 6 {
        out[i] = FX_PARTICLE_CLOUD_QUAD_INDICES[i].wrapping_add(base);
        i += 1;
    }
    out
}

#[inline]
pub fn fx_particle_spark_cell_verts(xyz: [f32; 3]) -> [GfxPosTexVertex; 8] {
    let mut verts = [GfxPosTexVertex {
        xyz: [0.0; 3],
        tex_coord: [0.0; 2],
    }; 8];
    let mut i = 0;
    while i < 8 {
        verts[i] = GfxPosTexVertex {
            xyz,
            tex_coord: FX_PARTICLE_SPARK_UV[i],
        };
        i += 1;
    }
    verts
}

#[inline]
pub fn fx_particle_spark_cell_indices(particle_id: u32) -> [u16; 18] {
    let base = (particle_id.wrapping_mul(FX_PARTICLE_SPARK_VERTS_PER_CELL)) as u16;
    let mut out = [0u16; 18];
    let mut i = 0;
    while i < 18 {
        out[i] = FX_PARTICLE_SPARK_INDICES[i].wrapping_add(base);
        i += 1;
    }
    out
}

#[inline]
pub fn gfx_pos_tex_vertex_bytes(vert: GfxPosTexVertex) -> [u8; GFX_POS_TEX_VERTEX_STRIDE] {
    let mut out = [0u8; GFX_POS_TEX_VERTEX_STRIDE];
    out[0..4].copy_from_slice(&vert.xyz[0].to_le_bytes());
    out[4..8].copy_from_slice(&vert.xyz[1].to_le_bytes());
    out[8..12].copy_from_slice(&vert.xyz[2].to_le_bytes());
    out[12..16].copy_from_slice(&vert.tex_coord[0].to_le_bytes());
    out[16..20].copy_from_slice(&vert.tex_coord[1].to_le_bytes());
    out
}

#[inline]
pub fn fx_particle_cloud_color_const(color: u32) -> [f32; 4] {
    let b = color.to_le_bytes();
    let s = crate::FX_RECIP_255 as f32;
    [
        b[2] as f32 * s,
        b[1] as f32 * s,
        b[0] as f32 * s,
        b[3] as f32 * s,
    ]
}

#[inline]
pub fn fx_particle_cloud_matrix_diag(size0: f32, size1: f32) -> [f32; 4] {
    [size0, 0.0, 0.0, size1]
}

#[inline]
pub fn fx_particle_fountain_parm0(age_msec: f32) -> [f32; 4] {
    [
        (age_msec - FX_PARTICLE_FOUNTAIN_AGE_BIAS_MS) * FX_PARTICLE_FOUNTAIN_AGE_SCALE,
        age_msec * FX_PARTICLE_FOUNTAIN_AGE_SCALE,
        0.0,
        0.0,
    ]
}

#[inline]
pub fn fx_sparkcloud_history_should_advance(write_idx: u32, last_time: i32, now_msec: i32) -> bool {
    if write_idx == 0 {
        return true;
    }
    (now_msec as f32) - (last_time as f32) >= FX_SPARKCLOUD_HISTORY_MIN_DT_MS
}

#[inline]
pub fn fx_sparkcloud_history_advance(
    read_idx: u32,
    write_idx: u32,
    now_msec: i32,
) -> (u32, u32, i32) {
    let new_write = write_idx.wrapping_add(1);
    let new_read = if new_write.wrapping_sub(read_idx) > FX_SPARK_CLOUD_SAMPLE_RING {
        read_idx.wrapping_add(1)
    } else {
        read_idx
    };
    (new_read, new_write, now_msec)
}

#[inline]
pub const fn fx_empty_particle_cloud() -> GfxParticleCloud {
    GfxParticleCloud {
        quat: [0.0, 0.0, 0.0, 1.0],
        pos: [0.0; 3],
        placement_scale: 1.0,
        axis_or_vel: [0.0; 3],
        color: 0xffff_ffff,
        size0: 0.0,
        size1: 0.0,
        flags: FX_PARTICLE_CLOUD_FLAG_SPARK,
        scale: 0.0,
    }
}

#[inline]
pub const fn fx_pack_gfx_color(rgba: [u8; 4]) -> u32 {
    let [r, g, b, a] = rgba;
    u32::from_le_bytes([b, g, r, a])
}

#[inline]
pub fn fx_build_cloud(
    world_origin: [f32; 3],
    elem_axis: [[f32; 3]; 3],
    size0: f32,
    size1: f32,
    placement_scale: f32,
    color_rgba: [u8; 4],
    elem_flags: i32,
    velocity: [f32; 3],
    time_offset: f32,
) -> GfxParticleCloud {
    let quat = fx_axis_to_quat(elem_axis);
    let vel = fx_vec3_normalize(velocity);
    GfxParticleCloud {
        quat,
        pos: world_origin,
        placement_scale,
        axis_or_vel: [
            world_origin[0] - vel[0],
            world_origin[1] - vel[1],
            world_origin[2] - vel[2],
        ],
        color: fx_pack_gfx_color(color_rgba),
        size0,
        size1,
        flags: ((elem_flags as u32) >> 29) & 3,
        scale: time_offset,
    }
}

#[inline]
pub fn fx_sparkcloud_fill_sample(
    world_origin: [f32; 3],
    elem_axis: [[f32; 3]; 3],
    size0: f32,
    placement_scale: f32,
    color_rgba: [u8; 4],
    elem_flags: i32,
    msec_now: i32,
) -> GfxParticleCloud {
    let quat = fx_axis_to_quat(elem_axis);
    let flags = (((elem_flags as u32) >> 29) & 3) | FX_PARTICLE_CLOUD_FLAG_SPARK;
    GfxParticleCloud {
        quat,
        pos: world_origin,
        placement_scale,
        axis_or_vel: world_origin,
        color: fx_pack_gfx_color(color_rgba),
        size0,
        size1: size0,
        flags,
        scale: msec_now as f32,
    }
}

#[inline]
pub fn fx_sparkcloud_lerp_sample(
    a: GfxParticleCloud,
    b: GfxParticleCloud,
    t: f32,
) -> GfxParticleCloud {
    let lerp = |x: f32, y: f32| x + t * (y - x);
    let pos = [
        lerp(a.pos[0], b.pos[0]),
        lerp(a.pos[1], b.pos[1]),
        lerp(a.pos[2], b.pos[2]),
    ];
    GfxParticleCloud {
        quat: fx_quat_nlerp(a.quat, b.quat, t),
        pos,
        placement_scale: lerp(a.placement_scale, b.placement_scale),
        axis_or_vel: pos,
        color: lerp_gfx_color(a.color, b.color, t),
        size0: lerp(a.size0, b.size0),
        size1: lerp(a.size1, b.size1),
        flags: a.flags,
        scale: lerp(a.scale, b.scale),
    }
}

#[inline]
pub fn fx_sparkcloud_build_triplet(
    samples: &[GfxParticleCloud; FX_SPARK_CLOUD_SAMPLE_RING as usize],
    read_idx: u32,
    write_idx: u32,
    draw_msec: f32,
    size1: f32,
) -> [GfxParticleCloud; 3] {
    let current = history_latest(samples, write_idx);
    if write_idx == 0 {
        return [current, current, current];
    }
    let near_t = draw_msec - fx_sparkcloud_history_lookback_ms(size1, false);
    let far_t = draw_msec - fx_sparkcloud_history_lookback_ms(size1, true);
    let near = history_sample_at(samples, read_idx, write_idx, near_t);
    let far = history_sample_at(samples, read_idx, write_idx, far_t);
    [current, near, far]
}

#[inline]
fn history_latest(
    samples: &[GfxParticleCloud; FX_SPARK_CLOUD_SAMPLE_RING as usize],
    write_idx: u32,
) -> GfxParticleCloud {
    if write_idx == 0 {
        return fx_empty_particle_cloud();
    }
    samples[((write_idx.wrapping_sub(1)) & FX_SPARK_CLOUD_SAMPLE_MASK) as usize]
}

#[inline]
fn history_sample_at(
    samples: &[GfxParticleCloud; FX_SPARK_CLOUD_SAMPLE_RING as usize],
    read_idx: u32,
    write_idx: u32,
    target_age: f32,
) -> GfxParticleCloud {
    if write_idx == 0 {
        return fx_empty_particle_cloud();
    }
    let mut u = write_idx.wrapping_sub(1);
    let mut newer = samples[(u & FX_SPARK_CLOUD_SAMPLE_MASK) as usize];
    if target_age >= newer.scale {
        return newer;
    }
    loop {
        if u == read_idx {
            return samples[(u & FX_SPARK_CLOUD_SAMPLE_MASK) as usize];
        }
        u = u.wrapping_sub(1);
        let older = samples[(u & FX_SPARK_CLOUD_SAMPLE_MASK) as usize];
        if older.scale > target_age {
            newer = older;
            continue;
        }
        if u == read_idx || older.scale == newer.scale {
            return older;
        }
        let denom = newer.scale - older.scale;
        if denom == 0.0 {
            return older;
        }
        let t = (target_age - older.scale) / denom;
        return fx_sparkcloud_lerp_sample(older, newer, t);
    }
}

#[inline]
fn lerp_gfx_color(a: u32, b: u32, t: f32) -> u32 {
    let aa = a.to_le_bytes();
    let bb = b.to_le_bytes();
    let mut out = [0u8; 4];
    for i in 0..4 {
        let v = (aa[i] as f32) + t * ((bb[i] as f32) - (aa[i] as f32));
        out[i] = libm::roundf(v).clamp(0.0, 255.0) as u8;
    }
    u32::from_le_bytes(out)
}

#[inline]
fn sat(x: f32) -> f32 {
    if x < 0.0 {
        0.0
    } else if x > 1.0 {
        1.0
    } else {
        x
    }
}
