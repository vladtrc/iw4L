use std::cmp::Reverse;
use std::collections::{BTreeMap, BTreeSet, BinaryHeap};

use sim::SimBrush;

use crate::observation::ModeObjective;
use crate::query::{QueryResult, QuerySubsystem, WalkSample, WorldQuery};

pub const NAV_SCHEMA: u32 = 7;
pub const GRID_IN: f32 = 48.0;
pub const SNAP_IN: f32 = 256.0;
pub const STEP_Z_IN: f32 = 18.0;
const DROP_Z_MAX: f32 = 192.0;
/// Stand-hull walk bake: grid, snap, step height. Part of the cache key.
pub const NAV_HULL: u32 = ((GRID_IN as u32) << 16) ^ ((SNAP_IN as u32) << 8) ^ (STEP_Z_IN as u32);
pub const EXPAND_CAP: u32 = 2048;
pub const SEED_PAD_IN: f32 = 768.0;
const DEDUPE_IN: f32 = 8.0;
const COLUMN_CAP: usize = 65536;
// Prefer completing a route within the shared quota over finding the shortest route.
const HEURISTIC_WEIGHT: f32 = 1.5;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PathError {
    EmptyGraph,
    NoStartSupport,
    NoGoalSupport,
    IncompleteGraph,
    Unreachable,
    BudgetExhausted { expanded: u32 },
    GraphChanged,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TraversalKind {
    Walk,
    Drop,
    BreakGlass,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct NavEdge {
    pub to: u16,
    pub cost: f32,
    pub kind: TraversalKind,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RouteStep {
    pub entry: [f32; 3],
    pub position: [f32; 3],
    pub kind: TraversalKind,
}

#[derive(Clone, Debug, Default)]
pub struct NavGraph {
    pub generation: u64,
    pub truncated: bool,
    pub digest: u64,
    pub schema: u32,
    pub hull: u32,
    pub nodes: Vec<[f32; 3]>,
    pub component: Vec<u16>,
    pub adj: Vec<Vec<NavEdge>>,
    pub drops: u32,
    pub component_links: Vec<Vec<u16>>,
}

/// Layout tag for the blob below. The bake's own [`NAV_SCHEMA`] and
/// [`NAV_HULL`] travel inside it, so only a change to the packing itself
/// belongs here.
const CACHE_MAGIC: u32 = 0x5641_4e30;
const CACHE_FORMAT: u32 = 1;

impl TraversalKind {
    fn tag(self) -> u8 {
        match self {
            Self::Walk => 0,
            Self::Drop => 1,
            Self::BreakGlass => 2,
        }
    }

    fn from_tag(tag: u8) -> Option<Self> {
        match tag {
            0 => Some(Self::Walk),
            1 => Some(Self::Drop),
            2 => Some(Self::BreakGlass),
            _ => None,
        }
    }
}

impl NavGraph {
    pub fn is_empty(&self) -> bool {
        self.nodes.is_empty()
    }

    /// The graph is a flat table of nodes and edges, so it stores as one blob.
    /// `component` and `adj` are one row per node, so their lengths are implied.
    /// `generation` is per-run bookkeeping and stays out.
    pub fn cache_encode(&self) -> Vec<u8> {
        let edges = self.adj.iter().map(Vec::len).sum::<usize>();
        let mut out = Vec::with_capacity(40 + self.nodes.len() * 18 + edges * 7);
        out.extend_from_slice(&CACHE_MAGIC.to_le_bytes());
        out.extend_from_slice(&CACHE_FORMAT.to_le_bytes());
        out.extend_from_slice(&self.digest.to_le_bytes());
        out.extend_from_slice(&self.schema.to_le_bytes());
        out.extend_from_slice(&self.hull.to_le_bytes());
        out.extend_from_slice(&self.drops.to_le_bytes());
        out.push(u8::from(self.truncated));
        out.extend_from_slice(&(self.nodes.len() as u32).to_le_bytes());
        for node in &self.nodes {
            for value in node {
                out.extend_from_slice(&value.to_le_bytes());
            }
        }
        for component in &self.component {
            out.extend_from_slice(&component.to_le_bytes());
        }
        for edges in &self.adj {
            out.extend_from_slice(&(edges.len() as u32).to_le_bytes());
            for edge in edges {
                out.extend_from_slice(&edge.to.to_le_bytes());
                out.extend_from_slice(&edge.cost.to_le_bytes());
                out.push(edge.kind.tag());
            }
        }
        out.extend_from_slice(&(self.component_links.len() as u32).to_le_bytes());
        for links in &self.component_links {
            out.extend_from_slice(&(links.len() as u32).to_le_bytes());
            for link in links {
                out.extend_from_slice(&link.to_le_bytes());
            }
        }
        out
    }

    /// A blob from disk is only as good as it reads back: a short, truncated or
    /// out-of-range one is a miss, not a graph the router would index past.
    pub fn cache_decode(blob: &[u8]) -> Option<Self> {
        let mut at = 0usize;
        if word(blob, &mut at)? != CACHE_MAGIC || word(blob, &mut at)? != CACHE_FORMAT {
            return None;
        }
        let digest = long(blob, &mut at)?;
        let schema = word(blob, &mut at)?;
        let hull = word(blob, &mut at)?;
        let drops = word(blob, &mut at)?;
        let truncated = match byte(blob, &mut at)? {
            0 => false,
            1 => true,
            _ => return None,
        };
        let count = word(blob, &mut at)? as usize;
        if count > u16::MAX as usize {
            return None;
        }
        let mut nodes = Vec::with_capacity(count);
        for _ in 0..count {
            nodes.push([
                float(blob, &mut at)?,
                float(blob, &mut at)?,
                float(blob, &mut at)?,
            ]);
        }
        let mut component = Vec::with_capacity(count);
        for _ in 0..count {
            component.push(half(blob, &mut at)?);
        }
        let mut adj = Vec::with_capacity(count);
        for _ in 0..count {
            let edges = word(blob, &mut at)? as usize;
            let mut row = Vec::with_capacity(edges.min(count));
            for _ in 0..edges {
                let to = half(blob, &mut at)?;
                let cost = float(blob, &mut at)?;
                let kind = TraversalKind::from_tag(byte(blob, &mut at)?)?;
                if to as usize >= count || !cost.is_finite() {
                    return None;
                }
                row.push(NavEdge { to, cost, kind });
            }
            adj.push(row);
        }
        // Component ids are only ever compared, never used as an index, so they
        // need no bound; the links between them are indexed, so they do.
        let groups = word(blob, &mut at)? as usize;
        let mut component_links = Vec::with_capacity(groups.min(count + 1));
        for _ in 0..groups {
            let links = word(blob, &mut at)? as usize;
            let mut row = Vec::with_capacity(links.min(groups));
            for _ in 0..links {
                let link = half(blob, &mut at)?;
                if link as usize >= groups {
                    return None;
                }
                row.push(link);
            }
            component_links.push(row);
        }
        (at == blob.len() && nodes.iter().flatten().all(|v| v.is_finite())).then_some(Self {
            generation: 0,
            truncated,
            digest,
            schema,
            hull,
            nodes,
            component,
            adj,
            drops,
            component_links,
        })
    }
}

fn take<'a>(blob: &'a [u8], at: &mut usize, len: usize) -> Option<&'a [u8]> {
    let end = at.checked_add(len)?;
    let slice = blob.get(*at..end)?;
    *at = end;
    Some(slice)
}

fn byte(blob: &[u8], at: &mut usize) -> Option<u8> {
    take(blob, at, 1).map(|bytes| bytes[0])
}

fn half(blob: &[u8], at: &mut usize) -> Option<u16> {
    Some(u16::from_le_bytes(take(blob, at, 2)?.try_into().ok()?))
}

fn word(blob: &[u8], at: &mut usize) -> Option<u32> {
    Some(u32::from_le_bytes(take(blob, at, 4)?.try_into().ok()?))
}

fn float(blob: &[u8], at: &mut usize) -> Option<f32> {
    Some(f32::from_le_bytes(take(blob, at, 4)?.try_into().ok()?))
}

fn long(blob: &[u8], at: &mut usize) -> Option<u64> {
    Some(u64::from_le_bytes(take(blob, at, 8)?.try_into().ok()?))
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
    world.enter(QuerySubsystem::Connector);
    let (mins, maxs) = bounds;
    let step = GRID_IN;
    let mut nodes = Vec::new();
    let z_top = maxs[2] + 72.0;
    let z_bot = mins[2] - 8.0;
    let mut sampled = BTreeSet::new();
    let mut dedupe = Buckets::new();
    let mut columns = 0;
    let mut refinements = Vec::new();
    let mut truncated = false;
    for seed in seeds {
        push_column(
            world,
            &mut nodes,
            &mut dedupe,
            *seed,
            seed[2] + STEP_Z_IN,
            z_bot,
        );
    }
    // Stable world-space tiles keep a distant bounds change from shifting doors
    // between samples. Multi-source breadth-first order gives every seed coverage
    // before spending the remaining column quota on distant geometry.
    const TILE_CELLS: i32 = 8;
    let tile_size = step * TILE_CELLS as f32;
    let lo = cell(mins, tile_size);
    let hi = cell(maxs, tile_size);
    let mut queue = std::collections::VecDeque::new();
    let mut tiles = BTreeSet::new();
    for seed in seeds.iter().chain(std::iter::once(&mins)) {
        let (x, y) = cell(*seed, tile_size);
        let tile = (x.clamp(lo.0, hi.0), y.clamp(lo.1, hi.1));
        if tiles.insert(tile) {
            queue.push_back(tile);
        }
    }
    'scan: while let Some((tx, ty)) = queue.pop_front() {
        for ix in 0..TILE_CELLS {
            for iy in 0..TILE_CELLS {
                let x = (tx * TILE_CELLS + ix) as f32 * step;
                let y = (ty * TILE_CELLS + iy) as f32 * step;
                if x < mins[0] || x > maxs[0] || y < mins[1] || y > maxs[1] {
                    continue;
                }
                if columns >= COLUMN_CAP || nodes.len() >= u16::MAX as usize {
                    truncated = true;
                    break 'scan;
                }
                let before = nodes.len();
                push_column(world, &mut nodes, &mut dedupe, [x, y, 0.0], z_top, z_bot);
                columns += 1;
                let boundary = nodes.len() == before
                    || nodes[before..].iter().any(|p| {
                        [(-step, 0.0), (step, 0.0), (0.0, -step), (0.0, step)]
                            .into_iter()
                            .any(|(dx, dy)| {
                                let Ok(hit) = world.navigation_trace(
                                    [p[0] + dx, p[1] + dy, p[2] + STEP_Z_IN],
                                    [p[0] + dx, p[1] + dy, p[2] - STEP_Z_IN],
                                ) else {
                                    return false;
                                };
                                hit.startsolid || hit.fraction >= 1.0 || hit.normal[2] < 0.7
                            })
                    });
                if boundary {
                    refinements.push([x, y]);
                }
            }
        }
        for (dx, dy) in [(-1, 0), (0, -1), (0, 1), (1, 0)] {
            let next = (tx + dx, ty + dy);
            if next.0 >= lo.0
                && next.0 <= hi.0
                && next.1 >= lo.1
                && next.1 <= hi.1
                && tiles.insert(next)
            {
                queue.push_back(next);
            }
        }
    }
    // Finish base coverage before refinement can consume the node/column cap.
    // Refine in the same seed-first tile order, with the remaining bake budget.
    'refine: for [x, y] in refinements {
        for dx in [-0.5, 0.5] {
            for dy in [-0.5, 0.5] {
                let at = [x + dx * step, y + dy * step, 0.0];
                if at[0] < mins[0]
                    || at[0] > maxs[0]
                    || at[1] < mins[1]
                    || at[1] > maxs[1]
                    || !sampled.insert(cell(at, step * 0.5))
                {
                    continue;
                }
                if columns >= COLUMN_CAP || nodes.len() >= u16::MAX as usize {
                    truncated = true;
                    break 'refine;
                }
                push_column(world, &mut nodes, &mut dedupe, at, z_top, z_bot);
                columns += 1;
            }
        }
    }
    let n = nodes.len();
    let mut adj = vec![Vec::new(); n];
    let buckets = node_buckets(&nodes, step);
    for i in 0..n {
        for j in neighbors(&buckets, nodes[i], step) {
            if i == j {
                continue;
            }
            let a = nodes[i];
            let b = nodes[j];
            let planar = dist_xy(a, b);
            if planar > step * 2.3 || planar < 1.0 {
                continue;
            }
            let dz = (a[2] - b[2]).abs();
            if dz > STEP_Z_IN.max(planar * 0.75) {
                continue;
            }
            let kind = match supported_walk(world, a, b) {
                Ok(WalkSample::Clear) => TraversalKind::Walk,
                Ok(WalkSample::BreakGlass) => TraversalKind::BreakGlass,
                _ => continue,
            };
            adj[i].push(NavEdge {
                to: j as u16,
                cost: planar
                    + dz
                    + if kind == TraversalKind::BreakGlass {
                        96.0
                    } else {
                        0.0
                    },
                kind,
            });
        }
    }
    let component = label_components(n, &adj);
    let drops = link_drops(world, &nodes, &mut adj, step);
    let mut component_links = vec![
        Vec::new();
        component
            .iter()
            .copied()
            .max()
            .map_or(0, |c| c as usize + 1)
    ];
    for (i, edges) in adj.iter().enumerate() {
        for edge in edges {
            let j = edge.to;
            let a = component[i];
            let b = component[j as usize];
            if a != b && !component_links[a as usize].contains(&b) {
                component_links[a as usize].push(b);
            }
        }
    }
    NavGraph {
        generation: 0,
        truncated,
        digest,
        schema: NAV_SCHEMA,
        hull: NAV_HULL,
        nodes,
        component,
        adj,
        drops,
        component_links,
    }
}

