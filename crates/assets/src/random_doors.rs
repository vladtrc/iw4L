//! Random door pairings with a connected backbone through every selected map.
type MinimumCostMatchingOutput = Option<(Option<usize>, Vec<(usize, usize)>)>;

use serde_json::{Value, json};
mod matching;
mod network;
mod visibility;

/// Maximum total dimension removed when fitting two native openings.
pub const MAX_DOOR_WIDTH_TRIM: f64 = 20.0;
pub const MAX_DOOR_HEIGHT_TRIM: f64 = 28.0;

pub(crate) const GENERATOR_VERSION: u64 = 1;

pub fn shuffle_random_doors(recipe: &mut Value, seed: u64) -> Result<(), String> {
    let count = recipe["worlds"]
        .as_array()
        .ok_or("random-door: no worlds")?
        .len();
    let catalogs = recipe["door_catalogs"]
        .as_array()
        .ok_or("random-door: no door catalogs")?;
    if count == 0 || catalogs.len() != count {
        return Err("random-door: world/catalog count mismatch".into());
    }
    let mut rng = Shuffle(seed.max(1));
    let mut doors: Vec<Vec<Value>> = catalogs
        .iter()
        .enumerate()
        .map(|(world, catalog)| {
            catalog
                .as_array()
                .ok_or("random-door: invalid door catalog")
                .map(|rows| {
                    rows.iter()
                        .map(|door| {
                            let mut door = door.clone();
                            door["world"] = json!(world);
                            door
                        })
                        .collect()
                })
        })
        .collect::<Result<_, _>>()?;
    for rows in &doors {
        for door in rows {
            if door["pair_visibility_version"] != 1 || !door["visible_doors"].is_array() {
                return Err("random-door: door catalog lacks native DPVS pairing visibility; regenerate the template".into());
            }
            if !door["two_sided"].is_boolean()
                || door["geometry_version"] != 4
                || door["opening_base"].as_array().is_none_or(|v| {
                    v.len() != 3 || v.iter().any(|n| n.as_f64().is_none_or(|n| !n.is_finite()))
                })
                || door["opening_size"].as_array().is_none_or(|v| {
                    v.len() != 2
                        || v.iter()
                            .any(|n| n.as_f64().is_none_or(|n| !n.is_finite() || n <= 0.))
                })
                || door["width_high"]
                    .as_f64()
                    .is_none_or(|n| !n.is_finite() || n <= 0.)
            {
                return Err(
                    "random-door: regenerate measured visible geometry catalog version 4".into(),
                );
            }
            if ["width", "height"]
                .iter()
                .any(|key| door[key].as_f64().is_none_or(|size| size <= 0.0))
            {
                return Err("random-door: invalid doorway dimensions".into());
            }
        }
    }
    let minimum = if count == 2 { 1 } else { 2 };
    for (world, rows) in doors.iter_mut().enumerate() {
        if rows.len() < minimum {
            return Err(format!(
                "random-door: {} has {} usable doors; needs at least {minimum}",
                recipe["worlds"][world],
                rows.len()
            ));
        }
        rng.shuffle(rows);
    }
    let mut rows: Vec<_> = doors.into_iter().flatten().collect();
    rng.shuffle(&mut rows);
    let costs = pairing_costs(&rows, seed);
    let fast = (count <= 2 && rows.len() <= 24)
        .then(|| minimum_cost_matching(&rows, count, &costs))
        .flatten()
        .map(|(ordinary, pairs)| (ordinary.into_iter().collect::<Vec<_>>(), pairs));
    let solution = match fast {
        Some(solution) => Some(solution),
        None => matching::solve(&rows, count, &costs, seed)?,
    };
    let Some((ordinary, mut pairs)) = solution else {
        return Err("random-door: no connected pairing within physical doorway trim limits".into());
    };
    network::refine(&rows, count, &costs, &mut pairs);
    let unpaired: Vec<_> = ordinary.into_iter().map(|i| rows[i].clone()).collect();
    let portals: Vec<_> = pairs
        .into_iter()
        .map(|(a, b)| {
            let width = size(&rows[a], "width").min(size(&rows[b], "width"));
            let height = size(&rows[a], "height").min(size(&rows[b], "height"));
            json!({"a":fit_portal_endpoint(&rows[a], width, height), "b":fit_portal_endpoint(&rows[b], width, height),
                "shared_aperture":{"width":width,"height":height}, "flip":true})
        })
        .collect();
    recipe["generator"] = json!({"algorithm":"random-door", "version":GENERATOR_VERSION});
    recipe["aperture_version"] = json!(4);
    recipe["portals"] = json!(portals);
    recipe["seed"] = json!(seed);
    recipe.as_object_mut().unwrap().remove("unpaired_door");
    recipe["unpaired_doors"] = json!(unpaired);
    recipe["display_name"] = json!("random-door");
    Ok(())
}

