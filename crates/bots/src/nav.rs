use std::cmp::Reverse;
use std::collections::BinaryHeap;

use sim::SimBrush;

use crate::query::{WalkSample, WorldQuery};

pub const NAV_SCHEMA: u32 = 4;
pub const GRID_IN: f32 = 48.0;
pub const SNAP_IN: f32 = 256.0;
pub const STEP_Z_IN: f32 = 18.0;
const DROP_Z_MAX: f32 = 192.0;
/// Stand-hull walk bake: grid, snap, step height. Part of the cache key.
pub const NAV_HULL: u32 = ((GRID_IN as u32) << 16) ^ ((SNAP_IN as u32) << 8) ^ (STEP_Z_IN as u32);
pub const EXPAND_CAP: u32 = 2048;
pub const SEED_PAD_IN: f32 = 768.0;
const CELL_CAP: u32 = 6400;
const DEDUPE_IN: f32 = 8.0;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PathError {
    EmptyGraph,
    Unreachable,
    BudgetExhausted { expanded: u32 },
}

#[derive(Clone, Debug, Default)]
pub struct NavGraph {
    pub digest: u64,
    pub schema: u32,
    pub hull: u32,
    pub nodes: Vec<[f32; 3]>,
    pub component: Vec<u16>,
    pub adj: Vec<Vec<(u16, f32)>>,
    pub drops: u32,
}

impl NavGraph {
    pub fn is_empty(&self) -> bool {
        self.nodes.is_empty()
    }
}

pub fn brush_bounds(brushes: &[SimBrush]) -> Option<([f32; 3], [f32; 3])> {
    let mut mins = [f32::MAX; 3];
    let mut maxs = [f32::MIN; 3];
    let mut any = false;
    for brush in brushes {
        let Some((bmin, bmax)) = aabb_of(brush) else {
            continue;
        };
        any = true;
        for i in 0..3 {
            mins[i] = mins[i].min(bmin[i]);
            maxs[i] = maxs[i].max(bmax[i]);
        }
    }
    any.then_some((mins, maxs))
}

pub fn playable_bounds(brushes: &[SimBrush], seeds: &[[f32; 3]]) -> Option<([f32; 3], [f32; 3])> {
    let (bmins, bmaxs) = brush_bounds(brushes)?;
    if seeds.is_empty() {
        return Some((bmins, bmaxs));
    }
    let mut mins = [f32::MAX; 3];
    let mut maxs = [f32::MIN; 3];
    for seed in seeds {
        for i in 0..3 {
            mins[i] = mins[i].min(seed[i]);
            maxs[i] = maxs[i].max(seed[i]);
        }
    }
    mins[0] -= SEED_PAD_IN;
    mins[1] -= SEED_PAD_IN;
    mins[2] -= 128.0;
    maxs[0] += SEED_PAD_IN;
    maxs[1] += SEED_PAD_IN;
    maxs[2] += 256.0;
    Some((
        [
            mins[0].max(bmins[0]),
            mins[1].max(bmins[1]),
            mins[2].max(bmins[2]),
        ],
        [
            maxs[0].min(bmaxs[0]),
            maxs[1].min(bmaxs[1]),
            maxs[2].min(bmaxs[2]),
        ],
    ))
}

fn aabb_of(brush: &SimBrush) -> Option<([f32; 3], [f32; 3])> {
    let mut mins = [f32::MAX; 3];
    let mut maxs = [f32::MIN; 3];
    let mut axes = [false; 3];
    for plane in &brush.planes {
        let axis = if plane[0].abs() > 0.9 {
            0
        } else if plane[1].abs() > 0.9 {
            1
        } else if plane[2].abs() > 0.9 {
            2
        } else {
            continue;
        };
        axes[axis] = true;
        let dist = plane[3] / plane[axis];
        if plane[axis] > 0.0 {
            maxs[axis] = maxs[axis].max(dist);
        } else {
            mins[axis] = mins[axis].min(dist);
        }
    }
    (axes[0] && axes[1] && axes[2]).then_some((mins, maxs))
}

pub fn bake(world: &mut impl WorldQuery, bounds: ([f32; 3], [f32; 3]), digest: u64) -> NavGraph {
    bake_seeded(world, bounds, digest, &[])
}

