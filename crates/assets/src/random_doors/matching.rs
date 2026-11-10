//! Maximum-cardinality connected pairings under a seeded edge cost.
use super::compatible;
use serde_json::Value;
use std::collections::HashMap;

type MatchingResult = Result<Option<(Vec<usize>, Vec<Pair>)>, String>;

type Pair = (usize, usize);
type State = (Vec<bool>, Vec<bool>, usize);

pub(super) fn solve(
    rows: &[Value],
    worlds: usize,
    pair_costs: &[Vec<f64>],
    seed: u64,
) -> MatchingResult {
    if rows.len() > 24 {
        return connected_blossom(rows, worlds, pair_costs, seed);
    }
    // Canonicalize constraints, not door identities. Shuffling equivalent
    // endpoints still changes the layout; it must not change search complexity
    // or whether the exact optimum can be proven within the resource budget.
    let mut order: Vec<_> = (0..rows.len()).collect();
    let keys: Vec<_> = rows
        .iter()
        .map(|r| {
            serde_json::json!([
                r["world"],
                r["width"],
                r["height"],
                r["clearance_apertures"]
            ])
            .to_string()
        })
        .collect();
    order.sort_by(|&a, &b| keys[a].cmp(&keys[b]));
    let ordered: Vec<_> = order.iter().map(|&i| rows[i].clone()).collect();
    let costs: Vec<Vec<_>> = order
        .iter()
        .map(|&a| order.iter().map(|&b| pair_costs[a][b]).collect())
        .collect();
    Ok(
        solve_ordered(&ordered, worlds, &costs)?.map(|(ordinary, pairs)| {
            (
                ordinary.into_iter().map(|i| order[i]).collect(),
                pairs
                    .into_iter()
                    .map(|(a, b)| (order[a], order[b]))
                    .collect(),
            )
        }),
    )
}

fn solve_ordered(rows: &[Value], worlds: usize, costs: &[Vec<f64>]) -> MatchingResult {
    let n = rows.len();
    let allowed: Vec<Vec<bool>> = (0..n)
        .map(|a| {
            (0..n)
                .map(|b| a != b && compatible(&rows[a], &rows[b]))
                .collect()
        })
        .collect();
    let world: Vec<_> = rows
        .iter()
        .map(|r| r["world"].as_u64().unwrap() as usize)
        .collect();
    // Equal doors are interchangeable for graph, compatibility and objective.
    // Their IDs/poses are retained when reconstructing the actual matching.
    let mut groups = Vec::new();
    for a in 0..n {
        let group = (0..a).find(|&b| {
            world[a] == world[b]
                && allowed[a][b]
                && (0..n).filter(|&i| i != a && i != b).all(|i| {
                    allowed[a][i] == allowed[b][i] && (costs[a][i] - costs[b][i]).abs() < 1e-6
                })
        });
        groups.push(group.map_or(a, |b| groups[b]));
    }
    let mut visited = vec![false; n];
    let mut odd = 0;
    for a in 0..n {
        if visited[a] {
            continue;
        }
        let mut stack = vec![a];
        visited[a] = true;
        let mut size = 0;
        while let Some(a) = stack.pop() {
            size += 1;
            for b in 0..n {
                if !visited[b] && allowed[a][b] {
                    visited[b] = true;
                    stack.push(b);
                }
            }
        }
        odd += size % 2;
    }
    // Prove every smaller cardinality impossible before allowing another two
    // ordinary doors. A resource limit is an error, never a false proof.
    for ordinary in (odd..=n).step_by(2) {
        let mut search = Search {
            allowed: &allowed,
            costs,
            world: &world,
            groups: &groups,
            used: vec![false; n],
            edges: vec![vec![0; worlds]; worlds],
            pairs: vec![],
            best: None,
            seen: HashMap::new(),
            budget: 2_000_000,
            exhausted: false,
        };
        search.visit(ordinary, 0.0);
        if search.exhausted {
            return Err(
                "random-door: exact matching search limit reached; optimum is not proven".into(),
            );
        }
        if let Some((_, pairs)) = search.best {
            let mut used = vec![false; n];
            for &(a, b) in &pairs {
                used[a] = true;
                used[b] = true;
            }
            return Ok(Some(((0..n).filter(|&i| !used[i]).collect(), pairs)));
        }
    }
    Ok(None)
}

