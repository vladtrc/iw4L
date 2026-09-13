use crate::isvalid::light_grid_corner_inch_deltas;
use crate::isvalid::{LIGHT_GRID_CELL_TO_INCHES_XY, LIGHT_GRID_CELL_TO_INCHES_Z};
use crate::pick::{LIGHT_GRID_XY_SCALE, LIGHT_GRID_Z_SCALE};

pub const LIGHT_GRID_TRACE_ALWAYS_ALLOW_TYPE: u8 = 1;

#[inline]
pub fn light_grid_trace_quantize_axis(sample: f32, recip: f64, cell_inches: f64) -> f32 {
    let cell = libm::floor((sample as f64) * recip);
    (cell * cell_inches) as f32
}

pub fn light_grid_trace_corner_pos(
    sample_pos: [f32; 3],
    corner_index: u32,
    row_axis: u32,
    col_axis: u32,
) -> Option<[f32; 3]> {
    let row = row_axis as usize;
    let col = col_axis as usize;
    if row > 2 || col > 2 {
        return None;
    }
    let mut pos = [
        light_grid_trace_quantize_axis(
            sample_pos[0],
            LIGHT_GRID_XY_SCALE,
            LIGHT_GRID_CELL_TO_INCHES_XY,
        ),
        light_grid_trace_quantize_axis(
            sample_pos[1],
            LIGHT_GRID_XY_SCALE,
            LIGHT_GRID_CELL_TO_INCHES_XY,
        ),
        light_grid_trace_quantize_axis(
            sample_pos[0],
            LIGHT_GRID_Z_SCALE,
            LIGHT_GRID_CELL_TO_INCHES_Z,
        ),
    ];
    let (d_row, d_col, d_z) = light_grid_corner_inch_deltas(corner_index);
    pos[row] += d_row;
    pos[col] += d_col;
    pos[2] += d_z;
    Some(pos)
}

pub fn light_grid_trace_allows(
    primary_light_type: u8,
    sample_pos: [f32; 3],
    corner_index: u32,
    row_axis: u32,
    col_axis: u32,
    can_influence: impl Fn([f32; 3]) -> bool,
) -> Option<bool> {
    if primary_light_type == LIGHT_GRID_TRACE_ALWAYS_ALLOW_TYPE {
        return Some(true);
    }
    let corner = light_grid_trace_corner_pos(sample_pos, corner_index, row_axis, col_axis)?;
    Some(can_influence(corner))
}