pub fn bake_seeded(
    world: &mut impl WorldQuery,
    bounds: ([f32; 3], [f32; 3]),
    digest: u64,
    seeds: &[[f32; 3]],
) -> NavGraph {
    let (mins, maxs) = bounds;
    let mut step = GRID_IN;
    let span_x = (maxs[0] - mins[0]).max(step);
    let span_y = (maxs[1] - mins[1]).max(step);
    let cells_x = (span_x / step).ceil() as u32;
    let cells_y = (span_y / step).ceil() as u32;
    if cells_x.saturating_mul(cells_y) > CELL_CAP {
        let cells = ((span_x * span_y) / CELL_CAP as f32).sqrt().max(step);
        step = cells;
    }
    let mut nodes = Vec::new();
    let mut xs = Vec::new();
    let mut x = mins[0];
    while x <= maxs[0] + 1.0 {
        xs.push(x);
        x += step;
    }
    let mut ys = Vec::new();
    let mut y = mins[1];
    while y <= maxs[1] + 1.0 {
        ys.push(y);
        y += step;
    }
    let z_top = maxs[2] + 72.0;
    let z_bot = mins[2] - 8.0;
    for &x in &xs {
        for &y in &ys {
            push_column(world, &mut nodes, [x, y, 0.0], z_top, z_bot);
        }
    }
    for seed in seeds {
        push_column(world, &mut nodes, *seed, z_top, z_bot);
    }
    let n = nodes.len();
    let mut adj = vec![Vec::new(); n];
    for i in 0..n {
        for j in (i + 1)..n {
            let a = nodes[i];
            let b = nodes[j];
            let dx = a[0] - b[0];
            let dy = a[1] - b[1];
            let planar = (dx * dx + dy * dy).sqrt();
            if planar > step * 1.6 || planar < 1.0 {
                continue;
            }
            let dz = (a[2] - b[2]).abs();
            if dz > STEP_Z_IN {
                continue;
            }
            if world.walk_hull(a, b) != WalkSample::Clear {
                continue;
            }
            let cost = planar + dz;
            adj[i].push((j as u16, cost));
            adj[j].push((i as u16, cost));
        }
    }
    let component = label_components(n, &adj);
    let drops = link_drops(world, &nodes, &mut adj, step);
    NavGraph {
        digest,
        schema: NAV_SCHEMA,
        hull: NAV_HULL,
        nodes,
        component,
        adj,
        drops,
    }
}

fn push_column(
    world: &mut impl WorldQuery,
    nodes: &mut Vec<[f32; 3]>,
    at: [f32; 3],
    z_top: f32,
    z_bot: f32,
) {
    let drop = world.hull_trace([at[0], at[1], z_top], [at[0], at[1], z_bot]);
    if drop.startsolid || drop.fraction >= 1.0 {
        return;
    }
    let feet = drop.endpos;
    let stand = world.hull_trace(feet, feet);
    if stand.startsolid {
        return;
    }
    if nodes.iter().any(|node| dist_xy(*node, feet) < DEDUPE_IN) {
        return;
    }
    nodes.push(feet);
}

fn dist_xy(a: [f32; 3], b: [f32; 3]) -> f32 {
    let dx = a[0] - b[0];
    let dy = a[1] - b[1];
    (dx * dx + dy * dy).sqrt()
}

fn link_drops(
    world: &mut impl WorldQuery,
    nodes: &[[f32; 3]],
    adj: &mut [Vec<(u16, f32)>],
    step: f32,
) -> u32 {
    let n = nodes.len();
    let planar_max = step * 2.0;
    let mut drops = 0u32;
    for i in 0..n {
        for j in 0..n {
            if i == j {
                continue;
            }
            let a = nodes[i];
            let b = nodes[j];
            let planar = dist_xy(a, b);
            if planar > planar_max || planar < 1.0 {
                continue;
            }
            let dz = a[2] - b[2];
            if dz <= STEP_Z_IN || dz > DROP_Z_MAX {
                continue;
            }
            if adj[i].iter().any(|&(k, _)| k as usize == j) {
                continue;
            }
            let horiz = world.hull_trace([a[0], a[1], a[2] + 2.0], [b[0], b[1], a[2] + 2.0]);
            if horiz.startsolid {
                continue;
            }
            let from = if horiz.fraction >= 1.0 {
                [b[0], b[1], a[2] + 2.0]
            } else {
                horiz.endpos
            };
            let land = world.hull_trace(from, [from[0], from[1], b[2] - 8.0]);
            if land.startsolid || land.fraction >= 1.0 {
                continue;
            }
            if dist_xy(land.endpos, b) > planar_max {
                continue;
            }
            if (land.endpos[2] - b[2]).abs() > STEP_Z_IN {
                continue;
            }
            adj[i].push((j as u16, planar + dz));
            drops += 1;
        }
    }
    drops
}

