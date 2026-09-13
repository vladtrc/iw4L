use crate::Bounds;
use crate::cell::DpvsPlanes;
use crate::vis::msb_get;

pub const SCENE_ENT_CELL_ROW_WORDS: usize = 0x100;

pub const SCENE_ENT_CELL_VIEW_BANKS: usize = 2;

#[must_use]
pub fn scene_ent_cell_bits_len(cell_count: usize) -> usize {
    cell_count.saturating_mul(SCENE_ENT_CELL_ROW_WORDS * SCENE_ENT_CELL_VIEW_BANKS)
}

pub fn unfilter_scene_ent_from_cells_view0(bits: &mut [u32], cell_count: usize, ent_id: u32) {
    let word = (ent_id >> 5) as usize;
    let mask = !(0x8000_0000u32 >> (ent_id & 31));
    let rows = cell_count.saturating_mul(SCENE_ENT_CELL_VIEW_BANKS);
    for row in 0..rows {
        let index = row * SCENE_ENT_CELL_ROW_WORDS + word;
        if let Some(slot) = bits.get_mut(index) {
            *slot &= mask;
        }
    }
}

pub fn add_scene_ent_to_cell(bits: &mut [u32], cell: usize, ent_id: u32) {
    add_scene_ent_to_cell_offset(bits, 0, cell, ent_id);
}

pub fn add_scene_ent_to_cell_offset(
    bits: &mut [u32],
    cell_offset: usize,
    cell: usize,
    ent_id: u32,
) {
    let word = (ent_id >> 5) as usize;
    let index = cell_offset
        .saturating_add(cell)
        .saturating_mul(SCENE_ENT_CELL_ROW_WORDS)
        .saturating_add(word);
    if let Some(slot) = bits.get_mut(index) {
        *slot |= 0x8000_0000u32 >> (ent_id & 31);
    }
}

#[must_use]
pub fn scene_ent_in_cell(bits: &[u32], cell: usize, ent_id: u32) -> bool {
    let start = cell * SCENE_ENT_CELL_ROW_WORDS;
    let row = bits
        .get(start..start + SCENE_ENT_CELL_ROW_WORDS)
        .unwrap_or(&[]);
    msb_get(row, ent_id as usize)
}

#[must_use]
pub fn scene_ent_cell_row(bits: &[u32], cell: usize) -> &[u32] {
    let start = cell * SCENE_ENT_CELL_ROW_WORDS;
    bits.get(start..start + SCENE_ENT_CELL_ROW_WORDS)
        .unwrap_or(&[])
}

#[must_use]
pub fn scene_ent_cell_row_second_pass(bits: &[u32], cell_count: usize, cell: usize) -> &[u32] {
    scene_ent_cell_row(bits, cell_count.saturating_add(cell))
}

#[must_use]
pub fn scene_ent_cell_walk_words(gfx_cfg_ent_count: u32) -> usize {
    (gfx_cfg_ent_count as usize) >> 5
}

#[must_use]
pub fn scene_ent_cell_walk_bits(gfx_cfg_ent_count: u32) -> usize {
    scene_ent_cell_walk_words(gfx_cfg_ent_count).saturating_mul(32)
}

pub fn filter_scene_ent_into_cells(
    planes: &DpvsPlanes<'_>,
    bounds: Bounds,
    bits: &mut [u32],
    ent_id: u32,
) {
    filter_scene_ent_into_cells_offset(planes, bounds, bits, ent_id, 0);
}

pub fn filter_bmodel_into_cells(
    planes: &DpvsPlanes<'_>,
    bounds: Bounds,
    bits: &mut [u32],
    ent_id: u32,
) {
    filter_scene_ent_into_cells_offset(planes, bounds, bits, ent_id, planes.cell_count as usize);
}

#[must_use]
pub fn dyn_pos_filter_bounds(origin: [f32; 3], extra: f32) -> Bounds {
    Bounds::from_mid_half(origin, [extra, extra, extra])
}

