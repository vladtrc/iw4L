use crate::row_rle::LightGridEntryQuad;

pub const LIGHT_GRID_CELL_ORIGIN_BIAS_I32: i32 = 0x20000;

pub const LIGHT_GRID_ORIGIN_BIAS: f64 = -131072.0;

pub const LIGHT_GRID_XY_SCALE: f64 = 0.03125;

pub const LIGHT_GRID_Z_SCALE: f64 = 0.015625;

pub const LIGHT_GRID_WEIGHT_ONE: f64 = 1.0;

pub const LIGHT_GRID_CORNER_WEIGHT_EPS: f32 = 0.001;

pub const LIGHT_GRID_NEG_CELL_FLOAT_FIX: f32 = 4294967296.0;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LightGridCell {
    pub x: i32,
    pub y: i32,
    pub z: i32,
}

#[inline]
pub fn light_grid_sample_cell(sample_pos: [f32; 3]) -> LightGridCell {
    LightGridCell {
        x: (libm::floorf(sample_pos[0]) as i32).wrapping_add(LIGHT_GRID_CELL_ORIGIN_BIAS_I32) >> 5,
        y: (libm::floorf(sample_pos[1]) as i32).wrapping_add(LIGHT_GRID_CELL_ORIGIN_BIAS_I32) >> 5,
        z: (libm::floorf(sample_pos[2]) as i32).wrapping_add(LIGHT_GRID_CELL_ORIGIN_BIAS_I32) >> 6,
    }
}

#[inline]
pub fn light_grid_cell_index_as_f32(cell: i32) -> f32 {
    let f = cell as f32;
    if cell < 0 {
        f + LIGHT_GRID_NEG_CELL_FLOAT_FIX
    } else {
        f
    }
}

#[inline]
pub fn light_grid_axis_lerp(sample: f32, cell: i32, scale: f64) -> f32 {
    let cell_f = light_grid_cell_index_as_f32(cell);
    (((sample as f64) - LIGHT_GRID_ORIGIN_BIAS) * scale - (cell_f as f64)) as f32
}

#[inline]
pub fn light_grid_axis_lerps(
    sample_pos: [f32; 3],
    cell: LightGridCell,
    row_axis: u32,
    col_axis: u32,
) -> [f32; 3] {
    let cells = [cell.x, cell.y, cell.z];
    let row = row_axis as usize;
    let col = col_axis as usize;
    [
        light_grid_axis_lerp(sample_pos[row], cells[row], LIGHT_GRID_XY_SCALE),
        light_grid_axis_lerp(sample_pos[col], cells[col], LIGHT_GRID_XY_SCALE),
        light_grid_axis_lerp(sample_pos[2], cell.z, LIGHT_GRID_Z_SCALE),
    ]
}

#[inline]
pub fn light_grid_corner_weights(axis_lerp: [f32; 3]) -> [f32; 8] {
    let fx = axis_lerp[0];
    let fy = axis_lerp[1];
    let fz = axis_lerp[2];
    let one = LIGHT_GRID_WEIGHT_ONE as f32;
    let omx = one - fx;
    let omy = one - fy;
    let omz = one - fz;

    let q0 = omy * omz;
    let q1 = omy * fz;
    let q2 = fy * omz;
    let q3 = fy * fz;

    [
        q0 * omx,
        q1 * omx,
        q2 * omx,
        q3 * omx,
        q0 * fx,
        q1 * fx,
        q2 * fx,
        q3 * fx,
    ]
}

#[inline]
pub const fn light_grid_corner_weight_keeps_entry(weight: f32) -> bool {
    weight >= LIGHT_GRID_CORNER_WEIGHT_EPS
}

#[inline]
pub const fn light_grid_primary_light_sun_band(sun_primary_first: u32) -> u32 {
    0x100u32.wrapping_sub(sun_primary_first)
}

#[inline]
pub const fn light_grid_corner_needs_trace(needs_trace: u8, corner_index: u32) -> bool {
    (needs_trace & (1u8 << (corner_index & 7))) != 0
}

