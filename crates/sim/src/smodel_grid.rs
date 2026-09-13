#[derive(Clone, Copy, Debug)]
pub struct SmodelBounds {
    pub mid: [f32; 3],
    pub half: [f32; 3],
}

#[derive(Clone, Debug)]
pub struct SmodelGrid {
    origin: [f32; 3],
    cell: f32,
    inv_cell: f32,
    dims: [i32; 3],
    packed: Vec<u16>,
    offsets: Vec<u32>,

    spill: Vec<u16>,
    model_n: u32,
    occupied_n: u32,
}

impl Default for SmodelGrid {
    fn default() -> Self {
        Self {
            origin: [0.0; 3],
            cell: CELL_START,
            inv_cell: 1.0 / CELL_START,
            dims: [0, 0, 0],
            packed: Vec::new(),
            offsets: Vec::new(),
            spill: Vec::new(),
            model_n: 0,
            occupied_n: 0,
        }
    }
}

const CELL_START: f32 = 256.0;
const MAX_DIM: i32 = 48;
const MAX_CELLS_PER_MODEL: i32 = 1024;
const PAD: f32 = 1.0;

impl SmodelGrid {
    pub fn is_built(&self) -> bool {
        self.dims[0] > 0 && self.dims[1] > 0 && self.dims[2] > 0
    }

    pub fn model_n(&self) -> u32 {
        self.model_n
    }

    pub fn cell_n(&self) -> u32 {
        if !self.is_built() {
            return 0;
        }
        (self.dims[0] * self.dims[1] * self.dims[2]) as u32
    }

    pub fn occupied_n(&self) -> u32 {
        self.occupied_n
    }

    pub fn spill_n(&self) -> u32 {
        self.spill.len() as u32
    }

    pub fn cell_size(&self) -> f32 {
        self.cell
    }

    pub fn build(models: impl IntoIterator<Item = SmodelBounds>) -> Self {
        let bounds: Vec<SmodelBounds> = models.into_iter().collect();
        if bounds.is_empty() || bounds.len() > u16::MAX as usize {
            return Self {
                model_n: bounds.len() as u32,
                ..Self::default()
            };
        }

        let mut mins = [f32::MAX; 3];
        let mut maxs = [f32::MIN; 3];
        let mut finite_n = 0u32;
        for b in &bounds {
            if !bounds_finite(b) {
                continue;
            }
            finite_n += 1;
            let (mn, mx) = aabb(b);
            for i in 0..3 {
                mins[i] = mins[i].min(mn[i]);
                maxs[i] = maxs[i].max(mx[i]);
            }
        }
        if finite_n == 0 {
            let mut grid = Self {
                model_n: bounds.len() as u32,
                ..Self::default()
            };
            grid.spill.extend(0..bounds.len() as u16);
            return grid;
        }
        for i in 0..3 {
            mins[i] -= PAD;
            maxs[i] += PAD;
        }

        let mut cell = CELL_START;
        let mut dims = [1i32; 3];
        loop {
            let mut ok = true;
            for i in 0..3 {
                let span = (maxs[i] - mins[i]).max(cell);
                let d = (span / cell).ceil() as i32;
                dims[i] = d.max(1);
                if dims[i] > MAX_DIM {
                    ok = false;
                }
            }
            if ok {
                break;
            }
            cell *= 2.0;
            if cell > 1.0e7 {
                let mut grid = Self {
                    model_n: bounds.len() as u32,
                    ..Self::default()
                };
                grid.spill.extend(0..bounds.len() as u16);
                return grid;
            }
        }

        let n_cells = (dims[0] * dims[1] * dims[2]) as usize;
        let mut counts = vec![0u32; n_cells];
        let mut spill = Vec::new();
        let mut ranges: Vec<Option<[i32; 6]>> = vec![None; bounds.len()];

        for (i, b) in bounds.iter().enumerate() {
            let idx = i as u16;
            if !bounds_finite(b) {
                spill.push(idx);
                continue;
            }
            let (mn, mx) = aabb(b);
            let ix0 = voxel(mn[0], mins[0], 1.0 / cell, dims[0]);
            let iy0 = voxel(mn[1], mins[1], 1.0 / cell, dims[1]);
            let iz0 = voxel(mn[2], mins[2], 1.0 / cell, dims[2]);
            let ix1 = voxel(mx[0], mins[0], 1.0 / cell, dims[0]);
            let iy1 = voxel(mx[1], mins[1], 1.0 / cell, dims[1]);
            let iz1 = voxel(mx[2], mins[2], 1.0 / cell, dims[2]);
            let vol = (ix1 - ix0 + 1) * (iy1 - iy0 + 1) * (iz1 - iz0 + 1);
            if vol > MAX_CELLS_PER_MODEL {
                spill.push(idx);
                continue;
            }
            ranges[i] = Some([ix0, iy0, iz0, ix1, iy1, iz1]);
            for iz in iz0..=iz1 {
                for iy in iy0..=iy1 {
                    for ix in ix0..=ix1 {
                        counts[cell_id(ix, iy, iz, dims)] += 1;
                    }
                }
            }
        }

        let mut offsets = vec![0u32; n_cells + 1];
        for i in 0..n_cells {
            offsets[i + 1] = offsets[i].saturating_add(counts[i]);
        }
        let mut packed = vec![0u16; offsets[n_cells] as usize];
        let mut cursor = offsets.clone();
        for (i, range) in ranges.iter().enumerate() {
            let Some([ix0, iy0, iz0, ix1, iy1, iz1]) = *range else {
                continue;
            };
            let idx = i as u16;
            for iz in iz0..=iz1 {
                for iy in iy0..=iy1 {
                    for ix in ix0..=ix1 {
                        let id = cell_id(ix, iy, iz, dims);
                        let slot = cursor[id] as usize;
                        packed[slot] = idx;
                        cursor[id] += 1;
                    }
                }
            }
        }
        let occupied_n = counts.iter().filter(|c| **c > 0).count() as u32;
        Self {
            origin: mins,
            cell,
            inv_cell: 1.0 / cell,
            dims,
            packed,
            offsets,
            spill,
            model_n: bounds.len() as u32,
            occupied_n,
        }
    }

