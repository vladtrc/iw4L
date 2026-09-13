use crate::pool::FX_RAND_TABLE_MOD;
use crate::random::fx_random_table_f32;

pub const FX_GLASS_SHATTER_BRANCH_SCALE: f32 = -3.0;

pub const FX_GLASS_SHATTER_TWO_PI: f32 = 6.283_185_482_025_146_5;

pub const FX_GLASS_FRINGE_MAXCOVERAGE: f32 = 0.2;

pub const FX_GLASS_STATE_FLAG_SHATTERED: u16 = 2;

pub const FX_GLASS_STATE_FLAG_CHILD_CLEAR: u16 = 4;

pub const FX_GLASS_LINEAR_VEL_MIN: f32 = 200.0;

pub const FX_GLASS_LINEAR_VEL_MAX: f32 = 400.0;

pub const FX_GLASS_ANGULAR_VEL_MIN: f32 = 5.0;

pub const FX_GLASS_ANGULAR_VEL_MAX: f32 = 35.0;

const FX_GLASS_MASS_AREA_MIN: f32 = 0.7;

const FX_GLASS_MASS_AREA_MAX: f32 = 2048.0;

const FX_GLASS_MASS_RSQRT_SCALE: f32 = 11.313_709_259_033_203;

pub const FX_GLASS_SPLIT_MAX_CHILDREN: usize = 6;
pub const FX_GLASS_SPLIT_MAX_VERTS: usize = 32;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FxGlassSplitLoop {
    pub verts: [[i16; 2]; FX_GLASS_SPLIT_MAX_VERTS],
    pub vert_n: u8,

    pub original_edges: u32,
}

impl Default for FxGlassSplitLoop {
    fn default() -> Self {
        Self {
            verts: [[0; 2]; FX_GLASS_SPLIT_MAX_VERTS],
            vert_n: 0,
            original_edges: 0,
        }
    }
}

pub fn fx_glass_shatter_rand(cursor: &mut u32) -> f32 {
    *cursor = cursor.saturating_add(1);
    if *cursor == FX_RAND_TABLE_MOD {
        *cursor = 0;
    }
    fx_random_table_f32(*cursor, 0)
}

pub fn fx_glass_interior_branch_count(rand01: f32) -> u32 {
    let rounded = libm::roundf(rand01 * FX_GLASS_SHATTER_BRANCH_SCALE) as i32;
    3u32.wrapping_sub(rounded as u32).clamp(3, 6)
}

pub fn fx_glass_interior_angle_step(branch_n: u32) -> f32 {
    if branch_n == 0 {
        return 0.0;
    }
    FX_GLASS_SHATTER_TWO_PI / branch_n as f32
}

pub fn fx_glass_piece_speed_scale(area_x2: f32) -> f32 {
    let v = area_x2.clamp(FX_GLASS_MASS_AREA_MIN, FX_GLASS_MASS_AREA_MAX);
    let i = 0x5f37_59df_u32.wrapping_sub(v.to_bits() >> 1);
    let y = f32::from_bits(i);
    y * (1.5 - 0.5 * v * y * y) * FX_GLASS_MASS_RSQRT_SCALE
}

pub fn fx_glass_lerp_range(min: f32, max: f32, rand01: f32) -> f32 {
    min + (max - min) * rand01
}

pub fn fx_glass_loop_area_x2(verts: &[[i16; 2]]) -> f32 {
    if verts.len() < 3 {
        return 0.0;
    }
    let mut acc = 0.0f32;
    for i in 0..verts.len() {
        let a = verts[i];
        let b = verts[(i + 1) % verts.len()];
        acc += a[0] as f32 * b[1] as f32 - b[0] as f32 * a[1] as f32;
    }
    acc.abs() * crate::glass::FX_GLASS_VERT_SCALE * crate::glass::FX_GLASS_VERT_SCALE
}