fn size(door: &Value, key: &str) -> f64 {
    let value = door[key].as_f64().unwrap();
    if key == "width" {
        value.min(door["width_high"].as_f64().unwrap())
    } else {
        value
    }
}

fn compatible(a: &Value, b: &Value) -> bool {
    let w = size(a, "width").min(size(b, "width"));
    let h = size(a, "height").min(size(b, "height"));
    let admits = |door: &Value| {
        door.get("clearance_apertures").is_none_or(|sizes| {
            sizes.as_array().is_some_and(|sizes| {
                sizes.iter().any(|s| {
                    s[0].as_f64().is_some_and(|v| (v - w).abs() < 1e-6)
                        && s[1].as_f64().is_some_and(|v| (v - h).abs() < 1e-6)
                })
            })
        })
    };
    (size(a, "width") - size(b, "width")).abs() <= MAX_DOOR_WIDTH_TRIM
        && (size(a, "height") - size(b, "height")).abs() <= MAX_DOOR_HEIGHT_TRIM
        && admits(a)
        && admits(b)
}

fn removed_area(a: &Value, b: &Value) -> f64 {
    a["opening_size"][0].as_f64().unwrap() * a["opening_size"][1].as_f64().unwrap()
        + b["opening_size"][0].as_f64().unwrap() * b["opening_size"][1].as_f64().unwrap()
        - 2.0 * size(a, "width").min(size(b, "width")) * size(a, "height").min(size(b, "height"))
}

const FAR_REWARD: f64 = 2500.;
const CROSS_REWARD: f64 = 2600.;
const VISIBLE_PENALTY: f64 = 2000.;
const PAIR_NOISE: f64 = 2000.;
const DISTANCE_CAP: f64 = 2048.;

fn mutually_visible(a: &Value, b: &Value) -> bool {
    a["world"] == b["world"]
        && [(a, b), (b, a)].iter().any(|(from, to)| {
            from["visible_doors"]
                .as_array()
                .is_some_and(|ids| ids.iter().any(|id| id == &to["id"]))
        })
}

fn pair_noise(a: &Value, b: &Value, seed: u64) -> f64 {
    fn key(row: &Value) -> String {
        serde_json::json!([row["world"], row.get("id").unwrap_or(row)]).to_string()
    }
    let mut keys = [key(a), key(b)];
    keys.sort();
    let mut hash = 0xcbf29ce484222325u64 ^ seed;
    for key in keys {
        for byte in key.bytes().chain(std::iter::once(255)) {
            hash = (hash ^ u64::from(byte)).wrapping_mul(0x100000001b3);
        }
    }
    hash = (hash ^ (hash >> 30)).wrapping_mul(0xbf58476d1ce4e5b9);
    hash = (hash ^ (hash >> 27)).wrapping_mul(0x94d049bb133111eb);
    hash ^= hash >> 31;
    let uniform = ((hash >> 12) as f64 + 0.5) / (1u64 << 52) as f64;
    -(-uniform.ln()).ln()
}

fn matching_cost(a: &Value, b: &Value, seed: u64) -> f64 {
    let distance = if a["world"] == b["world"] {
        a["base"]
            .as_array()
            .zip(b["base"].as_array())
            .map_or(0., |(a, b)| {
                a.iter()
                    .zip(b)
                    .filter_map(|(a, b)| Some((a.as_f64()? - b.as_f64()?).powi(2)))
                    .sum::<f64>()
                    .sqrt()
            })
    } else {
        0.
    };
    removed_area(a, b)
        - FAR_REWARD * distance.min(DISTANCE_CAP) / DISTANCE_CAP
        - CROSS_REWARD * f64::from(a["world"] != b["world"])
        + VISIBLE_PENALTY * f64::from(mutually_visible(a, b))
        + PAIR_NOISE * pair_noise(a, b, seed)
}

// Keep authored large same-world pairs ahead of the noisy secondary objective.
// The realized seed range bounds every possible difference in total edge cost.
fn prefer_large_pairs(rows: &[Value], costs: &mut [Vec<f64>]) {
    let priority = 1.
        + 2. * rows.len() as f64
            * costs
                .iter()
                .flatten()
                .copied()
                .map(f64::abs)
                .fold(0., f64::max);
    for a in 0..rows.len() {
        for b in a + 1..rows.len() {
            if rows[a]["world"] == rows[b]["world"]
                && rows[a]["opening_size"][0].as_f64().unwrap() >= 96.
                && rows[b]["opening_size"][0].as_f64().unwrap() >= 96.
            {
                costs[a][b] -= priority;
                costs[b][a] = costs[a][b];
            }
        }
    }
}

fn pairing_costs(rows: &[Value], seed: u64) -> Vec<Vec<f64>> {
    let mut costs = vec![vec![0.; rows.len()]; rows.len()];
    for a in 0..rows.len() {
        for b in a + 1..rows.len() {
            costs[a][b] = matching_cost(&rows[a], &rows[b], seed);
            costs[b][a] = costs[a][b];
        }
    }
    prefer_large_pairs(rows, &mut costs);
    costs
}

