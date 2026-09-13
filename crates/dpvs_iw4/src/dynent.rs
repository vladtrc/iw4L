use crate::Bounds;
use crate::aabb::bounds_culled;
use crate::cell::DpvsPlanes;
use crate::vis::msb_get;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DynBrushVisWrite {
    AlreadyVisible,

    Admitted,

    Culled,
}

pub fn write_dyn_brush_vis(vis: &mut u8, bounds: Bounds, planes: &[[f32; 4]]) -> DynBrushVisWrite {
    if *vis != 0 {
        return DynBrushVisWrite::AlreadyVisible;
    }
    if bounds_culled(bounds, planes) {
        DynBrushVisWrite::Culled
    } else {
        *vis = 1;
        DynBrushVisWrite::Admitted
    }
}

#[must_use]
pub fn dyn_brush_scene_list_admits(vis: u8, surface_count: u16) -> bool {
    vis & 1 != 0 && surface_count != 0
}

pub fn dyn_ent_client_word_count(client_count: u32) -> usize {
    (client_count as usize).div_ceil(32)
}

pub fn dyn_ent_cell_bits_len(word_count: usize, cell_count: usize) -> usize {
    word_count.saturating_mul(cell_count)
}

pub fn unfilter_dyn_ent_from_cells(
    bits: &mut [u32],
    word_count: usize,
    cell_count: usize,
    dyn_ent_id: u32,
) {
    if word_count == 0 {
        return;
    }
    let word = (dyn_ent_id >> 5) as usize;
    let mask = !(0x8000_0000u32 >> (dyn_ent_id & 31));
    for cell in 0..cell_count {
        let index = cell * word_count + word;
        if let Some(slot) = bits.get_mut(index) {
            *slot &= mask;
        }
    }
}

pub fn add_dyn_ent_to_cell(bits: &mut [u32], word_count: usize, cell: usize, dyn_ent_id: u32) {
    if word_count == 0 {
        return;
    }
    let word = (dyn_ent_id >> 5) as usize;
    let index = cell * word_count + word;
    if let Some(slot) = bits.get_mut(index) {
        *slot |= 0x8000_0000u32 >> (dyn_ent_id & 31);
    }
}

pub fn cell_dyn_ent_words(bits: &[u32], word_count: usize, cell: usize) -> &[u32] {
    if word_count == 0 {
        return &[];
    }
    let start = cell * word_count;
    bits.get(start..start + word_count).unwrap_or(&[])
}

pub fn dyn_ent_in_cell(bits: &[u32], word_count: usize, cell: usize, dyn_ent_id: u32) -> bool {
    let words = cell_dyn_ent_words(bits, word_count, cell);
    msb_get(words, dyn_ent_id as usize)
}

pub fn filter_dyn_ent_into_cells(
    planes: &DpvsPlanes<'_>,
    bounds: Bounds,
    bits: &mut [u32],
    word_count: usize,
    dyn_ent_id: u32,
) {
    unfilter_dyn_ent_from_cells(bits, word_count, planes.cell_count as usize, dyn_ent_id);
    if planes.nodes.is_empty() || planes.cell_count == 0 || word_count == 0 {
        return;
    }
    filter_dyn_ent_into_cells_r(
        planes,
        0,
        bounds,
        bits,
        word_count,
        dyn_ent_id,
        0,
        planes.nodes.len(),
    );
}

fn filter_dyn_ent_into_cells_r(
    planes: &DpvsPlanes<'_>,
    mut i: usize,
    bounds: Bounds,
    bits: &mut [u32],
    word_count: usize,
    dyn_ent_id: u32,
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
                    add_dyn_ent_to_cell(bits, word_count, cell, dyn_ent_id);
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
            filter_dyn_ent_into_cells_r(
                planes,
                front,
                bounds,
                bits,
                word_count,
                dyn_ent_id,
                depth + 1,
                max_depth,
            );
            i = back;
        }
    }
}

#[must_use]
pub fn unlink_dyn_ent_primary_light_bit_index(
    dyn_ent_id: u32,
    light_index: u32,
    sun_primary: u32,
    primary_count: u32,
) -> Option<u32> {
    let sun1 = sun_primary.wrapping_add(1);
    if light_index < sun1 || light_index >= primary_count {
        return None;
    }
    let span = primary_count.wrapping_sub(sun1) as i32;
    let i = span
        .wrapping_mul(dyn_ent_id as i32)
        .wrapping_sub(sun1 as i32)
        .wrapping_add(light_index as i32);
    if i < 0 { None } else { Some(i as u32) }
}

#[must_use]
pub fn dyn_ent_primary_light_vis_word_count(
    client_count: u32,
    sun_primary: u32,
    primary_count: u32,
) -> usize {
    if client_count == 0 {
        return 0;
    }
    unlink_dyn_ent_primary_light_bit_index(
        client_count - 1,
        primary_count.saturating_sub(1),
        sun_primary,
        primary_count,
    )
    .map(|bit| (bit >> 5) as usize + 1)
    .unwrap_or(0)
}

pub fn link_dyn_ent_primary_light_bit(
    words: &mut [u32],
    dyn_ent_id: u32,
    light_index: u32,
    sun_primary: u32,
    primary_count: u32,
    set: bool,
) {
    let Some(bit) =
        unlink_dyn_ent_primary_light_bit_index(dyn_ent_id, light_index, sun_primary, primary_count)
    else {
        return;
    };
    let w = (bit >> 5) as usize;
    let mask = 1u32 << (bit & 31);
    if let Some(word) = words.get_mut(w) {
        if set {
            *word |= mask;
        } else {
            *word &= !mask;
        }
    }
}

pub fn unlink_dyn_ent_from_primary_lights(
    words: &mut [u32],
    dyn_ent_id: u32,
    sun_primary: u32,
    primary_count: u32,
) {
    let mut light = sun_primary.saturating_add(1);
    while light < primary_count {
        if let Some(bit) =
            unlink_dyn_ent_primary_light_bit_index(dyn_ent_id, light, sun_primary, primary_count)
        {
            let w = (bit >> 5) as usize;
            if let Some(word) = words.get_mut(w) {
                *word &= !(1u32 << (bit & 31));
            }
        }
        light += 1;
    }
}

#[inline]
fn abs(v: f32) -> f32 {
    if v < 0.0 { -v } else { v }
}
