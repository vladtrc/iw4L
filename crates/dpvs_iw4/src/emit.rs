use crate::drawsurf::{GfxDrawSurf, with_object_id};
use crate::vis::VisBits;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct SurfRange {
    pub start: u16,
    pub count: u16,
}

impl SurfRange {
    pub fn end(self) -> u32 {
        u32::from(self.start) + u32::from(self.count)
    }
}

pub fn emit_visible_cell_roots(
    cell_roots: &[SurfRange],
    vis: &VisBits<'_>,
    out: &mut [SurfRange],
) -> usize {
    let mut n = 0usize;
    for (i, range) in cell_roots.iter().enumerate() {
        if !vis.get(i) || range.count == 0 {
            continue;
        }
        if let Some(slot) = out.get_mut(n) {
            *slot = *range;
            n += 1;
        } else {
            break;
        }
    }
    n
}

pub fn expand_sorted_span(sorted_surf_index: &[u16], range: SurfRange, out: &mut [u16]) -> usize {
    let start = usize::from(range.start);
    let count = usize::from(range.count);
    let mut n = 0usize;
    for i in 0..count {
        let Some(&surf) = sorted_surf_index.get(start + i) else {
            break;
        };
        if let Some(slot) = out.get_mut(n) {
            *slot = surf;
            n += 1;
        } else {
            break;
        }
    }
    n
}

pub fn emit_draw_surfs_for_span(
    surface_materials: &[GfxDrawSurf],
    sorted_surf_index: &[u16],
    range: SurfRange,
    object_id: u16,
    out: &mut [GfxDrawSurf],
) -> usize {
    let mut n = 0usize;
    let mut indices = [0u16; 256];
    let total = usize::from(range.count);
    let mut offset = 0usize;
    while offset < total && n < out.len() {
        let chunk = (total - offset).min(indices.len()).min(out.len() - n);
        let sub = SurfRange {
            start: range.start.saturating_add(offset as u16),
            count: chunk as u16,
        };
        let got = expand_sorted_span(sorted_surf_index, sub, &mut indices[..chunk]);
        for &surf in indices[..got].iter() {
            let base = surface_materials
                .get(usize::from(surf))
                .copied()
                .unwrap_or(GfxDrawSurf::from_packed(0));
            if let Some(slot) = out.get_mut(n) {
                *slot = with_object_id(base, object_id);
                n += 1;
            } else {
                return n;
            }
        }
        offset += got;
        if got == 0 {
            break;
        }
    }
    n
}
