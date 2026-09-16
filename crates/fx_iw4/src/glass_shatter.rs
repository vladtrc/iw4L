use crate::pool::FX_RAND_TABLE_MOD;
use crate::random::fx_random_table_f32;

pub const FX_GLASS_SHATTER_BRANCH_SCALE: f32 = -3.0;

pub const FX_GLASS_SHATTER_TWO_PI: f32 = 6.283_185_482_025_146_5;

pub const FX_GLASS_FRINGE_MAXCOVERAGE: f32 = 0.2;

pub const FX_GLASS_SHARD_MAXSIZE: f32 = 300.0;

pub const FX_GLASS_FRINGE_MAXSIZE: f32 = 150.0;

pub const FX_GLASS_MAX_PIECES_PER_FRAME: f32 = 100.0;

pub const FX_GLASS_SPLIT_QUEUE_CAP: usize = 32;

pub const FX_GLASS_SPLIT_OP_CAP: u32 = 48;

pub const FX_GLASS_SHARD_LIFETIME_MSEC: i32 = 5000;

pub const FX_GLASS_SETTLED_LIFETIME_MSEC: i32 = 1500;

pub const FX_GLASS_SETTLED_FADE_MSEC: i32 = 250;

pub const FX_GLASS_SETTLED_CAP: u32 = 64;

pub const FX_GLASS_AIRBORNE_PER_BREAK: u32 = 64;

pub const FX_GLASS_FRINGE_MAX_PIECES: usize = 8;

pub const FX_GLASS_RESTITUTION: f32 = 0.15;

pub const FX_GLASS_MOTION_STEP_MSEC: i32 = 16;

pub const FX_GLASS_CATCHUP_STEPS: i32 = 4;

pub const FX_GLASS_PENDING_SUPPORT_FRAC: f32 = 0.25;

pub const FX_GLASS_PENDING_MIN_MSEC: i32 = 150;

pub const FX_GLASS_PENDING_MAX_MSEC: i32 = 1000;

pub const FX_GLASS_ACCENT_BOUNCE_CAP: u32 = 16;

pub const FX_GLASS_AIRBORNE_CAP: u32 = 256;

pub const FX_GLASS_SHATTER_FX_PER_FRAME: u32 = 6;

pub const FX_GLASS_LANDING_CELL: f32 = 64.0;

pub const FX_GLASS_LANDING_AGGREGATE_MSEC: i32 = 100;

pub const FX_GLASS_STATE_FLAG_DAMAGED: u16 = 1;

pub const FX_GLASS_STATE_FLAG_SHATTERED: u16 = 2;

pub const FX_GLASS_STATE_FLAG_CHILD_CLEAR: u16 = 4;

pub const FX_GLASS_SHATTER_FX_32: &str = "code/glass_shatter_32x32";

pub const FX_GLASS_SHATTER_FX_64: &str = "code/glass_shatter_64x64";

pub const FX_GLASS_SHATTER_FX_PIECE: &str = "code/glass_shatter_piece";

pub fn fx_glass_shatter_fx_name(landing: bool) -> &'static str {
    if landing {
        FX_GLASS_SHATTER_FX_PIECE
    } else {
        FX_GLASS_SHATTER_FX_64
    }
}

pub fn fx_glass_shatter_fx_fallback(landing: bool) -> Option<&'static str> {
    if landing {
        None
    } else {
        Some(FX_GLASS_SHATTER_FX_32)
    }
}

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

#[derive(Clone, Copy, Debug)]
pub struct FxGlassRadialSplit {
    pub impact: [f32; 2],
    pub branch_n: u32,
    pub angle0: f32,
    pub ray_jitter: [f32; FX_GLASS_SPLIT_MAX_CHILDREN],
}

