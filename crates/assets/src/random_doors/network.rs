//! Bounded refinement of the chosen network, including mixed-pair face gates.
use serde_json::Value;

pub(super) fn refine(
    rows: &[Value],
    worlds: usize,
    costs: &[Vec<f64>],
    pairs: &mut Vec<(usize, usize)>,
) {
    let n = rows.len();
    let links: Vec<Vec<_>> = rows
        .iter()
        .map(|a| {
            rows.iter()
                .map(|b| super::visibility::exit_links(a, b))
                .collect()
        })
        .collect();
    let two: Vec<_> = rows
        .iter()
        .map(|r| r["two_sided"].as_bool().expect("validated native sides"))
        .collect();
    let world: Vec<_> = rows
        .iter()
        .map(|r| r["world"].as_u64().unwrap() as usize)
        .collect();
    let incoming: Vec<_> = rows
        .iter()
        .map(|r| 1.0 + r["visible_doors"].as_array().map_or(8, |v| v.len().min(8)) as f64 / 8.0)
        .collect();
    let compatible: Vec<Vec<_>> = rows
        .iter()
        .map(|a| rows.iter().map(|b| super::compatible(a, b)).collect())
        .collect();
    let locked: Vec<_> = pairs
        .iter()
        .map(|&(a, b)| {
            world[a] == world[b]
                && rows[a]["opening_size"][0].as_f64().unwrap() >= 96.0
                && rows[b]["opening_size"][0].as_f64().unwrap() >= 96.0
        })
        .collect();
    let score = |pairs: &[(usize, usize)]| {
        let mut partner = vec![usize::MAX; n];
        let mut sum = 0.0;
        for &(a, b) in pairs {
            partner[a] = b;
            partner[b] = a;
            sum += costs[a][b];
        }
        let active = |a: usize, side: usize| {
            partner[a] != usize::MAX && (side == 0 || (two[a] && two[partner[a]]))
        };
        let mut pressure = 0.0;
        for &(a, b) in pairs {
            for (entered, exit) in [(a, b), (b, a)] {
                #[expect(
                    clippy::needless_range_loop,
                    reason = "Graph coordinates identify vertices across adjacency and reachability tables."
                )]
                for side in 0..2 {
                    if !active(entered, side) {
                        continue;
                    }
                    #[expect(
                        clippy::needless_range_loop,
                        reason = "Graph coordinates identify vertices across adjacency and reachability tables."
                    )]
                    for other in 0..n {
                        if other == a || other == b {
                            continue;
                        }
                        #[expect(
                            clippy::needless_range_loop,
                            reason = "Graph coordinates identify vertices across adjacency and reachability tables."
                        )]
                        for other_side in 0..2 {
                            if active(other, other_side) {
                                pressure +=
                                    incoming[entered] * links[exit][other][side][other_side];
                            }
                        }
                    }
                }
            }
        }
        sum + super::VISIBLE_PENALTY * pressure / 8.0
    };
    let connected = |pairs: &[(usize, usize)]| {
        let mut adjacent = vec![vec![false; worlds]; worlds];
        for &(a, b) in pairs {
            if world[a] != world[b] {
                adjacent[world[a]][world[b]] = true;
                adjacent[world[b]][world[a]] = true;
            }
        }
        let required = if worlds <= 1 {
            0
        } else if worlds == 2 {
            1
        } else {
            2
        };
        if adjacent
            .iter()
            .any(|row| row.iter().filter(|&&v| v).count() < required)
        {
            return false;
        }
        let mut seen = vec![false; worlds];
        let mut stack = vec![0];
        while let Some(a) = stack.pop() {
            if seen[a] {
                continue;
            }
            seen[a] = true;
            for b in 0..worlds {
                if adjacent[a][b] && !seen[b] {
                    stack.push(b);
                }
            }
        }
        seen.into_iter().all(|v| v)
    };
    // Keep seeded edge noise in the objective and bound search effort. Locked
    // large pairs and unpaired ordinary entrances retain their initial owners.
    for _ in 0..2 {
        let mut best_score = score(pairs);
        let mut best = None;
        for i in 0..pairs.len() {
            if locked[i] {
                continue;
            }
            for j in i + 1..pairs.len() {
                if locked[j] {
                    continue;
                }
                let (a, b) = pairs[i];
                let (c, d) = pairs[j];
                for replacement in [[(a, c), (b, d)], [(a, d), (b, c)]] {
                    if replacement.iter().any(|&(x, y)| !compatible[x][y]) {
                        continue;
                    }
                    let mut candidate = pairs.clone();
                    candidate[i] = replacement[0];
                    candidate[j] = replacement[1];
                    if !connected(&candidate) {
                        continue;
                    }
                    let value = score(&candidate);
                    if value < best_score - 1e-6 {
                        best_score = value;
                        best = Some(candidate);
                    }
                }
            }
        }
        match best {
            Some(candidate) => *pairs = candidate,
            None => break,
        }
    }
}