pub fn fx_glass_fringe_prune_knock_order(
    areas: &[f32],
    supports: &[u32],
) -> [u8; FX_GLASS_SPLIT_MAX_CHILDREN] {
    let mut idx = [0u8; FX_GLASS_SPLIT_MAX_CHILDREN];
    let mut n = 0usize;
    let len = areas
        .len()
        .min(supports.len())
        .min(FX_GLASS_SPLIT_MAX_CHILDREN);
    for i in 0..len {
        if supports[i] != 0 {
            idx[n] = i as u8;
            n += 1;
        }
    }
    for i in 1..n {
        let mut j = i;
        while j > 0 {
            let a = areas[idx[j] as usize];
            let b = areas[idx[j - 1] as usize];
            if a <= b {
                break;
            }
            idx.swap(j, j - 1);
            j -= 1;
        }
    }
    idx
}

pub fn fx_glass_fringe_cap(original_area: f32) -> f32 {
    FX_GLASS_FRINGE_MAXCOVERAGE * original_area
}

pub fn fx_glass_point_in_convex(verts: &[[i16; 2]], p: [f32; 2]) -> bool {
    if verts.len() < 3 {
        return false;
    }
    let mut sign = 0.0f32;
    for i in 0..verts.len() {
        let a = verts[i];
        let b = verts[(i + 1) % verts.len()];
        let cross = (b[0] as f32 - a[0] as f32) * (p[1] - a[1] as f32)
            - (b[1] as f32 - a[1] as f32) * (p[0] - a[0] as f32);
        if cross.abs() <= 1e-3 {
            continue;
        }
        let s = if cross > 0.0 { 1.0 } else { -1.0 };
        if sign == 0.0 {
            sign = s;
        } else if s != sign {
            return false;
        }
    }
    true
}

pub fn fx_glass_radial_split(
    verts: &[[i16; 2]],
    impact: [f32; 2],
    branch_n: u32,
) -> [FxGlassSplitLoop; FX_GLASS_SPLIT_MAX_CHILDREN] {
    let mut out = [FxGlassSplitLoop::default(); FX_GLASS_SPLIT_MAX_CHILDREN];
    if verts.len() < 3 || !(3..=6).contains(&branch_n) {
        return out;
    }
    if !fx_glass_point_in_convex(verts, impact) {
        return out;
    }
    let n = branch_n as usize;
    let step = fx_glass_interior_angle_step(branch_n);
    let mut hits = [(0usize, 0.0f32, [0.0f32; 2]); FX_GLASS_SPLIT_MAX_CHILDREN];
    let mut hit_n = 0usize;
    for k in 0..n {
        let ang = step * k as f32;
        let dir = [libm::cosf(ang), libm::sinf(ang)];
        let Some((edge, t, p)) = closest_forward_hit(verts, impact, dir) else {
            return [FxGlassSplitLoop::default(); FX_GLASS_SPLIT_MAX_CHILDREN];
        };
        hits[hit_n] = (edge, t, p);
        hit_n += 1;
    }
    for i in 1..hit_n {
        let mut j = i;
        while j > 0 {
            let aa = libm::atan2f(hits[j].2[1] - impact[1], hits[j].2[0] - impact[0]);
            let bb = libm::atan2f(hits[j - 1].2[1] - impact[1], hits[j - 1].2[0] - impact[0]);
            if aa >= bb {
                break;
            }
            hits.swap(j, j - 1);
            j -= 1;
        }
    }
    for i in 0..hit_n {
        let a = hits[i];
        let b = hits[(i + 1) % hit_n];
        if let Some(loop_i) = build_child(verts, impact, a, b) {
            out[i] = loop_i;
        }
    }
    out
}