pub fn filter_dyn_pos_into_cells(
    planes: &DpvsPlanes<'_>,
    origin: [f32; 3],
    extra: f32,
    bits: &mut [u32],
    ent_id: u32,
    dvar_plus_c: bool,
) {
    let bounds = dyn_pos_filter_bounds(origin, extra);
    if dvar_plus_c {
        filter_scene_ent_into_all_cells(planes, bits, ent_id);
    } else {
        filter_scene_ent_into_cells(planes, bounds, bits, ent_id);
    }
}

pub fn filter_scene_ent_into_all_cells(planes: &DpvsPlanes<'_>, bits: &mut [u32], ent_id: u32) {
    unfilter_scene_ent_from_cells_view0(bits, planes.cell_count as usize, ent_id);
    if planes.nodes.is_empty() || planes.cell_count == 0 {
        return;
    }
    filter_scene_ent_into_all_cells_r(planes, 0, bits, ent_id, 0, 0, planes.nodes.len());
}

fn filter_scene_ent_into_cells_offset(
    planes: &DpvsPlanes<'_>,
    bounds: Bounds,
    bits: &mut [u32],
    ent_id: u32,
    cell_offset: usize,
) {
    unfilter_scene_ent_from_cells_view0(bits, planes.cell_count as usize, ent_id);
    if planes.nodes.is_empty() || planes.cell_count == 0 {
        return;
    }
    filter_scene_ent_into_cells_r(
        planes,
        0,
        bounds,
        bits,
        ent_id,
        cell_offset,
        0,
        planes.nodes.len(),
    );
}

fn filter_scene_ent_into_all_cells_r(
    planes: &DpvsPlanes<'_>,
    mut i: usize,
    bits: &mut [u32],
    ent_id: u32,
    cell_offset: usize,
    depth: usize,
    max_depth: usize,
) {
    if depth > max_depth {
        return;
    }
    let cell_count_plus = planes.cell_count as i32 + 1;
    for _ in 0..planes.nodes.len() {
        let Some(&cell_index_u) = planes.nodes.get(i) else {
            return;
        };
        let cell_index = i32::from(cell_index_u);
        if cell_index - cell_count_plus < 0 {
            if cell_index > 0 {
                let cell = (cell_index - 1) as usize;
                if cell < planes.cell_count as usize {
                    add_scene_ent_to_cell_offset(bits, cell_offset, cell, ent_id);
                }
            }
            return;
        }
        let Some(&offset) = planes.nodes.get(i + 1) else {
            return;
        };
        filter_scene_ent_into_all_cells_r(
            planes,
            i + 2,
            bits,
            ent_id,
            cell_offset,
            depth + 1,
            max_depth,
        );
        i = i.saturating_add(usize::from(offset));
    }
}

fn filter_scene_ent_into_cells_r(
    planes: &DpvsPlanes<'_>,
    mut i: usize,
    bounds: Bounds,
    bits: &mut [u32],
    ent_id: u32,
    cell_offset: usize,
    depth: usize,
    max_depth: usize,
) {
    if depth > max_depth {
        return;
    }
    let cell_count_plus = planes.cell_count as i32 + 1;
    for _ in 0..planes.nodes.len() {
        let Some(&cell_index_u) = planes.nodes.get(i) else {
            return;
        };
        let cell_index = i32::from(cell_index_u);
        if cell_index - cell_count_plus < 0 {
            if cell_index > 0 {
                let cell = (cell_index - 1) as usize;
                if cell < planes.cell_count as usize {
                    add_scene_ent_to_cell_offset(bits, cell_offset, cell, ent_id);
                }
            }
            return;
        }
        let plane_index = (cell_index - cell_count_plus) as usize;
        let Some(plane) = planes.planes.get(plane_index) else {
            return;
        };
        let (mid, half) = (bounds.mid(), bounds.half());
        let d = mid[0] * plane.normal[0] + mid[1] * plane.normal[1] + mid[2] * plane.normal[2]
            - plane.dist;
        let r = half[0] * abs(plane.normal[0])
            + half[1] * abs(plane.normal[1])
            + half[2] * abs(plane.normal[2]);
        let Some(&offset) = planes.nodes.get(i + 1) else {
            return;
        };
        let front = i + 2;
        let back = i + usize::from(offset);
        if d >= r {
            i = front;
        } else if d <= -r {
            i = back;
        } else {
            filter_scene_ent_into_cells_r(
                planes,
                front,
                bounds,
                bits,
                ent_id,
                cell_offset,
                depth + 1,
                max_depth,
            );
            i = back;
        }
    }
}