impl Default for FxGlassRadialSplit {
    fn default() -> Self {
        Self {
            impact: [0.0; 2],
            branch_n: 4,
            angle0: 0.0,
            ray_jitter: [0.5; FX_GLASS_SPLIT_MAX_CHILDREN],
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

pub fn fx_glass_centroid(verts: &[[i16; 2]]) -> [f32; 2] {
    if verts.is_empty() {
        return [0.0, 0.0];
    }
    let n = verts.len() as f32;
    let mut x = 0.0;
    let mut y = 0.0;
    for v in verts {
        x += v[0] as f32;
        y += v[1] as f32;
    }
    [x / n, y / n]
}

pub fn fx_glass_clamp_impact(verts: &[[i16; 2]], p: [f32; 2]) -> [f32; 2] {
    if verts.len() < 3 {
        return p;
    }
    if fx_glass_point_in_convex(verts, p) {
        return p;
    }
    let mut best = [verts[0][0] as f32, verts[0][1] as f32];
    let mut best_d2 = f32::INFINITY;
    for i in 0..verts.len() {
        let a = [verts[i][0] as f32, verts[i][1] as f32];
        let b = [
            verts[(i + 1) % verts.len()][0] as f32,
            verts[(i + 1) % verts.len()][1] as f32,
        ];
        let q = closest_on_segment(a, b, p);
        let dx = q[0] - p[0];
        let dy = q[1] - p[1];
        let d2 = dx * dx + dy * dy;
        if d2 < best_d2 {
            best_d2 = d2;
            best = q;
        }
    }
    let c = fx_glass_centroid(verts);
    [best[0] * 0.85 + c[0] * 0.15, best[1] * 0.85 + c[1] * 0.15]
}

fn closest_on_segment(a: [f32; 2], b: [f32; 2], p: [f32; 2]) -> [f32; 2] {
    let ab = [b[0] - a[0], b[1] - a[1]];
    let ap = [p[0] - a[0], p[1] - a[1]];
    let ab2 = ab[0] * ab[0] + ab[1] * ab[1];
    if ab2 <= 1e-8 {
        return a;
    }
    let t = (ap[0] * ab[0] + ap[1] * ab[1]) / ab2;
    let t = if t < 0.0 {
        0.0
    } else if t > 1.0 {
        1.0
    } else {
        t
    };
    [a[0] + ab[0] * t, a[1] + ab[1] * t]
}

pub fn fx_glass_shard_size_cap(original_area: f32, supported: bool) -> f32 {
    let base = if supported {
        FX_GLASS_FRINGE_MAXSIZE
    } else {
        FX_GLASS_SHARD_MAXSIZE
    };
    let floor = if FX_GLASS_MAX_PIECES_PER_FRAME > 0.0 {
        original_area / FX_GLASS_MAX_PIECES_PER_FRAME
    } else {
        0.0
    };
    if base < floor { floor } else { base }
}

pub fn fx_glass_needs_size_split(area_x2: f32, original_area: f32, supported: bool) -> bool {
    area_x2 > fx_glass_shard_size_cap(original_area, supported)
}

pub fn fx_glass_child_support(
    parent_verts: &[[i16; 2]],
    parent_support: u32,
    child_verts: &[[i16; 2]],
) -> u32 {
    if parent_support == 0 || parent_verts.len() < 2 || child_verts.len() < 2 {
        return 0;
    }
    let pn = parent_verts.len().min(32);
    let cn = child_verts.len().min(32);
    let mut out = 0u32;
    for i in 0..cn {
        let a = child_verts[i];
        let b = child_verts[(i + 1) % child_verts.len()];
        for e in 0..pn {
            if parent_support & (1u32 << e) == 0 {
                continue;
            }
            let pa = parent_verts[e];
            let pb = parent_verts[(e + 1) % parent_verts.len()];
            if on_parent_edge(pa, pb, a) && on_parent_edge(pa, pb, b) {
                out |= 1u32 << i;
                break;
            }
        }
    }
    out
}

fn on_parent_edge(a: [i16; 2], b: [i16; 2], p: [i16; 2]) -> bool {
    let abx = i32::from(b[0]) - i32::from(a[0]);
    let aby = i32::from(b[1]) - i32::from(a[1]);
    let apx = i32::from(p[0]) - i32::from(a[0]);
    let apy = i32::from(p[1]) - i32::from(a[1]);
    let cross = abx * apy - aby * apx;
    let abs_ab = abx.abs().max(aby.abs()).max(1);
    if cross.abs() > abs_ab {
        return false;
    }
    let min_x = i32::from(a[0].min(b[0])) - 1;
    let max_x = i32::from(a[0].max(b[0])) + 1;
    let min_y = i32::from(a[1].min(b[1])) - 1;
    let max_y = i32::from(a[1].max(b[1])) + 1;
    let px = i32::from(p[0]);
    let py = i32::from(p[1]);
    px >= min_x && px <= max_x && py >= min_y && py <= max_y
}

pub fn fx_glass_fringe_prune_knock_order(
    areas: &[f32],
    supports: &[u32],
) -> [u8; FX_GLASS_SPLIT_QUEUE_CAP] {
    let mut idx = [0u8; FX_GLASS_SPLIT_QUEUE_CAP];
    let mut n = 0usize;
    let len = areas
        .len()
        .min(supports.len())
        .min(FX_GLASS_SPLIT_QUEUE_CAP);
    for (i, support) in supports.iter().take(len).enumerate() {
        if *support != 0 {
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

pub fn fx_glass_life_fade(age_msec: i32, life_msec: i32, fade_msec: i32) -> f32 {
    if life_msec <= 0 {
        return 0.0;
    }
    let age = age_msec.max(0);
    if age >= life_msec {
        return 0.0;
    }
    if fade_msec <= 0 {
        return 1.0;
    }
    let remain = life_msec - age;
    if remain >= fade_msec {
        return 1.0;
    }
    remain as f32 / fade_msec as f32
}

pub fn fx_glass_scale_color_alpha(rgba: [u8; 4], fade: f32) -> [u8; 4] {
    let fade = if fade < 0.0 {
        0.0
    } else if fade > 1.0 {
        1.0
    } else {
        fade
    };
    let a = (rgba[3] as f32 * fade) as u8;
    [rgba[0], rgba[1], rgba[2], a]
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
    args: FxGlassRadialSplit,
) -> [FxGlassSplitLoop; FX_GLASS_SPLIT_MAX_CHILDREN] {
    let mut out = [FxGlassSplitLoop::default(); FX_GLASS_SPLIT_MAX_CHILDREN];
    if verts.len() < 3 || !(3..=6).contains(&args.branch_n) {
        return out;
    }
    let impact = fx_glass_clamp_impact(verts, args.impact);
    if !fx_glass_point_in_convex(verts, impact) {
        return out;
    }
    let n = args.branch_n as usize;
    let step = fx_glass_interior_angle_step(args.branch_n);
    let mut hits = [(0usize, 0.0f32, [0.0f32; 2]); FX_GLASS_SPLIT_MAX_CHILDREN];
    let mut hit_n = 0usize;
    for k in 0..n {
        let jitter = args.ray_jitter.get(k).copied().unwrap_or(0.5);
        let ang = args.angle0 + step * (k as f32 + (jitter - 0.5) * 0.5);
        let dir = [libm::cosf(ang), libm::sinf(ang)];
        let Some(hit) = closest_forward_hit(verts, impact, dir) else {
            continue;
        };
        hits[hit_n] = hit;
        hit_n += 1;
    }
    if hit_n < 2 {
        return [FxGlassSplitLoop::default(); FX_GLASS_SPLIT_MAX_CHILDREN];
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

pub fn fx_glass_normalize3(v: [f32; 3]) -> Option<[f32; 3]> {
    let len_sq = v[0] * v[0] + v[1] * v[1] + v[2] * v[2];
    if len_sq <= 1e-12 {
        return None;
    }
    let inv = 1.0 / libm::sqrtf(len_sq);
    Some([v[0] * inv, v[1] * inv, v[2] * inv])
}

pub fn fx_glass_cross3(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}

pub fn fx_glass_launch_dir(dir: [f32; 3], pane_normal: [f32; 3]) -> [f32; 3] {
    fx_glass_normalize3(dir)
        .or_else(|| fx_glass_normalize3(pane_normal))
        .unwrap_or([0.0, 0.0, 1.0])
}

pub fn fx_glass_splitmix64(state: &mut u64) -> f32 {
    *state = state.wrapping_add(0x9E37_79B9_7F4A_7C15);
    let mut z = *state;
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^= z >> 31;
    (z >> 40) as f32 * (1.0 / 16_777_216.0)
}

pub fn fx_glass_recenter_loop(
    verts: &mut [[i16; 2]],
    vert_n: usize,
    origin: [f32; 3],
    axis: [[f32; 3]; 3],
    uv: [f32; 2],
    tex: [[f32; 2]; 2],
) -> ([f32; 3], [f32; 2]) {
    let c = fx_glass_centroid(&verts[..vert_n]);
    for v in verts.iter_mut().take(vert_n) {
        v[0] = libm::roundf(v[0] as f32 - c[0]) as i16;
        v[1] = libm::roundf(v[1] as f32 - c[1]) as i16;
    }
    let scale = crate::glass::FX_GLASS_VERT_SCALE;
    let new_origin = [
        origin[0] + axis[0][0] * c[0] * scale + axis[1][0] * c[1] * scale,
        origin[1] + axis[0][1] * c[0] * scale + axis[1][1] * c[1] * scale,
        origin[2] + axis[0][2] * c[0] * scale + axis[1][2] * c[1] * scale,
    ];
    let new_uv = [
        uv[0] + tex[0][0] * c[0] + tex[0][1] * c[1],
        uv[1] + tex[1][0] * c[0] + tex[1][1] * c[1],
    ];
    (new_origin, new_uv)
}

pub fn fx_glass_chord_split(
    verts: &[[i16; 2]],
    rand01: f32,
    along_x: bool,
) -> [FxGlassSplitLoop; 2] {
    let mut out = [FxGlassSplitLoop::default(); 2];
    if verts.len() < 3 {
        return out;
    }
    let mut min_a = f32::INFINITY;
    let mut max_a = f32::NEG_INFINITY;
    for v in verts {
        let a = if along_x { v[0] as f32 } else { v[1] as f32 };
        min_a = min_a.min(a);
        max_a = max_a.max(a);
    }
    let span = max_a - min_a;
    if span <= 4.0 {
        return out;
    }
    let t = 0.3 + rand01.clamp(0.0, 1.0) * 0.4;
    let cut = min_a + span * t;
    let mut left = FxGlassSplitLoop::default();
    let mut right = FxGlassSplitLoop::default();
    for i in 0..verts.len() {
        let a = verts[i];
        let b = verts[(i + 1) % verts.len()];
        let a_val = if along_x { a[0] as f32 } else { a[1] as f32 };
        let b_val = if along_x { b[0] as f32 } else { b[1] as f32 };
        let a_left = a_val <= cut;
        if a_left {
            let _ = push_unique(&mut left, a);
        } else {
            let _ = push_unique(&mut right, a);
        }
        if (a_val - cut) * (b_val - cut) < 0.0 {
            let denom = b_val - a_val;
            let s = if denom.abs() <= 1e-6 {
                0.5
            } else {
                (cut - a_val) / denom
            };
            let hit = [
                libm::roundf(a[0] as f32 + (b[0] as f32 - a[0] as f32) * s) as i16,
                libm::roundf(a[1] as f32 + (b[1] as f32 - a[1] as f32) * s) as i16,
            ];
            let _ = push_unique(&mut left, hit);
            let _ = push_unique(&mut right, hit);
        }
    }
    if left.vert_n >= 3 {
        left.original_edges =
            fx_glass_child_support(verts, !0u32, &left.verts[..left.vert_n as usize]);
        out[0] = left;
    }
    if right.vert_n >= 3 {
        right.original_edges =
            fx_glass_child_support(verts, !0u32, &right.verts[..right.vert_n as usize]);
        out[1] = right;
    }
    out
}

pub fn fx_glass_support_frac(support_mask: u32, vert_n: u8) -> f32 {
    let edges = vert_n.max(1);
    support_mask.count_ones() as f32 / f32::from(edges)
}

pub fn fx_glass_launch_avel(dir: [f32; 3], pane_axis: [[f32; 3]; 3], ang: f32) -> [f32; 3] {
    let spin = fx_glass_cross3(dir, pane_axis[2]);
    if let Some(axis) = fx_glass_normalize3(spin) {
        [axis[0] * ang, axis[1] * ang, axis[2] * ang]
    } else {
        [
            pane_axis[0][0] * ang,
            pane_axis[0][1] * ang,
            pane_axis[0][2] * ang,
        ]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn square(half: i16) -> [[i16; 2]; 4] {
        [[-half, -half], [half, -half], [half, half], [-half, half]]
    }

    fn child_count(loops: &[FxGlassSplitLoop; FX_GLASS_SPLIT_MAX_CHILDREN]) -> usize {
        loops.iter().filter(|l| l.vert_n >= 3).count()
    }

    fn child_area_sum(loops: &[FxGlassSplitLoop; FX_GLASS_SPLIT_MAX_CHILDREN]) -> f32 {
        loops
            .iter()
            .filter(|l| l.vert_n >= 3)
            .map(|l| fx_glass_loop_area_x2(&l.verts[..l.vert_n as usize]))
            .sum()
    }

    #[test]
    fn branch_count_is_three_to_six() {
        assert_eq!(fx_glass_interior_branch_count(0.0), 3);
        assert_eq!(fx_glass_interior_branch_count(1.0), 6);
        assert_eq!(fx_glass_interior_branch_count(0.5), 5);
    }

    #[test]
    fn radial_split_covers_square() {
        let verts = square(160);
        let original = fx_glass_loop_area_x2(&verts);
        let loops = fx_glass_radial_split(
            &verts,
            FxGlassRadialSplit {
                impact: [0.0, 0.0],
                branch_n: 4,
                ..FxGlassRadialSplit::default()
            },
        );
        assert_eq!(child_count(&loops), 4);
        let sum = child_area_sum(&loops);
        assert!(
            (sum - original).abs() / original < 0.05,
            "{sum} vs {original}"
        );
    }

    #[test]
    fn impact_outside_clamps_and_still_splits() {
        let verts = square(160);
        let loops = fx_glass_radial_split(
            &verts,
            FxGlassRadialSplit {
                impact: [10_000.0, 10_000.0],
                branch_n: 3,
                ..FxGlassRadialSplit::default()
            },
        );
        assert!(child_count(&loops) >= 3);
    }

    #[test]
    fn one_missed_axis_aligned_ray_does_not_zero_the_split() {
        let verts = square(160);
        let loops = fx_glass_radial_split(
            &verts,
            FxGlassRadialSplit {
                impact: [0.0, 0.0],
                branch_n: 5,
                angle0: 0.1,
                ray_jitter: [0.0, 0.5, 0.5, 0.5, 0.5, 0.5],
            },
        );
        assert!(child_count(&loops) >= 2);
    }

    #[test]
    fn size_cap_raises_for_huge_panes() {
        let huge = 50_000.0;
        assert!(fx_glass_shard_size_cap(huge, false) > FX_GLASS_SHARD_MAXSIZE);
        assert!(fx_glass_needs_size_split(400.0, 200.0, false));
        assert!(!fx_glass_needs_size_split(100.0, 200.0, false));
        assert!(fx_glass_needs_size_split(200.0, 200.0, true));
    }

    #[test]
    fn launch_dir_falls_back_to_pane_normal() {
        let d = fx_glass_launch_dir([0.0, 0.0, 0.0], [0.0, 0.0, 1.0]);
        assert!((d[2] - 1.0).abs() < 1e-5);
        let n = fx_glass_launch_dir([3.0, 0.0, 0.0], [0.0, 0.0, 1.0]);
        assert!((n[0] - 1.0).abs() < 1e-5);
    }

    #[test]
    fn chord_split_does_not_share_one_vertex() {
        let verts = square(160);
        let loops = fx_glass_chord_split(&verts, 0.4, true);
        assert!(loops[0].vert_n >= 3 && loops[1].vert_n >= 3);
        let a0 = loops[0].verts[0];
        let shared = (0..loops[1].vert_n as usize).all(|i| loops[1].verts[i] == a0);
        assert!(!shared);
        let parent = fx_glass_loop_area_x2(&verts);
        let kids = fx_glass_loop_area_x2(&loops[0].verts[..loops[0].vert_n as usize])
            + fx_glass_loop_area_x2(&loops[1].verts[..loops[1].vert_n as usize]);
        assert!((parent - kids).abs() < parent * 0.15);
    }

    #[test]
    fn recenter_moves_origin_not_uv_scale() {
        let mut verts = square(160);
        let (origin, uv) = fx_glass_recenter_loop(
            &mut verts,
            4,
            [10.0, 20.0, 30.0],
            [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]],
            [0.5, 0.5],
            [[0.01, 0.0], [0.0, 0.01]],
        );
        assert!((origin[0] - 10.0).abs() < 1e-3);
        assert!((origin[1] - 20.0).abs() < 1e-3);
        let c = fx_glass_centroid(&verts);
        assert!(c[0].abs() < 2.0 && c[1].abs() < 2.0);
        assert!((uv[0] - 0.5).abs() < 1e-3);
        assert!((uv[1] - 0.5).abs() < 1e-3);
    }

    #[test]
    fn recenter_applies_tex_matrix_to_uv() {
        let mut verts = [[40i16, -40], [160, -40], [160, 40], [40, 40]];
        let (origin, uv) = fx_glass_recenter_loop(
            &mut verts,
            4,
            [0.0, 0.0, 0.0],
            [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]],
            [0.0, 0.0],
            [[0.01, 0.0], [0.0, 0.02]],
        );
        assert!(origin[0] > 0.0);
        assert!(uv[0] > 0.0);
        assert!(uv[1].abs() < 1e-3);
    }

    #[test]
    fn clamp_projects_outside_hit_onto_the_boundary() {
        let verts = square(160);
        let p = fx_glass_clamp_impact(&verts, [10_000.0, 0.0]);
        assert!(p[0] > 80.0 && p[0] <= 160.0);
        assert!(p[1].abs() < 20.0);
        let inside = fx_glass_clamp_impact(&verts, [10.0, -20.0]);
        assert!((inside[0] - 10.0).abs() < 1e-3);
        assert!((inside[1] + 20.0).abs() < 1e-3);
    }

    #[test]
    fn child_support_keeps_parent_edge_bits_in_child_space() {
        let parent = square(160);
        let child = [[-160i16, -160], [160, -160], [160, 160]];
        let mask = fx_glass_child_support(&parent, 0b1111, &child);
        assert_ne!(mask, 0);
        assert!(
            mask.count_ones() >= 2,
            "supported parent edges must map onto child edges, got {mask:#b}"
        );
        assert_ne!(mask, 1, "must not collapse every supported child to bit 0");
        let none = fx_glass_child_support(&parent, 0, &child);
        assert_eq!(none, 0);
        let bottom_only = fx_glass_child_support(&parent, 0b0001, &child);
        assert_ne!(bottom_only, 0);
        let top_only = fx_glass_child_support(&parent, 0b0100, &child);
        assert_eq!(top_only, 0);
    }

    #[test]
    fn settled_fade_is_one_until_the_last_quarter_second() {
        assert_eq!(
            fx_glass_life_fade(
                0,
                FX_GLASS_SETTLED_LIFETIME_MSEC,
                FX_GLASS_SETTLED_FADE_MSEC
            ),
            1.0
        );
        assert_eq!(
            fx_glass_life_fade(
                FX_GLASS_SETTLED_LIFETIME_MSEC - FX_GLASS_SETTLED_FADE_MSEC,
                FX_GLASS_SETTLED_LIFETIME_MSEC,
                FX_GLASS_SETTLED_FADE_MSEC
            ),
            1.0
        );
        let mid = fx_glass_life_fade(
            FX_GLASS_SETTLED_LIFETIME_MSEC - FX_GLASS_SETTLED_FADE_MSEC / 2,
            FX_GLASS_SETTLED_LIFETIME_MSEC,
            FX_GLASS_SETTLED_FADE_MSEC,
        );
        assert!((mid - 0.5).abs() < 1e-5, "{mid}");
        assert_eq!(
            fx_glass_life_fade(
                FX_GLASS_SETTLED_LIFETIME_MSEC,
                FX_GLASS_SETTLED_LIFETIME_MSEC,
                FX_GLASS_SETTLED_FADE_MSEC
            ),
            0.0
        );
        let faded = fx_glass_scale_color_alpha([10, 20, 30, 200], 0.5);
        assert_eq!(faded, [10, 20, 30, 100]);
    }
}
