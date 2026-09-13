pub const COM_CONVEX_HULL_MAX: usize = 64;

const FRONT_EPS: f32 = 0.001;

const BACK_EPS: f32 = -0.001;

fn vec2_normalize(v: &mut [f32; 2]) {
    let mut len = libm::sqrtf(v[0] * v[0] + v[1] * v[1]);
    if 0.0 <= -len {
        len = 1.0;
    }
    let inv = 1.0 / len;
    v[0] *= inv;
    v[1] *= inv;
}

fn swap_u32(order: &mut [u32], a: usize, b: usize) {
    order.swap(a, b);
}

fn add_point_to_hull(
    point_index: u32,
    new_index: usize,
    hull_order: &mut [u32],
    hull_point_count: usize,
) -> usize {
    for i in (new_index..hull_point_count).rev() {
        hull_order[i + 1] = hull_order[i];
    }
    hull_order[new_index] = point_index;
    hull_point_count + 1
}

fn initial_hull(
    pts: &[[f32; 2]],
    n: usize,
    point_order: &mut [u32],
    hull_order: &mut [u32],
) -> bool {
    let mut min_index = 0usize;
    let mut max_index = 0usize;
    point_order[0] = 0;
    for i in 1..n {
        point_order[i] = i as u32;
        if pts[i][1] < pts[max_index][1] {
            if pts[min_index][1] > pts[i][1] {
                min_index = i;
            }
        } else {
            max_index = i;
        }
    }
    if min_index == max_index {
        return false;
    }
    hull_order[0] = min_index as u32;
    hull_order[1] = max_index as u32;
    if min_index <= max_index {
        swap_u32(point_order, max_index, n - 1);
        swap_u32(point_order, min_index, n - 2);
    } else {
        swap_u32(point_order, min_index, n - 1);
        swap_u32(point_order, max_index, n - 2);
    }
    true
}

fn hull_pt(pts: &[[f32; 2]], hull_order: &[u32], i: usize) -> [f32; 2] {
    pts[hull_order[i] as usize]
}

fn recursively_grow_hull(
    pts: &[[f32; 2]],
    point_order: &mut [u32],
    point_count: usize,
    first_index: usize,
    mut second_index: usize,
    hull_order: &mut [u32],
    mut hull_point_count: usize,
) -> usize {
    if point_count == 0 {
        return hull_point_count;
    }
    let mut edge = [
        hull_pt(pts, hull_order, first_index)[1] - hull_pt(pts, hull_order, second_index)[1],
        hull_pt(pts, hull_order, second_index)[0] - hull_pt(pts, hull_order, first_index)[0],
    ];
    vec2_normalize(&mut edge);
    let a = hull_pt(pts, hull_order, first_index);
    let edge_d = a[0] * edge[0] + a[1] * edge[1];
    let mut bot = 0isize;
    let mut top = point_count as isize - 1;
    let mut front_dist = FRONT_EPS;
    let mut front_index: isize = -1;
    while bot <= top {
        loop {
            let i = point_order[bot as usize] as usize;
            let dist = pts[i][0] * edge[0] + pts[i][1] * edge[1] - edge_d;
            if dist <= 0.0 {
                break;
            }
            if dist > front_dist {
                front_dist = dist;
                front_index = bot;
            }
            bot += 1;
            if bot > top {
                break;
            }
        }
        if bot > top {
            break;
        }
        loop {
            let i = point_order[top as usize] as usize;
            let dist = pts[i][0] * edge[0] + pts[i][1] * edge[1] - edge_d;
            if dist > 0.0 {
                if dist > front_dist {
                    front_dist = dist;
                    front_index = bot;
                }
                if bot < top {
                    swap_u32(point_order, bot as usize, top as usize);
                }
                bot += 1;
                top -= 1;
                break;
            }
            top -= 1;
            if bot > top {
                break;
            }
        }
    }
    if front_index < 0 {
        return hull_point_count;
    }
    let top_u = top as usize;
    swap_u32(point_order, front_index as usize, top_u);
    hull_point_count = add_point_to_hull(
        point_order[top_u],
        first_index + 1,
        hull_order,
        hull_point_count,
    );
    if top_u == 0 {
        return hull_point_count;
    }
    if second_index != 0 {
        second_index = first_index + 2;
    }
    let grown = recursively_grow_hull(
        pts,
        point_order,
        top_u,
        first_index + 1,
        second_index,
        hull_order,
        hull_point_count,
    );
    recursively_grow_hull(
        pts,
        point_order,
        top_u,
        first_index,
        first_index + 1,
        hull_order,
        grown,
    )
}