struct Search<'a> {
    allowed: &'a [Vec<bool>],
    costs: &'a [Vec<f64>],
    world: &'a [usize],
    groups: &'a [usize],
    used: Vec<bool>,
    edges: Vec<Vec<usize>>,
    pairs: Vec<Pair>,
    best: Option<(f64, Vec<Pair>)>,
    seen: HashMap<State, f64>,
    budget: usize,
    exhausted: bool,
}
impl Search<'_> {
    fn visit(&mut self, skips: usize, cost: f64) {
        if self.budget == 0 {
            self.exhausted = true;
            return;
        }
        self.budget -= 1;
        let count = self.edges.len();
        let required: usize = if count == 1 {
            0
        } else if count == 2 {
            1
        } else {
            2
        };
        let available: Vec<_> = (0..self.used.len()).filter(|&i| !self.used[i]).collect();
        if skips > available.len() {
            return;
        }
        for w in 0..count {
            let degree = (0..count)
                .filter(|&v| v != w && self.edges[w][v] > 0)
                .count();
            if available.iter().filter(|&&a| self.world[a] == w).count()
                < required.saturating_sub(degree)
            {
                return;
            }
            let possible = (0..count)
                .filter(|&v| {
                    v != w
                        && (self.edges[w][v] > 0
                            || available.iter().any(|&a| {
                                self.world[a] == w
                                    && available
                                        .iter()
                                        .any(|&b| self.world[b] == v && self.allowed[a][b])
                            }))
                })
                .count();
            if possible < required {
                return;
            }
        }
        if available.is_empty() {
            let mut reached = vec![false; count];
            reached[0] = true;
            loop {
                let before = reached.clone();
                #[expect(
                    clippy::needless_range_loop,
                    reason = "Graph coordinates identify vertices across adjacency and reachability tables."
                )]
                for w in 0..count {
                    if before[w] {
                        #[expect(
                            clippy::needless_range_loop,
                            reason = "Graph coordinates identify vertices across adjacency and reachability tables."
                        )]
                        for v in 0..count {
                            reached[v] |= self.edges[w][v] > 0;
                        }
                    }
                }
                if reached == before {
                    break;
                }
            }
            if !self.pairs.is_empty()
                && reached.into_iter().all(|x| x)
                && self.best.as_ref().is_none_or(|b| cost < b.0 - 1e-6)
            {
                self.best = Some((cost, self.pairs.clone()));
            }
            return;
        }
        // Each matched vertex pays at least its cheapest remaining edge. Each
        // edge is counted twice; omit the most expensive `skips` vertices.
        let mut minima: Vec<_> = available
            .iter()
            .map(|&a| {
                available
                    .iter()
                    .filter(|&&b| self.allowed[a][b])
                    .map(|&b| self.costs[a][b])
                    .fold(f64::INFINITY, f64::min)
            })
            .collect();
        minima.sort_by(f64::total_cmp);
        let bound = cost + minima[..available.len() - skips].iter().sum::<f64>() * 0.5;
        if !bound.is_finite() || self.best.as_ref().is_some_and(|b| bound >= b.0 - 1e-6) {
            return;
        }
        let state = (
            self.used.clone(),
            self.edges.iter().flatten().map(|&n| n > 0).collect(),
            skips,
        );
        if self.seen.get(&state).is_some_and(|&old| old <= cost + 1e-6) {
            return;
        }
        self.seen.insert(state, cost);
        let a = *available
            .iter()
            .min_by_key(|&&a| available.iter().filter(|&&b| self.allowed[a][b]).count())
            .unwrap();
        let wa = self.world[a];
        let mut mates = Vec::new();
        for &b in &available {
            if self.allowed[a][b] && !mates.iter().any(|&old| self.groups[old] == self.groups[b]) {
                mates.push(b);
            }
        }
        mates.sort_by(|&b, &c| self.costs[a][b].total_cmp(&self.costs[a][c]));
        self.used[a] = true;
        for b in mates {
            let wb = self.world[b];
            self.used[b] = true;
            self.edges[wa][wb] += 1;
            self.edges[wb][wa] += 1;
            self.pairs.push((a, b));
            self.visit(skips, cost + self.costs[a][b]);
            self.pairs.pop();
            self.edges[wa][wb] -= 1;
            self.edges[wb][wa] -= 1;
            self.used[b] = false;
            if self.exhausted {
                break;
            }
        }
        if skips > 0 && !self.exhausted {
            self.visit(skips - 1, cost);
        }
        self.used[a] = false;
    }
}

