use crate::pick::{LightGridCell, light_grid_cell_as_xyz, light_grid_cell_index_as_f32};

pub const LIGHT_GRID_CELL_TO_INCHES_XY: f64 = 32.0;

pub const LIGHT_GRID_CELL_TO_INCHES_Z: f64 = 64.0;

pub const LIGHT_GRID_ISVALID_ORIGIN_SUB: f64 = 131072.0;

pub const LIGHT_GRID_SIGHT_NUDGE: f64 = f64::from_bits(0x3f847ae140000000);

pub const LIGHT_GRID_SIGHT_CONTENT_MASK: u32 = 0x2001;

const VEC3_NORMALIZE_EPS: f32 = 1e-12;

#[inline]
pub fn light_grid_corner_inch_deltas(corner_index: u32) -> (f32, f32, f32) {
    let c = corner_index & 7;
    (
        ((c & 4) * 8) as f32,
        ((c & 2) << 4) as f32,
        ((c & 1) << 6) as f32,
    )
}

#[inline]
pub fn light_grid_cell_axis_to_inches(cell: i32, z_axis: bool) -> f32 {
    let cell_f = light_grid_cell_index_as_f32(cell) as f64;
    let scale = if z_axis {
        LIGHT_GRID_CELL_TO_INCHES_Z
    } else {
        LIGHT_GRID_CELL_TO_INCHES_XY
    };
    (cell_f * scale - LIGHT_GRID_ISVALID_ORIGIN_SUB) as f32
}

pub fn light_grid_isvalid_corner_pos(
    cell: LightGridCell,
    corner_index: u32,
    row_axis: u32,
    col_axis: u32,
) -> Option<[f32; 3]> {
    let row = row_axis as usize;
    let col = col_axis as usize;
    if row > 2 || col > 2 {
        return None;
    }
    let xyz = light_grid_cell_as_xyz(cell);
    let mut pos = [
        light_grid_cell_axis_to_inches(xyz[0], false),
        light_grid_cell_axis_to_inches(xyz[1], false),
        light_grid_cell_axis_to_inches(xyz[2], true),
    ];
    let (d_row, d_col, d_z) = light_grid_corner_inch_deltas(corner_index);
    pos[row] += d_row;
    pos[col] += d_col;
    pos[2] += d_z;
    Some(pos)
}

#[inline]
pub fn light_grid_vec3_normalize(v: &mut [f32; 3]) {
    let len_sq = v[0] * v[0] + v[1] * v[1] + v[2] * v[2];
    if len_sq <= VEC3_NORMALIZE_EPS * VEC3_NORMALIZE_EPS {
        return;
    }
    let inv = 1.0 / libm::sqrtf(len_sq);
    v[0] *= inv;
    v[1] *= inv;
    v[2] *= inv;
}

pub fn light_grid_isvalid_nudged_target(sample_pos: [f32; 3], grid_pos: [f32; 3]) -> [f32; 3] {
    let mut dir = [
        sample_pos[0] - grid_pos[0],
        sample_pos[1] - grid_pos[1],
        sample_pos[2] - grid_pos[2],
    ];
    light_grid_vec3_normalize(&mut dir);
    let n = LIGHT_GRID_SIGHT_NUDGE as f32;
    [
        grid_pos[0] + dir[0] * n,
        grid_pos[1] + dir[1] * n,
        grid_pos[2] + dir[2] * n,
    ]
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LightGridIsValidSegment {
    pub sample_pos: [f32; 3],
    pub nudged_grid_pos: [f32; 3],
    pub grid_pos: [f32; 3],
}

pub fn light_grid_isvalid_segment(
    cell: LightGridCell,
    corner_index: u32,
    row_axis: u32,
    col_axis: u32,
    sample_pos: [f32; 3],
) -> Option<LightGridIsValidSegment> {
    let grid_pos = light_grid_isvalid_corner_pos(cell, corner_index, row_axis, col_axis)?;
    let nudged_grid_pos = light_grid_isvalid_nudged_target(sample_pos, grid_pos);
    Some(LightGridIsValidSegment {
        sample_pos,
        nudged_grid_pos,
        grid_pos,
    })
}

pub fn light_grid_isvalid_sample(
    cell: LightGridCell,
    corner_index: u32,
    row_axis: u32,
    col_axis: u32,
    sample_pos: [f32; 3],
    sight_clear: impl Fn([f32; 3], [f32; 3]) -> bool,
) -> Option<bool> {
    let seg = light_grid_isvalid_segment(cell, corner_index, row_axis, col_axis, sample_pos)?;
    Some(sight_clear(seg.sample_pos, seg.nudged_grid_pos))
}