fn grow_initial_hull(
    pts: &[[f32; 2]],
    point_order: &mut [u32],
    point_count: usize,
    hull_order: &mut [u32],
) -> usize {
    if point_count == 0 {
        return 0;
    }
    let mut edge = [
        hull_pt(pts, hull_order, 1)[1] - hull_pt(pts, hull_order, 0)[1],
        hull_pt(pts, hull_order, 0)[0] - hull_pt(pts, hull_order, 1)[0],
    ];
    vec2_normalize(&mut edge);
    let a = hull_pt(pts, hull_order, 0);
    let edge_d = a[0] * edge[0] + a[1] * edge[1];
    let mut bot = 0isize;
    let mut top = point_count as isize - 1;
    let mut front_dist = FRONT_EPS;
    let mut front_index: isize = -1;
    let mut back_dist = BACK_EPS;
    let mut back_index: isize = -1;
    while bot <= top {
        loop {
            let i = point_order[bot as usize] as usize;
            let dist = pts[i][0] * edge[0] + pts[i][1] * edge[1] - edge_d;
            if dist < 0.0 {
                if dist < back_dist {
                    back_dist = dist;
                    back_index = bot;
                }
                break;
            }
            if dist > front_dist {
                front_dist = dist;
                front_index = bot;
            }
            bot += 1;
            if bot > top {
                break;
            }
        }
        if bot > top {
            break;
        }
        loop {
            let i = point_order[top as usize] as usize;
            let dist = pts[i][0] * edge[0] + pts[i][1] * edge[1] - edge_d;
            if dist > 0.0 {
                if dist > front_dist {
                    front_dist = dist;
                    front_index = bot;
                }
                if bot >= top {
                    break;
                }
                swap_u32(point_order, bot as usize, top as usize);
                if back_index == bot {
                    back_index = top;
                }
                bot += 1;
                top -= 1;
                break;
            }
            if dist < back_dist {
                back_dist = dist;
                back_index = top;
            }
            top -= 1;
            if bot > top {
                break;
            }
        }
    }
    if front_index < 0 && back_index < 0 {
        return 0;
    }
    let mut hull_point_count = 2usize;
    let top_u = top as usize;
    if front_index >= 0 {
        swap_u32(point_order, front_index as usize, top_u);
        hull_point_count = add_point_to_hull(point_order[top_u], 2, hull_order, hull_point_count);
        if top_u > 0 {
            hull_point_count =
                recursively_grow_hull(pts, point_order, top_u, 2, 0, hull_order, hull_point_count);
            hull_point_count =
                recursively_grow_hull(pts, point_order, top_u, 1, 2, hull_order, hull_point_count);
        }
    }
    if back_index >= 0 {
        swap_u32(point_order, back_index as usize, bot as usize);
        hull_point_count =
            add_point_to_hull(point_order[bot as usize], 1, hull_order, hull_point_count);
        let rest = (point_count as isize - bot - 1).max(0) as usize;
        if rest != 0 {
            let start = bot as usize + 1;
            let grown = recursively_grow_hull(
                pts,
                &mut point_order[start..],
                rest,
                1,
                2,
                hull_order,
                hull_point_count,
            );
            hull_point_count = recursively_grow_hull(
                pts,
                &mut point_order[start..],
                rest,
                0,
                1,
                hull_order,
                grown,
            );
        }
    }
    hull_point_count
}

pub fn com_convex_hull(points: &[[f32; 2]], hull: &mut [[f32; 2]]) -> usize {
    let n = points.len();
    if !(3..=COM_CONVEX_HULL_MAX).contains(&n) || hull.is_empty() {
        return 0;
    }
    let mut pts = [[0.0f32; 2]; COM_CONVEX_HULL_MAX];
    pts[..n].copy_from_slice(points);
    let offset = [-pts[0][0], -pts[0][1]];
    for p in pts.iter_mut().take(n) {
        p[0] += offset[0];
        p[1] += offset[1];
    }
    let mut point_order = [0u32; COM_CONVEX_HULL_MAX];
    let mut hull_order = [0u32; COM_CONVEX_HULL_MAX];
    if !initial_hull(&pts, n, &mut point_order, &mut hull_order) {
        return 0;
    }
    let hull_n = grow_initial_hull(&pts, &mut point_order, n - 2, &mut hull_order);
    let out_n = hull_n.min(hull.len());
    for i in 0..out_n {
        let p = pts[hull_order[i] as usize];
        hull[i] = [p[0] - offset[0], p[1] - offset[1]];
    }
    out_n
}

#[derive(Clone, Copy, Debug)]
pub struct PortalHullPoints {
    pub points: [[f32; 2]; COM_CONVEX_HULL_MAX],
    pub count: u8,
}

impl PortalHullPoints {
    pub const EMPTY: Self = Self {
        points: [[0.0; 2]; COM_CONVEX_HULL_MAX],
        count: 0,
    };
}

pub fn add_vert_to_portal_hull_points(hull: &mut PortalHullPoints, uv: [f32; 2]) -> bool {
    let mut n = usize::from(hull.count);
    if n == COM_CONVEX_HULL_MAX {
        let mut compact = [[0.0f32; 2]; COM_CONVEX_HULL_MAX];
        let h = com_convex_hull(&hull.points[..n], &mut compact);
        if h == COM_CONVEX_HULL_MAX || h == 0 {
            if h == COM_CONVEX_HULL_MAX {
                return false;
            }
            hull.count = 0;
            n = 0;
        } else {
            hull.points[..h].copy_from_slice(&compact[..h]);
            hull.count = h as u8;
            n = h;
        }
    }
    if n >= COM_CONVEX_HULL_MAX {
        return false;
    }
    hull.points[n] = uv;
    hull.count = (n + 1) as u8;
    true
}