// Connectivity is fixed by a seeded backbone; blossom optimizes the remaining
// general graph. Multiple backbones avoid sacrificing cardinality to one greedy
// edge. Integer costs use up to .01 square unit resolution in this solver.
fn connected_blossom(
    rows: &[Value],
    worlds: usize,
    costs: &[Vec<f64>],
    seed: u64,
) -> MatchingResult {
    fn remaining(rows: &[Value], costs: &[Vec<f64>], used: &[bool]) -> Result<Vec<Pair>, String> {
        let largest = costs
            .iter()
            .flatten()
            .copied()
            .map(f64::abs)
            .fold(0., f64::max);
        if !largest.is_finite() {
            return Err("random-door: non-finite matching cost".into());
        }
        // Bound the total integer objective, including the primary large-pair
        // reward. Ordinary-sized costs retain .01 u² resolution.
        let scale = if largest == 0. {
            100.
        } else {
            (100_000_000. / rows.len().max(1) as f64 / largest).min(100.)
        };
        let mut edges = Vec::new();
        for a in 0..rows.len() {
            for b in a + 1..rows.len() {
                if used[a] || used[b] || !compatible(&rows[a], &rows[b]) {
                    continue;
                }
                let cost = costs[a][b];
                if !cost.is_finite() || !scale.is_finite() || scale <= 0. {
                    return Err("random-door: pairing cost outside blossom integer range".into());
                }
                edges.push((a, b, (-cost * scale).round() as i32));
            }
        }
        let mates = mwmatching::Matching::new(edges).max_cardinality().solve();
        Ok(mates
            .iter()
            .enumerate()
            .filter_map(|(a, &b)| (b != mwmatching::SENTINEL && a < b).then_some((a, b)))
            .collect())
    }
    let upper = remaining(rows, costs, &vec![false; rows.len()])?.len();
    if upper == 0 {
        return Ok(None);
    }
    let mut rng = super::Shuffle(seed ^ 0x6a09e667f3bcc909);
    if rng.0 == 0 {
        rng.0 = 1;
    }
    let mut best: Option<(f64, Vec<Pair>)> = None;
    for _ in 0..if worlds == 1 { 1 } else { 128 } {
        let world: Vec<_> = rows
            .iter()
            .map(|row| row["world"].as_u64().unwrap() as usize)
            .collect();
        let mut used = vec![false; rows.len()];
        let mut pairs = Vec::new();
        let mut connected: Vec<_> = (0..worlds).collect();
        let mut adjacent = vec![vec![false; worlds]; worlds];
        let mut edges = Vec::new();
        for a in 0..rows.len() {
            for b in a + 1..rows.len() {
                if world[a] != world[b] && compatible(&rows[a], &rows[b]) {
                    edges.push((a, b));
                }
            }
        }
        rng.shuffle(&mut edges);
        // A tree does not require a Hamiltonian cycle in the world graph.
        for &(a, b) in &edges {
            let (wa, wb) = (world[a], world[b]);
            if used[a] || used[b] || connected[wa] == connected[wb] {
                continue;
            }
            let from = connected[wb];
            let to = connected[wa];
            for component in &mut connected {
                if *component == from {
                    *component = to;
                }
            }
            used[a] = true;
            used[b] = true;
            adjacent[wa][wb] = true;
            adjacent[wb][wa] = true;
            pairs.push((a, b));
        }
        if connected.iter().any(|&component| component != connected[0]) {
            continue;
        }
        let required = if worlds > 2 {
            2
        } else {
            worlds.saturating_sub(1)
        };
        let mut degree: Vec<_> = adjacent
            .iter()
            .map(|row| row.iter().filter(|&&edge| edge).count())
            .collect();
        let mut possible = true;
        while degree.iter().any(|&n| n < required) {
            let candidates: Vec<_> = edges
                .iter()
                .copied()
                .filter(|&(a, b)| {
                    let (wa, wb) = (world[a], world[b]);
                    !used[a]
                        && !used[b]
                        && !adjacent[wa][wb]
                        && (degree[wa] < required || degree[wb] < required)
                })
                .collect();
            if candidates.is_empty() {
                possible = false;
                break;
            }
            let (a, b) = candidates[rng.index(candidates.len())];
            let (wa, wb) = (world[a], world[b]);
            used[a] = true;
            used[b] = true;
            adjacent[wa][wb] = true;
            adjacent[wb][wa] = true;
            degree[wa] += 1;
            degree[wb] += 1;
            pairs.push((a, b));
        }
        if !possible {
            continue;
        }
        pairs.extend(remaining(rows, costs, &used)?);
        if pairs.len() != upper {
            continue;
        }
        let cost = pairs.iter().map(|&(a, b)| costs[a][b]).sum::<f64>();
        if best.as_ref().is_none_or(|(old, _)| cost < *old) {
            best = Some((cost, pairs));
        }
    }
    let Some((_, pairs)) = best else {
        return Err("random-door: no connected maximum-cardinality backbone found".into());
    };
    let mut used = vec![false; rows.len()];
    for &(a, b) in &pairs {
        used[a] = true;
        used[b] = true;
    }
    Ok(Some((
        (0..rows.len()).filter(|&i| !used[i]).collect(),
        pairs,
    )))
}