fn label_components(n: usize, adj: &[Vec<(u16, f32)>]) -> Vec<u16> {
    let mut component = vec![u16::MAX; n];
    let mut next = 0u16;
    for start in 0..n {
        if component[start] != u16::MAX {
            continue;
        }
        let mut stack = vec![start];
        component[start] = next;
        while let Some(i) = stack.pop() {
            for &(j, _) in &adj[i] {
                let j = j as usize;
                if component[j] == u16::MAX {
                    component[j] = next;
                    stack.push(j);
                }
            }
        }
        next = next.saturating_add(1);
    }
    component
}

pub fn nearest_within(graph: &NavGraph, feet: [f32; 3], max_dist: f32) -> Option<u16> {
    let max_d2 = max_dist * max_dist;
    let mut best: Option<(u16, f32)> = None;
    for (i, pos) in graph.nodes.iter().enumerate() {
        let dx = pos[0] - feet[0];
        let dy = pos[1] - feet[1];
        let dz = pos[2] - feet[2];
        let d2 = dx * dx + dy * dy + dz * dz;
        if d2 > max_d2 {
            continue;
        }
        if best.is_none_or(|(_, bd)| d2 < bd) {
            best = Some((i as u16, d2));
        }
    }
    best.map(|(id, _)| id)
}

pub fn roam_node(graph: &NavGraph, from: [f32; 3], salt: u32) -> Option<[f32; 3]> {
    let start = nearest_within(graph, from, SNAP_IN)?;
    let comp = graph.component[start as usize];
    let mut best: Option<(u32, [f32; 3])> = None;
    let mut fallback = None;
    for (i, pos) in graph.nodes.iter().enumerate() {
        if graph.component[i] != comp {
            continue;
        }
        let planar = dist_xy(*pos, from);
        if planar > 8.0 && fallback.is_none() {
            fallback = Some(*pos);
        }
        if planar < 128.0 {
            continue;
        }
        let h = (i as u32).wrapping_mul(0x9E37_79B1) ^ salt;
        if best.is_none_or(|(best_h, _)| h >= best_h) {
            best = Some((h, *pos));
        }
    }
    best.map(|(_, pos)| pos).or(fallback)
}

pub fn find_path(
    graph: &NavGraph,
    start_feet: [f32; 3],
    goal_feet: [f32; 3],
    expand_budget: &mut u32,
) -> Result<Vec<[f32; 3]>, PathError> {
    if graph.nodes.is_empty() {
        return Err(PathError::EmptyGraph);
    }
    let start = nearest_within(graph, start_feet, SNAP_IN).ok_or(PathError::Unreachable)?;
    let goal = nearest_within(graph, goal_feet, SNAP_IN).ok_or(PathError::Unreachable)?;
    if start == goal {
        return Ok(vec![graph.nodes[goal as usize]]);
    }
    let n = graph.nodes.len();
    let mut g = vec![f32::INFINITY; n];
    let mut parent = vec![u16::MAX; n];
    let mut open = BinaryHeap::new();
    g[start as usize] = 0.0;
    open.push(Reverse((OrdF(heuristic(graph, start, goal)), start, 0u32)));
    let mut expanded = 0u32;
    while let Some(Reverse((_, i, _))) = open.pop() {
        if expanded >= EXPAND_CAP || *expand_budget == 0 {
            return Err(PathError::BudgetExhausted { expanded });
        }
        expanded += 1;
        *expand_budget -= 1;
        if i == goal {
            return Ok(reconstruct(graph, &parent, start, goal));
        }
        let gi = g[i as usize];
        for &(j, cost) in &graph.adj[i as usize] {
            let ng = gi + cost;
            if ng + 0.01 < g[j as usize] {
                g[j as usize] = ng;
                parent[j as usize] = i;
                let f = ng + heuristic(graph, j, goal);
                open.push(Reverse((OrdF(f), j, expanded)));
            }
        }
    }
    Err(PathError::Unreachable)
}

fn heuristic(graph: &NavGraph, i: u16, goal: u16) -> f32 {
    let a = graph.nodes[i as usize];
    let b = graph.nodes[goal as usize];
    let dx = a[0] - b[0];
    let dy = a[1] - b[1];
    let dz = a[2] - b[2];
    (dx * dx + dy * dy + dz * dz).sqrt()
}

fn reconstruct(graph: &NavGraph, parent: &[u16], start: u16, goal: u16) -> Vec<[f32; 3]> {
    let mut path = Vec::new();
    let mut i = goal;
    path.push(graph.nodes[i as usize]);
    while i != start {
        i = parent[i as usize];
        if i == u16::MAX {
            break;
        }
        path.push(graph.nodes[i as usize]);
    }
    path.reverse();
    path
}

#[derive(Clone, Copy, PartialEq)]
struct OrdF(f32);

impl Eq for OrdF {}

impl Ord for OrdF {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.0.total_cmp(&other.0)
    }
}

impl PartialOrd for OrdF {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}