pub fn fit_portal_endpoint(door: &Value, width: f64, height: f64) -> Value {
    let mut out = door.clone();
    let outer = door["opening_size"].as_array().unwrap();
    let base = door["base"].as_array().unwrap();
    let opening = door["opening_base"].as_array().unwrap();
    let normal = door["normal"].as_array().unwrap();
    let dx = base[0].as_f64().unwrap() - opening[0].as_f64().unwrap();
    let dy = base[1].as_f64().unwrap() - opening[1].as_f64().unwrap();
    let across = -normal[1].as_f64().unwrap() * dx + normal[0].as_f64().unwrap() * dy;
    let bottom = base[2].as_f64().unwrap() - opening[2].as_f64().unwrap();
    let half = outer[0].as_f64().unwrap() * 0.5;
    out["original_size"] = json!([size(door, "width"), size(door, "height")]);
    out["trim"] = json!({"left":half+across-width*0.5,"right":half-across-width*0.5,
        "top":outer[1].as_f64().unwrap()-bottom-height,"bottom":bottom,
        "thickness":door["thickness"].as_f64().unwrap()});
    out["width"] = json!(width);
    out["height"] = json!(height);
    out
}

/// Exact seeded cost including the cross-world edge and choice of one ordinary
/// door. A dummy endpoint represents the ordinary door.
fn minimum_cost_matching(
    rows: &[Value],
    worlds: usize,
    pair_costs: &[Vec<f64>],
) -> MinimumCostMatchingOutput {
    type Choice = (f64, usize, usize);
    fn solve(
        mask: u64,
        crossed: bool,
        required: bool,
        allowed: &[u64],
        costs: &[Vec<f64>],
        cross: &[Vec<bool>],
        memo: &mut std::collections::HashMap<(u64, bool), Option<Choice>>,
    ) -> Option<f64> {
        if mask == 0 {
            return (!required || crossed).then_some(0.0);
        }
        if let Some(answer) = memo.get(&(mask, crossed)) {
            return answer.map(|c| c.0);
        }
        let a = (0..allowed.len())
            .filter(|&i| mask & (1 << i) != 0)
            .min_by_key(|&i| (mask & allowed[i]).count_ones())?;
        let mut mates = mask & allowed[a];
        let mut best: Option<Choice> = None;
        while mates != 0 {
            let b = mates.trailing_zeros() as usize;
            mates &= mates - 1;
            if let Some(rest) = solve(
                mask & !(1 << a) & !(1 << b),
                crossed || cross[a][b],
                required,
                allowed,
                costs,
                cross,
                memo,
            ) {
                let cost = rest + costs[a][b];
                if best.is_none_or(|old| cost < old.0 - 1e-6) {
                    best = Some((cost, a, b));
                }
            }
        }
        memo.insert((mask, crossed), best);
        best.map(|v| v.0)
    }
    let n = rows.len() + rows.len() % 2;
    let mut allowed = vec![0u64; n];
    let mut costs = vec![vec![0.0; n]; n];
    let mut cross = vec![vec![false; n]; n];
    for a in 0..n {
        for b in 0..n {
            if a == b {
                continue;
            }
            let dummy = a == rows.len() || b == rows.len();
            if dummy || compatible(&rows[a], &rows[b]) {
                allowed[a] |= 1 << b;
            }
            if !dummy {
                costs[a][b] = pair_costs[a][b];
                cross[a][b] = rows[a]["world"] != rows[b]["world"];
            }
        }
    }
    let mut mask = (1u64 << n) - 1;
    let mut crossed = false;
    let mut memo = Default::default();
    solve(
        mask,
        crossed,
        worlds == 2,
        &allowed,
        &costs,
        &cross,
        &mut memo,
    )?;
    let mut pairs = Vec::new();
    let mut ordinary = None;
    while mask != 0 {
        let (_, a, b) = memo[&(mask, crossed)]?;
        if a == rows.len() {
            ordinary = Some(b);
        } else if b == rows.len() {
            ordinary = Some(a);
        } else {
            pairs.push((a, b));
        }
        mask &= !(1 << a) & !(1 << b);
        crossed |= cross[a][b];
    }
    Some((ordinary, pairs))
}

struct Shuffle(u64);
impl Shuffle {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }
    fn index(&mut self, len: usize) -> usize {
        let range = len as u64;
        let bound = u64::MAX - u64::MAX % range;
        loop {
            let n = self.next();
            if n < bound {
                return (n % range) as usize;
            }
        }
    }
    fn shuffle<T>(&mut self, rows: &mut [T]) {
        for i in (1..rows.len()).rev() {
            let j = self.index(i + 1);
            rows.swap(i, j);
        }
    }
}