fn push_column(
    world: &mut impl WorldQuery,
    nodes: &mut Vec<[f32; 3]>,
    dedupe: &mut Buckets,
    at: [f32; 3],
    z_top: f32,
    z_bot: f32,
) {
    let mut top = z_top;
    while top > z_bot && nodes.len() < u16::MAX as usize {
        let Ok(drop) = world.navigation_trace([at[0], at[1], top], [at[0], at[1], z_bot]) else {
            break;
        };
        if drop.startsolid {
            top -= 32.0;
            continue;
        }
        if drop.fraction >= 1.0 {
            break;
        }
        let feet = drop.endpos;
        let Ok(stand) = world.navigation_trace(feet, feet) else {
            break;
        };
        if drop.normal[2] >= 0.7
            && !stand.startsolid
            && !neighbors(dedupe, feet, DEDUPE_IN).into_iter().any(|i| {
                dist_xy(nodes[i], feet) < DEDUPE_IN && (nodes[i][2] - feet[2]).abs() < DEDUPE_IN
            })
        {
            dedupe
                .entry(cell(feet, DEDUPE_IN))
                .or_default()
                .push(nodes.len());
            nodes.push(feet);
        }
        // Continue below this floor, including rooms beneath roofs and bridges.
        top = feet[2] - 32.0;
    }
}

