use crate::rle::light_grid_rle_run_stride;

pub const LIGHT_GRID_ROW_HEADER_SIZE: usize = 0x0c;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LightGridRowHeader {
    pub col_start: u16,
    pub col_count: u16,
    pub row_start: u16,
    pub row_count: u16,

    pub entry_base: u32,
}

impl LightGridRowHeader {
    #[inline]
    pub fn from_bytes(bytes: &[u8]) -> Option<Self> {
        if bytes.len() < LIGHT_GRID_ROW_HEADER_SIZE {
            return None;
        }
        Some(Self {
            col_start: u16::from_le_bytes(bytes[0..2].try_into().ok()?),
            col_count: u16::from_le_bytes(bytes[2..4].try_into().ok()?),
            row_start: u16::from_le_bytes(bytes[4..6].try_into().ok()?),
            row_count: u16::from_le_bytes(bytes[6..8].try_into().ok()?),
            entry_base: u32::from_le_bytes(bytes[8..12].try_into().ok()?),
        })
    }

    #[inline]
    pub const fn rle_stride(self) -> usize {
        light_grid_rle_run_stride(self.row_count)
    }
}

#[inline]
pub const fn light_grid_row_contains_cell(header: &LightGridRowHeader, col: u32, row: u32) -> bool {
    (header.col_count as u32) >= col.wrapping_add(1)
        && (header.row_count as u32) >= row.wrapping_add(1)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LightGridRleCursor {
    pub run_offset: usize,

    pub col_in_run: u32,

    pub entry_index: u32,

    pub run_length: u8,

    pub run_increment: u8,
}

pub fn light_grid_rle_advance_to_column(
    rle: &[u8],
    row_count: u16,
    entry_base: u32,
    mut col_offset: u32,
) -> Option<LightGridRleCursor> {
    if rle.is_empty() {
        return None;
    }
    let solid_stride = light_grid_rle_run_stride(row_count);
    let mut off = 0usize;
    let mut entry_index = entry_base;
    loop {
        if off >= rle.len() {
            return None;
        }
        let run_length = rle[off];
        if (run_length as u32) > col_offset {
            let run_increment = *rle.get(off + 1)?;
            return Some(LightGridRleCursor {
                run_offset: off,
                col_in_run: col_offset,
                entry_index,
                run_length,
                run_increment,
            });
        }
        let run_increment = *rle.get(off + 1)?;
        entry_index = entry_index.wrapping_add(u32::from(run_increment) * u32::from(run_length));
        col_offset -= u32::from(run_length);
        let step = if run_increment == 0 { 2 } else { solid_stride };
        off = off.checked_add(step)?;
    }
}

pub const LIGHT_GRID_ROW_ABSENT: u16 = 0xffff;

#[inline]
pub fn light_grid_solid_run_z_base(run: &[u8], wide_row: bool) -> Option<u32> {
    let lo = *run.get(2)?;
    if wide_row {
        let hi = *run.get(3)?;
        Some(u32::from(u16::from_le_bytes([lo, hi])))
    } else {
        Some(u32::from(lo))
    }
}

#[inline]
pub const fn light_grid_solid_run_entry_index(
    entry_base: u32,
    col_in_run: u32,
    row_in_header: u32,
    run_increment: u8,
    z_base: u32,
) -> u32 {
    let row_rem = row_in_header.wrapping_sub(z_base);
    entry_base
        .wrapping_add((run_increment as u32).wrapping_mul(col_in_run))
        .wrapping_add(row_rem)
}

#[inline]
pub const fn light_grid_solid_run_has_entry(row_rem: u32, run_increment: u8) -> bool {
    row_rem < (run_increment as u32)
}

#[inline]
pub const fn light_grid_entry_byte_offset(
    entry_index: u32,
    row_rem: u32,
    run_increment: u8,
) -> Option<u32> {
    if light_grid_solid_run_has_entry(row_rem, run_increment) {
        Some(entry_index.wrapping_mul(4))
    } else {
        None
    }
}

#[inline]
pub const fn light_grid_column_entry_pair(
    entry_index: u32,
    row_rem: u32,
    run_increment: u8,
) -> [Option<u32>; 2] {
    [
        light_grid_entry_byte_offset(entry_index, row_rem, run_increment),
        light_grid_entry_byte_offset(
            entry_index.wrapping_add(1),
            row_rem.wrapping_add(1),
            run_increment,
        ),
    ]
}

#[inline]
pub const fn light_grid_at_row_last_column(cell_col: u32, header: &LightGridRowHeader) -> bool {
    cell_col.wrapping_add(1) == (header.col_start as u32).wrapping_add(header.col_count as u32)
}

#[inline]
pub const fn light_grid_next_run_first_column_index(
    run_start_entry_base: u32,
    current_run_length: u8,
    current_run_increment: u8,
    row_in_header: u32,
    next_z_base: u32,
) -> u32 {
    let row_rem = row_in_header.wrapping_sub(next_z_base);
    run_start_entry_base
        .wrapping_add((current_run_increment as u32).wrapping_mul(current_run_length as u32))
        .wrapping_add(row_rem)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LightGridEntryQuad {
    pub corners: [Option<u32>; 4],

    pub clear_needs_trace: bool,
}

pub fn light_grid_solid_run_entry_quad(
    cursor: &LightGridRleCursor,
    row_in_header: u32,
    z_base: u32,
    at_row_last_column: bool,
    next_run: Option<&[u8]>,
    wide_row: bool,
) -> Option<LightGridEntryQuad> {
    let clear_needs_trace = row_in_header < z_base;
    let row_rem = row_in_header.wrapping_sub(z_base);
    let entry_index = light_grid_solid_run_entry_index(
        cursor.entry_index,
        cursor.col_in_run,
        row_in_header,
        cursor.run_increment,
        z_base,
    );

    let pair0 = light_grid_column_entry_pair(entry_index, row_rem, cursor.run_increment);

    let (c2, c3) = if cursor.col_in_run.wrapping_add(1) < u32::from(cursor.run_length) {
        let next_index = entry_index.wrapping_add(cursor.run_increment as u32);
        let pair1 = light_grid_column_entry_pair(next_index, row_rem, cursor.run_increment);
        (pair1[0], pair1[1])
    } else if at_row_last_column {
        (None, None)
    } else {
        let next = next_run?;
        let next_z = light_grid_solid_run_z_base(next, wide_row)?;
        let next_inc = *next.get(1)?;
        let next_index = light_grid_next_run_first_column_index(
            cursor.entry_index,
            cursor.run_length,
            cursor.run_increment,
            row_in_header,
            next_z,
        );
        let next_rem = row_in_header.wrapping_sub(next_z);
        let pair1 = light_grid_column_entry_pair(next_index, next_rem, next_inc);
        (pair1[0], pair1[1])
    };

    Some(LightGridEntryQuad {
        corners: [pair0[0], pair0[1], c2, c3],
        clear_needs_trace,
    })
}

pub fn light_grid_empty_run_entry_quad(
    cursor: &LightGridRleCursor,
    row_in_header: u32,
    at_row_last_column: bool,
    next_run: Option<&[u8]>,
    wide_row: bool,
) -> Option<LightGridEntryQuad> {
    debug_assert_eq!(cursor.run_increment, 0);

    let clear_needs_trace = if let Some(next) = next_run {
        let next_inc = *next.get(1)?;
        if next_inc != 0 {
            let next_z = light_grid_solid_run_z_base(next, wide_row)?;
            row_in_header < u32::from(next_inc).wrapping_add(next_z)
        } else {
            false
        }
    } else {
        false
    };

    if cursor.col_in_run.wrapping_add(1) < u32::from(cursor.run_length) {
        return Some(LightGridEntryQuad {
            corners: [None, None, None, None],
            clear_needs_trace,
        });
    }

    if at_row_last_column {
        return Some(LightGridEntryQuad {
            corners: [None, None, None, None],
            clear_needs_trace,
        });
    }

    let next = next_run?;
    let next_z = light_grid_solid_run_z_base(next, wide_row)?;
    let next_inc = *next.get(1)?;

    let next_index = light_grid_next_run_first_column_index(
        cursor.entry_index,
        cursor.run_length,
        0,
        row_in_header,
        next_z,
    );
    let next_rem = row_in_header.wrapping_sub(next_z);
    let pair1 = light_grid_column_entry_pair(next_index, next_rem, next_inc);
    Some(LightGridEntryQuad {
        corners: [None, None, pair1[0], pair1[1]],
        clear_needs_trace,
    })
}

pub fn light_grid_column_before_row_start_quad(
    entry_base: u32,
    row_in_header: u32,
    first_run: &[u8],
    wide_row: bool,
) -> Option<LightGridEntryQuad> {
    let z_base = light_grid_solid_run_z_base(first_run, wide_row)?;
    let increment = *first_run.get(1)?;
    let clear_needs_trace = row_in_header < z_base;
    let row_rem = row_in_header.wrapping_sub(z_base);
    let entry_index = entry_base.wrapping_add(row_rem);
    let pair = light_grid_column_entry_pair(entry_index, row_rem, increment);
    Some(LightGridEntryQuad {
        corners: [None, None, pair[0], pair[1]],
        clear_needs_trace,
    })
}