#[must_use]
pub fn scene_ent_box_reaches_cell(planes: &DpvsPlanes<'_>, bounds: Bounds, cell: usize) -> bool {
    if planes.nodes.is_empty() || planes.cell_count == 0 {
        return false;
    }
    reaches_cell_r(planes, 0, bounds, cell, 0)
}

pub const SCENE_ENT_CELL_REACH_EPSILON: f32 = 0.001;

const SCENE_ENT_CELL_REACH_HALF: f32 = 0.5;

fn narrow_axial_front(bounds: Bounds, axis: usize, dist: f32) -> Bounds {
    let (mut mid, mut half) = (bounds.mid(), bounds.half());
    let max = mid[axis] + half[axis];
    mid[axis] = (dist + max) * SCENE_ENT_CELL_REACH_HALF;
    half[axis] = (max - dist) * SCENE_ENT_CELL_REACH_HALF;
    Bounds::from_mid_half(mid, half)
}

fn narrow_axial_back(bounds: Bounds, axis: usize, dist: f32) -> Bounds {
    let (mut mid, mut half) = (bounds.mid(), bounds.half());
    let min = mid[axis] - half[axis];
    mid[axis] = (min + dist) * SCENE_ENT_CELL_REACH_HALF;
    half[axis] = (dist - min) * SCENE_ENT_CELL_REACH_HALF;
    Bounds::from_mid_half(mid, half)
}

fn reaches_cell_r(
    planes: &DpvsPlanes<'_>,
    mut i: usize,
    mut bounds: Bounds,
    cell: usize,
    depth: usize,
) -> bool {
    if depth > planes.nodes.len() {
        return false;
    }
    let cell_count_plus = planes.cell_count as i32 + 1;
    for _ in 0..planes.nodes.len() {
        let Some(&node) = planes.nodes.get(i) else {
            return false;
        };
        let index = i32::from(node);
        if index - cell_count_plus < 0 {
            return index > 0 && (index - 1) as usize == cell;
        }
        let Some(plane) = planes.planes.get((index - cell_count_plus) as usize) else {
            return false;
        };
        let (mid, half) = (bounds.mid(), bounds.half());
        let d = mid[0] * plane.normal[0] + mid[1] * plane.normal[1] + mid[2] * plane.normal[2]
            - plane.dist;
        let r = half[0] * abs(plane.normal[0])
            + half[1] * abs(plane.normal[1])
            + half[2] * abs(plane.normal[2])
            - SCENE_ENT_CELL_REACH_EPSILON;
        let Some(&offset) = planes.nodes.get(i + 1) else {
            return false;
        };
        let front = i + 2;
        let back = i + usize::from(offset);
        if d > r {
            i = front;
        } else if d < -r {
            i = back;
        } else {
            let axis = usize::from(plane.r#type);
            if axis < 3 {
                if plane.dist < mid[axis] + half[axis]
                    && reaches_cell_r(
                        planes,
                        front,
                        narrow_axial_front(bounds, axis, plane.dist),
                        cell,
                        depth + 1,
                    )
                {
                    return true;
                }
                bounds = narrow_axial_back(bounds, axis, plane.dist);
                i = back;
            } else {
                if reaches_cell_r(planes, front, bounds, cell, depth + 1) {
                    return true;
                }
                i = back;
            }
        }
    }
    false
}

#[inline]
fn abs(v: f32) -> f32 {
    if v < 0.0 { -v } else { v }
}