    pub fn query(
        &self,
        start: [f32; 3],
        end: [f32; 3],
        fraction: f32,
        model_n: usize,
        scratch: &mut GridScratch,
    ) -> SmodelGridQuery {
        scratch.begin(model_n);
        if !self.is_built() {
            let n = model_n.min(u16::MAX as usize);
            for i in 0..n as u16 {
                scratch.push_unique(i);
            }
            return SmodelGridQuery {
                candidates: scratch.candidates.len() as u32,
                cells: 0,
                fallback: true,
            };
        }

        for &idx in &self.spill {
            scratch.push_unique(idx);
        }

        let t_cap = fraction.clamp(0.0, 1.0);
        if t_cap <= 0.0 {
            return SmodelGridQuery {
                candidates: scratch.candidates.len() as u32,
                cells: 0,
                fallback: false,
            };
        }

        let cells = self.dda(start, end, t_cap, scratch);
        SmodelGridQuery {
            candidates: scratch.candidates.len() as u32,
            cells,
            fallback: false,
        }
    }

    fn dda(&self, start: [f32; 3], end: [f32; 3], t_cap: f32, scratch: &mut GridScratch) -> u32 {
        let dir = [end[0] - start[0], end[1] - start[1], end[2] - start[2]];
        let grid_max = [
            self.origin[0] + self.cell * self.dims[0] as f32,
            self.origin[1] + self.cell * self.dims[1] as f32,
            self.origin[2] + self.cell * self.dims[2] as f32,
        ];
        let Some((t0, t1)) = clip_ray_aabb(start, dir, self.origin, grid_max, t_cap) else {
            return 0;
        };
        let p = [
            start[0] + dir[0] * t0,
            start[1] + dir[1] * t0,
            start[2] + dir[2] * t0,
        ];
        let mut cell = [
            voxel(p[0], self.origin[0], self.inv_cell, self.dims[0]),
            voxel(p[1], self.origin[1], self.inv_cell, self.dims[1]),
            voxel(p[2], self.origin[2], self.inv_cell, self.dims[2]),
        ];

        let mut step = [0i32; 3];
        let mut t_max = [f32::INFINITY; 3];
        let mut t_delta = [f32::INFINITY; 3];
        for i in 0..3 {
            if dir[i].abs() < 1e-20 {
                step[i] = 0;
                continue;
            }
            if dir[i] > 0.0 {
                step[i] = 1;
                let next = self.origin[i] + (cell[i] + 1) as f32 * self.cell;
                t_max[i] = (next - start[i]) / dir[i];
                t_delta[i] = self.cell / dir[i];
            } else {
                step[i] = -1;
                let next = self.origin[i] + cell[i] as f32 * self.cell;
                t_max[i] = (next - start[i]) / dir[i];
                t_delta[i] = -self.cell / dir[i];
            }
        }

        let mut visited = 0u32;
        let safety = self.cell_n().saturating_add(2);
        loop {
            self.visit_cell(cell, scratch);
            visited = visited.saturating_add(1);
            if visited >= safety {
                break;
            }
            let axis = argmin3(t_max);
            if t_max[axis] > t1 + 1e-5 {
                break;
            }
            cell[axis] += step[axis];
            if cell[axis] < 0 || cell[axis] >= self.dims[axis] {
                break;
            }
            t_max[axis] += t_delta[axis];
        }
        visited
    }