fn dist_xy(a: [f32; 3], b: [f32; 3]) -> f32 {
    let dx = a[0] - b[0];
    let dy = a[1] - b[1];
    (dx * dx + dy * dy).sqrt()
}

fn link_drops(
    world: &mut impl WorldQuery,
    nodes: &[[f32; 3]],
    adj: &mut [Vec<NavEdge>],
    step: f32,
) -> u32 {
    let n = nodes.len();
    let planar_max = step * 2.0;
    let mut drops = 0u32;
    let buckets = node_buckets(nodes, step);
    for i in 0..n {
        for j in neighbors(&buckets, nodes[i], step) {
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
            if adj[i].iter().any(|e| e.to as usize == j) {
                continue;
            }
            let Ok(horiz) = world.hull_trace([a[0], a[1], a[2] + 2.0], [b[0], b[1], a[2] + 2.0])
            else {
                continue;
            };
            if horiz.startsolid || horiz.fraction < 1.0 {
                continue;
            }
            let from = [b[0], b[1], a[2] + 2.0];
            let Ok(land) = world.hull_trace(from, [from[0], from[1], b[2] - 8.0]) else {
                continue;
            };
            if land.startsolid || land.normal[2] < 0.7 || land.fraction >= 1.0 {
                continue;
            }
            if dist_xy(land.endpos, b) > DEDUPE_IN {
                continue;
            }
            if (land.endpos[2] - b[2]).abs() > STEP_Z_IN {
                continue;
            }
            adj[i].push(NavEdge {
                to: j as u16,
                cost: planar + dz,
                kind: TraversalKind::Drop,
            });
            drops += 1;
        }
    }
    drops
}