fn closest_forward_hit(
    verts: &[[i16; 2]],
    origin: [f32; 2],
    dir: [f32; 2],
) -> Option<(usize, f32, [f32; 2])> {
    let mut best_t = f32::INFINITY;
    let mut best = None;
    for i in 0..verts.len() {
        let a = [verts[i][0] as f32, verts[i][1] as f32];
        let b = [
            verts[(i + 1) % verts.len()][0] as f32,
            verts[(i + 1) % verts.len()][1] as f32,
        ];
        let Some((t_ray, t_seg)) = ray_segment(origin, dir, a, b) else {
            continue;
        };
        if t_ray <= 1e-3 || !(0.0..=1.0).contains(&t_seg) || t_ray >= best_t {
            continue;
        }
        best_t = t_ray;
        best = Some((
            i,
            t_seg,
            [origin[0] + dir[0] * t_ray, origin[1] + dir[1] * t_ray],
        ));
    }
    best
}

fn ray_segment(o: [f32; 2], d: [f32; 2], a: [f32; 2], b: [f32; 2]) -> Option<(f32, f32)> {
    let ex = b[0] - a[0];
    let ey = b[1] - a[1];
    let det = d[0] * ey - d[1] * ex;
    if det.abs() <= 1e-8 {
        return None;
    }
    let ox = a[0] - o[0];
    let oy = a[1] - o[1];
    let t_ray = (ox * ey - oy * ex) / det;
    let t_seg = (ox * d[1] - oy * d[0]) / det;
    Some((t_ray, t_seg))
}

fn build_child(
    verts: &[[i16; 2]],
    impact: [f32; 2],
    hit_a: (usize, f32, [f32; 2]),
    hit_b: (usize, f32, [f32; 2]),
) -> Option<FxGlassSplitLoop> {
    let mut loop_i = FxGlassSplitLoop::default();
    push_unique(&mut loop_i, round_i16(impact))?;
    push_unique(&mut loop_i, round_i16(hit_a.2))?;
    if hit_a.0 == hit_b.0 {
        push_unique(&mut loop_i, round_i16(hit_b.2))?;
    } else {
        let mut e = (hit_a.0 + 1) % verts.len();
        loop {
            push_unique(&mut loop_i, verts[e])?;
            if e == hit_b.0 {
                break;
            }
            e = (e + 1) % verts.len();
            if e == (hit_a.0 + 1) % verts.len() {
                break;
            }
        }
        push_unique(&mut loop_i, round_i16(hit_b.2))?;
    }
    if loop_i.vert_n < 3 {
        return None;
    }
    loop_i.original_edges = original_edge_mask(verts.len(), hit_a.0, hit_b.0);
    Some(loop_i)
}

fn original_edge_mask(n: usize, edge_a: usize, edge_b: usize) -> u32 {
    if n == 0 || n > 32 {
        return 0;
    }
    if edge_a == edge_b {
        return 1u32 << (edge_a % 32);
    }
    let mut m = 0u32;
    let mut e = edge_a;
    loop {
        m |= 1u32 << (e % 32);
        if e == edge_b {
            break;
        }
        e = (e + 1) % n;
        if e == edge_a {
            break;
        }
    }
    m
}

fn round_i16(p: [f32; 2]) -> [i16; 2] {
    [
        libm::roundf(p[0]).clamp(i16::MIN as f32, i16::MAX as f32) as i16,
        libm::roundf(p[1]).clamp(i16::MIN as f32, i16::MAX as f32) as i16,
    ]
}

fn push_unique(loop_i: &mut FxGlassSplitLoop, v: [i16; 2]) -> Option<()> {
    if (loop_i.vert_n as usize) >= FX_GLASS_SPLIT_MAX_VERTS {
        return None;
    }
    if loop_i.vert_n > 0 {
        let last = loop_i.verts[loop_i.vert_n as usize - 1];
        if last == v {
            return Some(());
        }
        if loop_i.verts[0] == v {
            return Some(());
        }
    }
    loop_i.verts[loop_i.vert_n as usize] = v;
    loop_i.vert_n = loop_i.vert_n.saturating_add(1);
    Some(())
}