#[inline]
pub const fn light_grid_corner_is_suppressed(
    needs_trace: u8,
    corner_index: u32,
    is_valid_if_traced: bool,
) -> bool {
    light_grid_corner_needs_trace(needs_trace, corner_index) && !is_valid_if_traced
}

#[inline]
pub fn light_grid_primary_light_prefers(
    current: u8,
    current_weight: f32,
    candidate: u8,
    candidate_weight: f32,
    sun_band: u32,
) -> bool {
    if current == 0 {
        return true;
    }
    if candidate == 0 {
        return false;
    }
    let cur = u32::from(current);
    let cand = u32::from(candidate);
    cur >= sun_band || (cand < sun_band && current_weight < candidate_weight)
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LightGridPickCorner {
    pub primary_light: Option<u8>,
    pub needs_trace: u8,
    pub weight: f32,

    pub is_valid_if_traced: bool,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LightGridPickPrimary {
    pub primary_light: u8,
    pub best_weight: f32,
    pub honor_suppression: bool,

    pub entry_alive: [bool; 8],
}

#[inline]
pub const fn light_grid_cell_as_xyz(cell: LightGridCell) -> [i32; 3] {
    [cell.x, cell.y, cell.z]
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LightGridPickRowRleCells {
    pub base: [i32; 3],

    pub adjacent: [i32; 3],
}

#[inline]
pub const fn light_grid_pick_default_grid_entry(
    low_cleared_param4: bool,
    high_cleared_param4: bool,
) -> u32 {
    if low_cleared_param4 || high_cleared_param4 {
        0
    } else {
        1
    }
}

#[inline]
pub fn light_grid_pick_row_rle_cells(
    base_cell: LightGridCell,
    row_axis: u32,
) -> Option<LightGridPickRowRleCells> {
    let axis = row_axis as usize;
    if axis > 2 {
        return None;
    }
    let base = light_grid_cell_as_xyz(base_cell);
    let mut adjacent = base;
    adjacent[axis] = adjacent[axis].wrapping_add(1);
    Some(LightGridPickRowRleCells { base, adjacent })
}

#[inline]
pub fn light_grid_pick_merge_entry_quads(
    low_row: LightGridEntryQuad,
    high_row: LightGridEntryQuad,
) -> [Option<u32>; 8] {
    [
        low_row.corners[0],
        low_row.corners[1],
        low_row.corners[2],
        low_row.corners[3],
        high_row.corners[0],
        high_row.corners[1],
        high_row.corners[2],
        high_row.corners[3],
    ]
}

pub fn light_grid_pick_primary_from_corners(
    corners: &[LightGridPickCorner; 8],
    sun_band: u32,
) -> LightGridPickPrimary {
    let mut primary_light: u8 = 0;
    let mut best_weight = 0.0f32;
    let mut honor = false;
    let mut alive = [true; 8];

    for (i, corner) in corners.iter().enumerate() {
        let Some(cand_primary) = corner.primary_light else {
            alive[i] = false;
            continue;
        };
        if !light_grid_corner_weight_keeps_entry(corner.weight) {
            alive[i] = false;
            continue;
        }
        let suppressed = light_grid_corner_is_suppressed(
            corner.needs_trace,
            i as u32,
            corner.is_valid_if_traced,
        );
        if !suppressed {
            if !honor {
                primary_light = cand_primary;
                best_weight = corner.weight;
                honor = true;
                for slot in alive.iter_mut().take(i) {
                    *slot = false;
                }
            } else if light_grid_primary_light_prefers(
                primary_light,
                best_weight,
                cand_primary,
                corner.weight,
                sun_band,
            ) {
                primary_light = cand_primary;
                best_weight = corner.weight;
            }
        } else if honor {
            alive[i] = false;
        } else if light_grid_primary_light_prefers(
            primary_light,
            best_weight,
            cand_primary,
            corner.weight,
            sun_band,
        ) {
            primary_light = cand_primary;
            best_weight = corner.weight;
        }
    }

    LightGridPickPrimary {
        primary_light,
        best_weight,
        honor_suppression: honor,
        entry_alive: alive,
    }
}