fn label_components(n: usize, adj: &[Vec<NavEdge>]) -> Vec<u16> {
    let mut links = vec![Vec::new(); n];
    for (i, edges) in adj.iter().enumerate() {
        for e in edges {
            links[i].push(e.to as usize);
            links[e.to as usize].push(i);
        }
    }
    let mut component = vec![u16::MAX; n];
    let mut next = 0u16;
    for start in 0..n {
        if component[start] != u16::MAX {
            continue;
        }
        let mut stack = vec![start];
        component[start] = next;
        while let Some(i) = stack.pop() {
            for &j in &links[i] {
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
        // Prefer the current floor over a shelf or roof directly overhead.
        let score = d2 + 3.0 * dz * dz;
        if best.is_none_or(|(_, bd)| score < bd) {
            best = Some((i as u16, score));
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
        if !(128.0..=1024.0).contains(&planar) {
            continue;
        }
        let mut h = (i as u32).wrapping_add(salt.wrapping_mul(0x9E37_79B1));
        h = (h ^ (h >> 16)).wrapping_mul(0x85eb_ca6b);
        h = (h ^ (h >> 13)).wrapping_mul(0xc2b2_ae35);
        h ^= h >> 16;
        if best.is_none_or(|(best_h, _)| h >= best_h) {
            best = Some((h, *pos));
        }
    }
    best.map(|(_, pos)| pos).or(fallback)
}

/// Baked supports that could serve as firing positions against a threat at
/// `at`: real graph nodes inside the weapon's band, ordered by how well they fit
/// that band and how little travel they cost from `from`, capped at `max`.
///
/// Component membership is a cheap filter, not a promise that a directed route
/// exists — Drop and BreakGlass edges are one-way, so the caller still routes.
/// Height is the node's own; nothing here invents a position in free space.
pub fn firing_supports(
    graph: &NavGraph,
    from: [f32; 3],
    at: [f32; 3],
    hold: f32,
    max: usize,
) -> Vec<[f32; 3]> {
    if max == 0 {
        return Vec::new();
    }
    let Some(start) = nearest_within(graph, from, SNAP_IN) else {
        return Vec::new();
    };
    let comp = graph.component[start as usize];
    // A band around the weapon's hold range: closer than half of it gives up the
    // weapon's advantage, further than it starts to miss.
    let near = (hold * 0.5).max(48.0);
    let far = hold * 1.25;
    // One support per sector around the threat. Scoring alone would crowd the
    // set onto the side the bot already stands on, which is the side whose line
    // of fire has just failed.
    let mut sectors: Vec<Option<(f32, [f32; 3])>> = vec![None; max];
    for (i, pos) in graph.nodes.iter().enumerate() {
        if graph.component[i] != comp {
            continue;
        }
        let range = dist_xy(*pos, at);
        if !(near..=far).contains(&range) {
            continue;
        }
        let bearing = (pos[1] - at[1]).atan2(pos[0] - at[0]) + core::f32::consts::PI;
        let sector = ((bearing / core::f32::consts::TAU * max as f32) as usize).min(max - 1);
        // Prefer the band's own distance, then the shortest approach. Travel is
        // scored in the same units so neither term can drown the other.
        let score = (range - hold).abs() + 0.5 * dist_xy(*pos, from) + 2.0 * (pos[2] - at[2]).abs();
        if sectors[sector].is_none_or(|(best, _)| score < best) {
            sectors[sector] = Some((score, *pos));
        }
    }
    let mut picked: Vec<(f32, [f32; 3])> = sectors.into_iter().flatten().collect();
    picked.sort_by(|a, b| a.0.total_cmp(&b.0));
    picked.into_iter().map(|(_, pos)| pos).collect()
}

/// Graph-only query for topology diagnostics; runtime callers use `find_route`
/// to validate connectors and interaction volumes against collision.
pub fn find_path(
    graph: &NavGraph,
    start_feet: [f32; 3],
    goal_feet: [f32; 3],
    expand_budget: &mut u32,
) -> Result<Vec<RouteStep>, PathError> {
    if graph.nodes.is_empty() {
        return Err(if graph.truncated {
            PathError::IncompleteGraph
        } else {
            PathError::EmptyGraph
        });
    }
    let start = nearest_within(graph, start_feet, SNAP_IN).ok_or(PathError::Unreachable)?;
    let goal = nearest_within(graph, goal_feet, SNAP_IN).ok_or(PathError::Unreachable)?;
    search(graph, &[(start, 0.0)], &[goal], expand_budget)
}

/// Connect the actual feet position to several locally reachable graph supports.
/// Objective queries accept every support in the authoritative interaction volume.
pub fn find_route(
    world: &mut impl WorldQuery,
    graph: &NavGraph,
    from: [f32; 3],
    to: [f32; 3],
    objective: Option<ModeObjective>,
    budget: &mut u32,
) -> Result<Vec<RouteStep>, PathError> {
    find_route_resumable(
        world,
        graph,
        from,
        to,
        objective,
        budget,
        &mut RouteWork::default(),
    )
}

/// What the request has already finished. Everything before the current phase
/// stays valid while the key holds, so a denied tick repeats no query.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum RoutePhase {
    DirectApproach,
    StartAttachments,
    GoalAttachments,
    Search,
    ValidateEntry,
    ValidateGoal,
}

/// The request identity. Geometry-dependent work survives while this holds;
/// live state is still rechecked before a result is used.
#[derive(Clone, Copy, Debug, PartialEq)]
struct RouteKey {
    graph: (u64, u64, u32, u32),
    to: [f32; 3],
    objective: Option<(u32, u32, crate::observation::ObjectiveAction, [f32; 3], f32)>,
    from: [f32; 3],
}

impl RouteKey {
    fn new(
        graph: &NavGraph,
        from: [f32; 3],
        to: [f32; 3],
        objective: Option<ModeObjective>,
    ) -> Self {
        Self {
            graph: graph_key(graph),
            to,
            objective: objective.map(|o| (o.id, o.round, o.action, o.origin, o.radius)),
            from,
        }
    }

    fn matches(&self, other: &Self) -> bool {
        self.graph == other.graph
            && self.to == other.to
            && self.objective == other.objective
            && distance(self.from, other.from) <= GRID_IN
    }
}

#[derive(Clone, Debug)]
pub struct RouteRequest {
    key: RouteKey,
    objective: Option<ModeObjective>,
    phase: RoutePhase,
    probe: Option<SupportProbe>,
    start_candidates: Vec<(u16, f32)>,
    start_cursor: usize,
    starts: Vec<(u16, f32)>,
    goal_candidates: Vec<u16>,
    goal_cursor: usize,
    goals: Vec<u16>,
    goal_kinds: BTreeMap<u16, TraversalKind>,
    search: Option<RouteSearch>,
    path: Vec<RouteStep>,
    age_slices: u32,
    starved_slices: u32,
}

impl RouteRequest {
    fn begin(key: RouteKey, objective: Option<ModeObjective>) -> Self {
        Self {
            key,
            objective,
            phase: RoutePhase::DirectApproach,
            probe: None,
            start_candidates: Vec::new(),
            start_cursor: 0,
            starts: Vec::new(),
            goal_candidates: Vec::new(),
            goal_cursor: 0,
            goals: Vec::new(),
            goal_kinds: BTreeMap::new(),
            search: None,
            path: Vec::new(),
            age_slices: 0,
            starved_slices: 0,
        }
    }

    /// Everything a granted query can move. Equal marks mean the slice bought
    /// nothing, which is what starvation counts.
    fn progress_mark(&self) -> (RoutePhase, usize, usize, usize, usize, u32, usize, u32) {
        (
            self.phase,
            self.start_cursor,
            self.starts.len(),
            self.goal_cursor,
            self.goals.len(),
            self.search.as_ref().map_or(0, |s| s.expanded),
            self.path.len(),
            self.probe.as_ref().map_or(0, |p| p.done),
        )
    }

    /// Connector work a cancelled request throws away and a reissue must redo.
    fn holds_attachments(&self) -> bool {
        self.start_cursor > 0 || !self.starts.is_empty() || !self.goals.is_empty()
    }

    fn expanded(&self) -> u32 {
        self.search.as_ref().map_or(0, |s| s.expanded)
    }

    fn deferred(&self) -> PathError {
        PathError::BudgetExhausted {
            expanded: self.expanded(),
        }
    }

    /// A live clearance check follows the bot: a probe started somewhere else is
    /// restarted rather than resumed against geometry it never looked at.
    fn probe_for(&mut self, a: [f32; 3], b: [f32; 3]) -> &mut SupportProbe {
        if self.probe.is_none_or(|probe| probe.a != a || probe.b != b) {
            self.probe = Some(SupportProbe::begin(a, b));
        }
        self.probe.as_mut().expect("probe just installed")
    }

    /// One bounded slice of service. `Ok` completes the request; a
    /// `BudgetExhausted` error leaves every finished phase in place. `live` is
    /// where the bot stands now, which the checks against live state use; the
    /// candidate set stays the one the request was opened with.
    fn advance(
        &mut self,
        world: &mut impl WorldQuery,
        graph: &NavGraph,
        budget: &mut u32,
        live: [f32; 3],
    ) -> Result<Vec<RouteStep>, PathError> {
        let from = self.key.from;
        let to = self.key.to;
        let objective = self.objective;
        if self.phase == RoutePhase::DirectApproach {
            // Nearby destinations need no graph detour when the complete approach
            // is supported. Keep the same collision/glass contract as baked edges.
            if distance(live, to) <= GRID_IN * 2.0
                && objective.is_none_or(|obj| world.objective_contains(obj, to))
            {
                match self.probe_for(live, to).advance(world) {
                    Err(_) => return Err(self.deferred()),
                    Ok(WalkSample::Clear) => {
                        return Ok(vec![RouteStep {
                            entry: live,
                            position: to,
                            kind: TraversalKind::Walk,
                        }]);
                    }
                    Ok(WalkSample::BreakGlass) => {
                        return Ok(vec![RouteStep {
                            entry: live,
                            position: to,
                            kind: TraversalKind::BreakGlass,
                        }]);
                    }
                    Ok(WalkSample::Blocked) => {}
                }
            }
            self.probe = None;
            self.phase = RoutePhase::StartAttachments;
            self.start_candidates = near_nodes(graph, from);
        }
        if self.phase == RoutePhase::StartAttachments {
            while self.start_cursor < self.start_candidates.len() && self.starts.len() < 4 {
                let (id, cost) = self.start_candidates[self.start_cursor];
                let node = graph.nodes[id as usize];
                match self.probe_for(from, node).advance(world) {
                    Err(_) => return Err(self.deferred()),
                    Ok(WalkSample::Clear | WalkSample::BreakGlass) => self.starts.push((id, cost)),
                    Ok(WalkSample::Blocked) => {}
                }
                self.probe = None;
                self.start_cursor += 1;
            }
            if self.starts.is_empty() {
                // Nothing baked near this position is a coverage answer. Baked
                // nodes that every supported walk refused are a collision answer
                // about this moment — the hull mask includes other clients — and
                // reporting that as `IncompleteGraph` put it in the terminal
                // memo, where it answered the bot for the life of the graph
                // instead of being probed again.
                return Err(if self.start_candidates.is_empty() && graph.truncated {
                    PathError::IncompleteGraph
                } else {
                    PathError::NoStartSupport
                });
            }
            self.phase = RoutePhase::GoalAttachments;
            if let Some(obj) = objective {
                for (i, p) in graph.nodes.iter().enumerate() {
                    if world.objective_contains(obj, *p) {
                        self.goals.push(i as u16);
                    }
                }
            } else {
                self.goal_candidates = near_nodes(graph, to)
                    .into_iter()
                    .map(|(id, _)| id)
                    .collect();
            }
        }
        if self.phase == RoutePhase::GoalAttachments {
            while objective.is_none()
                && self.goal_cursor < self.goal_candidates.len()
                && self.goals.len() < 4
            {
                let id = self.goal_candidates[self.goal_cursor];
                let node = graph.nodes[id as usize];
                match self.probe_for(node, to).advance(world) {
                    Err(_) => return Err(self.deferred()),
                    Ok(sample @ (WalkSample::Clear | WalkSample::BreakGlass)) => {
                        self.goals.push(id);
                        self.goal_kinds.insert(
                            id,
                            if sample == WalkSample::BreakGlass {
                                TraversalKind::BreakGlass
                            } else {
                                TraversalKind::Walk
                            },
                        );
                    }
                    Ok(WalkSample::Blocked) => {}
                }
                self.probe = None;
                self.goal_cursor += 1;
            }
            if self.goals.is_empty() {
                // An objective's goals are the baked nodes inside its volume, so
                // an empty set is coverage. A plain destination is attached by
                // probing, which is the same live collision question as the
                // start side.
                let unbaked = objective.is_some() || self.goal_candidates.is_empty();
                return Err(if unbaked && graph.truncated {
                    PathError::IncompleteGraph
                } else {
                    PathError::NoGoalSupport
                });
            }
            self.phase = RoutePhase::Search;
        }
        if self.phase == RoutePhase::Search {
            let starts = &self.starts;
            let goals = &self.goals;
            let search = self
                .search
                .get_or_insert_with(|| RouteSearch::begin(graph, starts, goals));
            self.path = search.advance(graph, budget)?;
            self.phase = RoutePhase::ValidateEntry;
        }
        // The completed route waits here for collision quota; A* does not rerun.
        if self.phase == RoutePhase::ValidateEntry {
            if let Some(first) = self.path.first().copied() {
                let kind = match self.probe_for(live, first.position).advance(world) {
                    Err(_) => return Err(self.deferred()),
                    Ok(WalkSample::Clear) => TraversalKind::Walk,
                    Ok(WalkSample::BreakGlass) => TraversalKind::BreakGlass,
                    Ok(WalkSample::Blocked) => return Err(PathError::NoStartSupport),
                };
                self.probe = None;
                let head = &mut self.path[0];
                head.kind = kind;
                head.entry = live;
            }
            self.phase = RoutePhase::ValidateGoal;
        }
        let tail_needed = self
            .path
            .last()
            .is_some_and(|p| distance(p.position, to) > 1.0)
            && objective.is_none_or(|obj| world.objective_contains(obj, to));
        if tail_needed {
            let entry = self.path.last().unwrap().position;
            let kind = if objective.is_none() {
                self.goal_kinds
                    .iter()
                    .find(|(id, _)| graph.nodes[**id as usize] == entry)
                    .map(|(_, &kind)| kind)
            } else {
                let sample = match self.probe_for(entry, to).advance(world) {
                    Err(_) => return Err(self.deferred()),
                    Ok(sample) => sample,
                };
                self.probe = None;
                match sample {
                    WalkSample::Clear => Some(TraversalKind::Walk),
                    WalkSample::BreakGlass => Some(TraversalKind::BreakGlass),
                    WalkSample::Blocked => None,
                }
            };
            if let Some(kind) = kind {
                self.path.push(RouteStep {
                    entry,
                    position: to,
                    kind,
                });
            }
        }
        Ok(std::mem::take(&mut self.path))
    }
}

/// Route outcomes and starvation for the bot that owns this work, plus the
/// terminal results that must not be re-searched against an unchanged graph.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct RouteStats {
    pub completed: u32,
    pub terminal: u32,
    pub cancelled: u32,
    /// Requests dropped while they already held attachment work.
    pub discarded_attachments: u32,
    pub denied_slices: u32,
    pub max_age_slices: u32,
    pub max_starved_slices: u32,
}

const TERMINAL_MEMO: usize = 8;

/// One bot's routing work: the request being served and the graph-topology
/// answers that reissuing cannot change.
#[derive(Clone, Debug, Default)]
pub struct RouteWork {
    active: Option<RouteRequest>,
    terminal: Vec<(RouteKey, PathError)>,
    stats: RouteStats,
}

impl RouteWork {
    pub fn stats(&self) -> RouteStats {
        self.stats
    }

    pub fn is_active(&self) -> bool {
        self.active.is_some()
    }

    /// Slices the live request has waited, and its longest run without progress.
    pub fn live_age(&self) -> (u32, u32) {
        self.active
            .as_ref()
            .map_or((0, 0), |r| (r.age_slices, r.starved_slices))
    }

    pub fn cancel(&mut self) {
        self.drop_active(true);
        self.terminal.clear();
    }

    fn drop_active(&mut self, cancelled: bool) {
        let Some(request) = self.active.take() else {
            return;
        };
        if !cancelled {
            return;
        }
        self.stats.cancelled = self.stats.cancelled.saturating_add(1);
        if request.holds_attachments() {
            self.stats.discarded_attachments = self.stats.discarded_attachments.saturating_add(1);
        }
    }

    /// Budget denial stays pending; a truncated or disconnected graph does not
    /// become answerable by asking it again. Collision-dependent refusals are
    /// left out on purpose — they need live rechecking, not a negative cache.
    fn remember(&mut self, key: RouteKey, error: PathError) {
        if !matches!(
            error,
            PathError::Unreachable | PathError::IncompleteGraph | PathError::EmptyGraph
        ) {
            return;
        }
        self.terminal.retain(|(k, _)| !k.matches(&key));
        self.terminal.insert(0, (key, error));
        self.terminal.truncate(TERMINAL_MEMO);
    }

    fn recall(&self, key: &RouteKey) -> Option<PathError> {
        self.terminal
            .iter()
            .find(|(k, _)| k.matches(key))
            .map(|(_, error)| *error)
    }
}

pub fn find_route_resumable(
    world: &mut impl WorldQuery,
    graph: &NavGraph,
    from: [f32; 3],
    to: [f32; 3],
    objective: Option<ModeObjective>,
    budget: &mut u32,
    work: &mut RouteWork,
) -> Result<Vec<RouteStep>, PathError> {
    world.enter(QuerySubsystem::Connector);
    let key = RouteKey::new(graph, from, to, objective);
    // Terminal answers belong to the coverage that produced them.
    work.terminal.retain(|(k, _)| k.graph == key.graph);
    if work
        .active
        .as_ref()
        .is_some_and(|r| !r.key.matches(&key) || r.key.graph != key.graph)
    {
        work.drop_active(true);
    }
    if graph.is_empty() {
        let error = if graph.truncated {
            PathError::IncompleteGraph
        } else {
            PathError::EmptyGraph
        };
        work.remember(key, error);
        return Err(error);
    }
    if let Some(error) = work.recall(&key) {
        return Err(error);
    }
    let request = work
        .active
        .get_or_insert_with(|| RouteRequest::begin(key, objective));
    let before = request.progress_mark();
    let result = request.advance(world, graph, budget, from);
    request.age_slices = request.age_slices.saturating_add(1);
    if request.progress_mark() == before {
        request.starved_slices = request.starved_slices.saturating_add(1);
    } else {
        request.starved_slices = 0;
    }
    let stats = &mut work.stats;
    stats.max_age_slices = stats.max_age_slices.max(request.age_slices);
    stats.max_starved_slices = stats.max_starved_slices.max(request.starved_slices);
    match result {
        Err(PathError::BudgetExhausted { expanded }) => {
            stats.denied_slices = stats.denied_slices.saturating_add(1);
            Err(PathError::BudgetExhausted { expanded })
        }
        Err(error) => {
            work.stats.terminal = work.stats.terminal.saturating_add(1);
            work.drop_active(false);
            work.remember(key, error);
            Err(error)
        }
        Ok(path) => {
            work.stats.completed = work.stats.completed.saturating_add(1);
            work.drop_active(false);
            Ok(path)
        }
    }
}

/// Graph supports within snapping range of a point, nearest first and stable on
/// ties, capped so that quota slicing alone cannot change the considered set.
fn near_nodes(graph: &NavGraph, at: [f32; 3]) -> Vec<(u16, f32)> {
    let mut nearby: Vec<_> = graph
        .nodes
        .iter()
        .enumerate()
        .filter_map(|(i, p)| {
            let d = distance(at, *p);
            (d <= SNAP_IN && (at[2] - p[2]).abs() <= STEP_Z_IN).then_some((i as u16, d))
        })
        .collect();
    nearby.sort_by(|a, b| a.1.total_cmp(&b.1).then(a.0.cmp(&b.0)));
    nearby.truncate(8);
    nearby
}

#[derive(Clone, Debug)]
pub struct RouteSearch {
    graph_key: (u64, u64, u32, u32),
    goals: Vec<u16>,
    is_goal: Vec<bool>,
    g: Vec<f32>,
    parent: Vec<Option<(u16, TraversalKind)>>,
    closed: Vec<bool>,
    open: BinaryHeap<Reverse<(OrdF, u16, u32)>>,
    pub expanded: u32,
    result: Option<Result<Vec<RouteStep>, PathError>>,
}

fn graph_key(graph: &NavGraph) -> (u64, u64, u32, u32) {
    (graph.generation, graph.digest, graph.schema, graph.hull)
}

impl RouteSearch {
    pub fn begin(graph: &NavGraph, starts: &[(u16, f32)], goals: &[u16]) -> Self {
        let n = graph.nodes.len();
        let mut search = Self {
            graph_key: graph_key(graph),
            goals: goals.to_vec(),
            is_goal: vec![false; n],
            g: vec![f32::INFINITY; n],
            parent: vec![None; n],
            closed: vec![false; n],
            open: BinaryHeap::new(),
            expanded: 0,
            result: None,
        };
        for &id in goals {
            search.is_goal[id as usize] = true;
        }
        for &(id, cost) in starts {
            search.g[id as usize] = cost;
            search.open.push(Reverse((
                OrdF(cost + HEURISTIC_WEIGHT * search.heuristic(graph, id)),
                id,
                0,
            )));
        }
        search
    }

    fn heuristic(&self, graph: &NavGraph, id: u16) -> f32 {
        self.goals
            .iter()
            .map(|&g| distance(graph.nodes[id as usize], graph.nodes[g as usize]))
            .fold(f32::INFINITY, f32::min)
    }

    pub fn advance(
        &mut self,
        graph: &NavGraph,
        budget: &mut u32,
    ) -> Result<Vec<RouteStep>, PathError> {
        if self.graph_key != graph_key(graph) || self.g.len() != graph.nodes.len() {
            return Err(PathError::GraphChanged);
        }
        if let Some(result) = &self.result {
            return result.clone();
        }
        let mut slice = 0;
        while let Some(&Reverse((_, i, _))) = self.open.peek() {
            if self.closed[i as usize] {
                self.open.pop();
                continue;
            }
            // Leave the next live node in the heap until work is actually granted.
            if *budget == 0 || slice >= EXPAND_CAP {
                return Err(PathError::BudgetExhausted {
                    expanded: self.expanded,
                });
            }
            self.open.pop();
            self.closed[i as usize] = true;
            self.expanded += 1;
            slice += 1;
            *budget -= 1;
            if self.is_goal[i as usize] {
                let path = reconstruct(graph, &self.parent, i);
                self.result = Some(Ok(path.clone()));
                return Ok(path);
            }
            for edge in &graph.adj[i as usize] {
                let j = edge.to as usize;
                let ng = self.g[i as usize] + edge.cost;
                if !self.closed[j] && ng + 0.01 < self.g[j] {
                    self.g[j] = ng;
                    self.parent[j] = Some((i, edge.kind));
                    let f = ng + HEURISTIC_WEIGHT * self.heuristic(graph, edge.to);
                    self.open.push(Reverse((OrdF(f), edge.to, self.expanded)));
                }
            }
        }
        let error = if graph.truncated {
            PathError::IncompleteGraph
        } else {
            PathError::Unreachable
        };
        self.result = Some(Err(error));
        Err(error)
    }
}

fn search(
    graph: &NavGraph,
    starts: &[(u16, f32)],
    goals: &[u16],
    budget: &mut u32,
) -> Result<Vec<RouteStep>, PathError> {
    RouteSearch::begin(graph, starts, goals).advance(graph, budget)
}

fn distance(a: [f32; 3], b: [f32; 3]) -> f32 {
    ((a[0] - b[0]).powi(2) + (a[1] - b[1]).powi(2) + (a[2] - b[2]).powi(2)).sqrt()
}

fn reconstruct(
    graph: &NavGraph,
    parent: &[Option<(u16, TraversalKind)>],
    goal: u16,
) -> Vec<RouteStep> {
    let mut path = Vec::new();
    let mut i = goal;
    loop {
        let (entry, kind) = parent[i as usize]
            .map(|(p, kind)| (graph.nodes[p as usize], kind))
            .unwrap_or((graph.nodes[i as usize], TraversalKind::Walk));
        path.push(RouteStep {
            entry,
            position: graph.nodes[i as usize],
            kind,
        });
        let Some((p, _)) = parent[i as usize] else {
            break;
        };
        i = p;
    }
    path.reverse();
    path
}

type Buckets = BTreeMap<(i32, i32), Vec<usize>>;
fn cell(p: [f32; 3], step: f32) -> (i32, i32) {
    ((p[0] / step).floor() as i32, (p[1] / step).floor() as i32)
}
fn node_buckets(nodes: &[[f32; 3]], step: f32) -> Buckets {
    let mut buckets = Buckets::new();
    for (i, &p) in nodes.iter().enumerate() {
        buckets.entry(cell(p, step)).or_default().push(i);
    }
    buckets
}
fn neighbors(buckets: &Buckets, p: [f32; 3], step: f32) -> Vec<usize> {
    let (x, y) = cell(p, step);
    let mut result = Vec::new();
    for dx in -3..=3 {
        for dy in -3..=3 {
            if let Some(ids) = buckets.get(&(x + dx, y + dy)) {
                result.extend_from_slice(ids);
            }
        }
    }
    result.sort_unstable();
    result
}

/// A connector check that keeps its own cursor: one clearance query and one
/// floor probe per 16-unit segment. A denied query leaves the cursor where it
/// was, so the next grant continues instead of restarting the whole check.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SupportProbe {
    a: [f32; 3],
    b: [f32; 3],
    clearance: Option<WalkSample>,
    segments: u32,
    /// Floor probes already accepted; also the progress mark for starvation.
    done: u32,
    previous_z: f32,
}