    fn visit_cell(&self, cell: [i32; 3], scratch: &mut GridScratch) {
        let id = cell_id(cell[0], cell[1], cell[2], self.dims);
        let a = self.offsets[id] as usize;
        let b = self.offsets[id + 1] as usize;
        for &idx in &self.packed[a..b] {
            scratch.push_unique(idx);
        }
    }
}

#[derive(Clone, Debug, Default)]
pub struct GridScratch {
    pub candidates: Vec<u16>,
    stamp: u32,
    seen: Vec<u32>,
}

impl GridScratch {
    fn begin(&mut self, model_n: usize) {
        self.candidates.clear();
        self.stamp = self.stamp.wrapping_add(1);
        if self.stamp == 0 {
            self.seen.fill(0);
            self.stamp = 1;
        }
        if self.seen.len() < model_n {
            self.seen.resize(model_n, 0);
        }
    }

    fn push_unique(&mut self, idx: u16) {
        let i = idx as usize;
        if i >= self.seen.len() {
            self.seen.resize(i + 1, 0);
        }
        if self.seen[i] == self.stamp {
            return;
        }
        self.seen[i] = self.stamp;
        self.candidates.push(idx);
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct SmodelGridQuery {
    pub candidates: u32,
    pub cells: u32,
    pub fallback: bool,
}

fn bounds_finite(b: &SmodelBounds) -> bool {
    b.mid.iter().all(|c| c.is_finite()) && b.half.iter().all(|c| c.is_finite() && *c >= 0.0)
}

fn aabb(b: &SmodelBounds) -> ([f32; 3], [f32; 3]) {
    (
        [
            b.mid[0] - b.half[0],
            b.mid[1] - b.half[1],
            b.mid[2] - b.half[2],
        ],
        [
            b.mid[0] + b.half[0],
            b.mid[1] + b.half[1],
            b.mid[2] + b.half[2],
        ],
    )
}

fn voxel(p: f32, origin: f32, inv: f32, dim: i32) -> i32 {
    let v = ((p - origin) * inv).floor() as i32;
    v.clamp(0, dim - 1)
}

fn cell_id(ix: i32, iy: i32, iz: i32, dims: [i32; 3]) -> usize {
    ((iz * dims[1] + iy) * dims[0] + ix) as usize
}

fn argmin3(t: [f32; 3]) -> usize {
    if t[0] <= t[1] && t[0] <= t[2] {
        0
    } else if t[1] <= t[2] {
        1
    } else {
        2
    }
}

fn clip_ray_aabb(
    start: [f32; 3],
    dir: [f32; 3],
    mins: [f32; 3],
    maxs: [f32; 3],
    t_cap: f32,
) -> Option<(f32, f32)> {
    let mut t0 = 0.0_f32;
    let mut t1 = t_cap.min(1.0);
    for i in 0..3 {
        if dir[i].abs() < 1e-20 {
            if start[i] < mins[i] || start[i] > maxs[i] {
                return None;
            }
            continue;
        }
        let mut t_near = (mins[i] - start[i]) / dir[i];
        let mut t_far = (maxs[i] - start[i]) / dir[i];
        if t_near > t_far {
            core::mem::swap(&mut t_near, &mut t_far);
        }
        t0 = t0.max(t_near);
        t1 = t1.min(t_far);
        if t0 > t1 {
            return None;
        }
    }
    Some((t0, t1))
}
