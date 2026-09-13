use crate::CPlane;
use crate::vis::msb_set;

#[derive(Clone, Copy, Debug)]
pub struct DpvsPlanes<'a> {
    pub planes: &'a [CPlane],
    pub nodes: &'a [u16],
    pub cell_count: u32,
}

impl<'a> DpvsPlanes<'a> {
    pub fn cell_for_point(&self, origin: [f32; 3]) -> Option<usize> {
        if self.nodes.is_empty() || self.cell_count == 0 {
            return None;
        }
        let cell_count_plus = self.cell_count as i32 + 1;
        let mut i = 0usize;

        for _ in 0..self.nodes.len() {
            let cell_index = i32::from(*self.nodes.get(i)?);
            if cell_index - cell_count_plus < 0 {
                let leaf = cell_index - 1;
                return usize::try_from(leaf)
                    .ok()
                    .filter(|&c| c < self.cell_count as usize);
            }
            let plane_index = (cell_index - cell_count_plus) as usize;
            let plane = self.planes.get(plane_index)?;
            let d = origin[0] * plane.normal[0]
                + origin[1] * plane.normal[1]
                + origin[2] * plane.normal[2]
                - plane.dist;
            i += if d > 0.0 {
                2
            } else {
                usize::from(*self.nodes.get(i + 1)?)
            };
        }
        None
    }

    pub fn cells_overlapping_sphere(&self, origin: [f32; 3], radius: f32, bits: &mut [u32]) -> u32 {
        if self.nodes.is_empty() || self.cell_count == 0 {
            return 0;
        }
        let before = count_msb_ones(bits);
        cells_overlapping_sphere_r(self, 0, origin, radius, bits, 0, self.nodes.len());
        count_msb_ones(bits).saturating_sub(before)
    }
}

fn count_msb_ones(bits: &[u32]) -> u32 {
    bits.iter().map(|w| w.count_ones()).sum()
}

fn cells_overlapping_sphere_r(
    planes: &DpvsPlanes<'_>,
    mut i: usize,
    origin: [f32; 3],
    radius: f32,
    bits: &mut [u32],
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
                    msb_set(bits, cell);
                }
            }
            return;
        }
        let plane_index = (cell_index - cell_count_plus) as usize;
        let Some(plane) = planes.planes.get(plane_index) else {
            return;
        };
        let d =
            origin[0] * plane.normal[0] + origin[1] * plane.normal[1] + origin[2] * plane.normal[2]
                - plane.dist;
        let Some(&offset) = planes.nodes.get(i + 1) else {
            return;
        };
        let front = i + 2;
        let back = i + usize::from(offset);
        if d >= radius {
            i = front;
        } else if d <= -radius {
            i = back;
        } else {
            cells_overlapping_sphere_r(planes, front, origin, radius, bits, depth + 1, max_depth);
            i = back;
        }
    }
}