impl SupportProbe {
    pub fn begin(a: [f32; 3], b: [f32; 3]) -> Self {
        Self {
            a,
            b,
            clearance: None,
            segments: (dist_xy(a, b) / 16.0).ceil().max(1.0) as u32,
            done: 0,
            previous_z: a[2],
        }
    }

    pub fn advance(&mut self, world: &mut impl WorldQuery) -> QueryResult<WalkSample> {
        let clearance = match self.clearance {
            Some(sample) => sample,
            None => {
                let sample = world.walk_hull(self.a, self.b)?;
                self.clearance = Some(sample);
                sample
            }
        };
        if !matches!(clearance, WalkSample::Clear | WalkSample::BreakGlass) {
            return Ok(clearance);
        }
        while self.done < self.segments {
            let t = (self.done + 1) as f32 / self.segments as f32;
            let p: [f32; 3] = std::array::from_fn(|i| self.a[i] + (self.b[i] - self.a[i]) * t);
            let ground =
                world.navigation_trace([p[0], p[1], p[2] + 2.0], [p[0], p[1], p[2] - STEP_Z_IN])?;
            if ground.startsolid
                || ground.normal[2] < 0.7
                || ground.fraction >= 1.0
                || (ground.endpos[2] - self.previous_z).abs() > STEP_Z_IN + 1.0
            {
                return Ok(WalkSample::Blocked);
            }
            self.previous_z = ground.endpos[2];
            self.done += 1;
        }
        Ok(if (self.b[2] - self.previous_z).abs() > STEP_Z_IN + 1.0 {
            WalkSample::Blocked
        } else {
            clearance
        })
    }
}

pub fn supported_walk(
    world: &mut impl WorldQuery,
    a: [f32; 3],
    b: [f32; 3],
) -> QueryResult<WalkSample> {
    SupportProbe::begin(a, b).advance(world)
}

/// Revalidate the horizontal approach and the landing of an explicitly selected drop.
pub fn drop_clear(
    world: &mut impl WorldQuery,
    from: [f32; 3],
    to: [f32; 3],
) -> QueryResult<WalkSample> {
    if from[2] - to[2] <= STEP_Z_IN {
        return world.walk_hull(from, to);
    }
    if from[2] - to[2] > DROP_Z_MAX {
        return Ok(WalkSample::Blocked);
    }
    let top = [to[0], to[1], from[2] + 2.0];
    match world.walk_hull(from, [to[0], to[1], from[2]])? {
        WalkSample::Clear => {}
        other => return Ok(other),
    }
    let land = world.hull_trace(top, [to[0], to[1], to[2] - 8.0])?;
    Ok(
        if !land.startsolid
            && land.normal[2] >= 0.7
            && land.fraction < 1.0
            && (land.endpos[2] - to[2]).abs() <= STEP_Z_IN
        {
            WalkSample::Clear
        } else {
            WalkSample::Blocked
        },
    )
}

#[derive(Clone, Copy, Debug, PartialEq)]
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
