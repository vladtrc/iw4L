use crate::use_prox::GameObjectTeam;

pub const ALLIED_DISTANCE_WEIGHT: f32 = 2.0;

pub const FAVORED_WEIGHT_BONUS: f32 = 50_000.0;

pub const PREDICTED_WEIGHT_BONUS: f32 = 100.0;

pub const CARE_PACKAGE_WEIGHT_PENALTY: f32 = 500_000.0;

pub const TI_WINDOW_MS: i32 = 15_000;

pub const TI_DIST_WEIGHT: f32 = 0.1;

pub const SNIPER_DIST_WEIGHT: f32 = 0.5;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SpawnDistancePlayer {
    pub origin: [f32; 3],
    pub team: GameObjectTeam,
    pub was_ti: bool,
    pub spawn_age_ms: i32,
    pub is_sniper: bool,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SpawnPointDistances {
    pub dist_sum_allies: f32,
    pub dist_sum_axis: f32,
    pub weighted_dist_sum_allies: f32,
    pub weighted_dist_sum_axis: f32,
    pub num_players: i32,
}

fn xy_length(from: [f32; 3], to: [f32; 3]) -> f32 {
    let dx = to[0] - from[0];
    let dy = to[1] - from[1];
    libm::sqrtf(dx * dx + dy * dy)
}

fn dist_weight(player: &SpawnDistancePlayer) -> f32 {
    let mut weight = 1.0;
    if player.was_ti && player.spawn_age_ms < TI_WINDOW_MS {
        weight *= TI_DIST_WEIGHT;
    }
    if player.is_sniper {
        weight *= SNIPER_DIST_WEIGHT;
    }
    weight
}

pub fn spawn_point_update_distances(
    spawn_origin: [f32; 3],
    players: &[SpawnDistancePlayer],
) -> SpawnPointDistances {
    let mut dist_allies = 0.0;
    let mut dist_axis = 0.0;
    let mut wdist_allies = 0.0;
    let mut wdist_axis = 0.0;
    let mut wsum_allies = 0.0;
    let mut wsum_axis = 0.0;
    let mut total_allies = 0i32;
    let mut total_axis = 0i32;
    let mut num = 0i32;
    for player in players {
        let team = match player.team {
            GameObjectTeam::Allies | GameObjectTeam::Axis => player.team,
            GameObjectTeam::Neutral | GameObjectTeam::None => continue,
        };
        let dist = xy_length(spawn_origin, player.origin);
        let w = dist_weight(player);
        match team {
            GameObjectTeam::Allies => {
                dist_allies += dist;
                wdist_allies += dist * w;
                wsum_allies += w;
                total_allies += 1;
            }
            GameObjectTeam::Axis => {
                dist_axis += dist;
                wdist_axis += dist * w;
                wsum_axis += w;
                total_axis += 1;
            }
            GameObjectTeam::Neutral | GameObjectTeam::None => {}
        }
        num += 1;
    }
    if wsum_allies != 0.0 {
        wdist_allies = wdist_allies / wsum_allies * total_allies as f32;
    }
    if wsum_axis != 0.0 {
        wdist_axis = wdist_axis / wsum_axis * total_axis as f32;
    }
    SpawnPointDistances {
        dist_sum_allies: dist_allies,
        dist_sum_axis: dist_axis,
        weighted_dist_sum_allies: wdist_allies,
        weighted_dist_sum_axis: wdist_axis,
        num_players: num,
    }
}

fn team_dist(d: &SpawnPointDistances, team: GameObjectTeam, weighted: bool) -> f32 {
    match (team, weighted) {
        (GameObjectTeam::Allies, false) => d.dist_sum_allies,
        (GameObjectTeam::Axis, false) => d.dist_sum_axis,
        (GameObjectTeam::Allies, true) => d.weighted_dist_sum_allies,
        (GameObjectTeam::Axis, true) => d.weighted_dist_sum_axis,
        _ => 0.0,
    }
}

pub fn near_team_base_weight(
    d: &SpawnPointDistances,
    my_team: GameObjectTeam,
    care_blocked: bool,
    favored: bool,
    predicted: bool,
) -> f32 {
    let enemy = match my_team {
        GameObjectTeam::Allies => GameObjectTeam::Axis,
        GameObjectTeam::Axis => GameObjectTeam::Allies,
        GameObjectTeam::Neutral | GameObjectTeam::None => return 0.0,
    };
    let mut weight = if d.num_players > 0 {
        (team_dist(d, enemy, false) - ALLIED_DISTANCE_WEIGHT * team_dist(d, my_team, true))
            / d.num_players as f32
    } else {
        0.0
    };
    if care_blocked {
        weight -= CARE_PACKAGE_WEIGHT_PENALTY;
    }
    if favored {
        weight += FAVORED_WEIGHT_BONUS;
    }
    if predicted {
        weight += PREDICTED_WEIGHT_BONUS;
    }
    weight
}

pub const MAX_SIGHT_TRACED_SPAWNPOINTS: i32 = 3;

pub const LOS_PENALTY_DEFAULT: f32 = 100_000.0;

pub const AVOID_SAME_SPAWN_PENALTY: f32 = 1000.0;

pub const TELEFRAG_FULL_PENALTY: f32 = 100_000.0;

pub const TELEFRAG_PARTIAL_PER: f32 = 1500.0;

pub const WEAPON_DAMAGE_PENALTY_DEFAULT: f32 = 100_000.0;

pub const MIN_GRENADE_DIST_SQ: f32 = 62_500.0;

pub const SPAWN_REUSE_MAX_MS: i32 = 10_000;

pub const SPAWN_REUSE_MAX_DIST_SQ: f32 = 1_048_576.0;

pub const SPAWN_REUSE_PENALTY: f32 = 5000.0;

pub const LAST_MINUTE_DIST_INIT: f32 = 100_000_000.0;

pub fn adjust_sight_value(sight_value: f32) -> f32 {
    if sight_value <= 0.0 {
        0.0
    } else if sight_value >= 1.0 {
        1.0
    } else {
        sight_value * 0.5 + 0.25
    }
}

pub fn los_penalty(nonzero_dvar: Option<f32>) -> f32 {
    nonzero_dvar.unwrap_or(LOS_PENALTY_DEFAULT)
}

pub fn weapon_damage_penalty(nonzero_dvar: Option<f32>) -> f32 {
    nonzero_dvar.unwrap_or(WEAPON_DAMAGE_PENALTY_DEFAULT)
}

pub fn grenade_blocks_spawn(spawn: [f32; 3], grenade: [f32; 3]) -> bool {
    let dx = spawn[0] - grenade[0];
    let dy = spawn[1] - grenade[1];
    let dz = spawn[2] - grenade[2];
    dx * dx + dy * dy + dz * dz < MIN_GRENADE_DIST_SQ
}

pub fn avoid_same_spawn_weight(weight: f32, last_spawn_defined: bool) -> f32 {
    if last_spawn_defined {
        weight - AVOID_SAME_SPAWN_PENALTY
    } else {
        weight
    }
}

pub fn telefrag_weight_penalty(
    origin_would_telefrag: bool,
    consecutive_telefrag_alternates: i32,
    alternate_len: i32,
    force_spawn_near_teammates: bool,
) -> f32 {
    if !origin_would_telefrag {
        return 0.0;
    }
    let telefrag_count = 1 + consecutive_telefrag_alternates;
    if telefrag_count < alternate_len + 1 {
        if force_spawn_near_teammates {
            0.0
        } else {
            TELEFRAG_PARTIAL_PER * telefrag_count as f32
        }
    } else {
        TELEFRAG_FULL_PENALTY
    }
}

pub fn spawn_reuse_worsen(timepassed_ms: i32, dist_sq: f32) -> Option<f32> {
    if timepassed_ms >= SPAWN_REUSE_MAX_MS {
        return None;
    }
    if dist_sq >= SPAWN_REUSE_MAX_DIST_SQ {
        return None;
    }
    Some(
        SPAWN_REUSE_PENALTY
            * (1.0 - dist_sq / SPAWN_REUSE_MAX_DIST_SQ)
            * (1.0 - timepassed_ms as f32 / SPAWN_REUSE_MAX_MS as f32),
    )
}

pub fn max_weight_indices(weights: &[f32], out: &mut [usize]) -> usize {
    if weights.is_empty() || out.is_empty() {
        return 0;
    }
    let mut best = weights[0];
    let mut n = 0usize;
    for (i, &w) in weights.iter().enumerate() {
        if w > best {
            best = w;
            n = 0;
        }
        if w == best && n < out.len() {
            out[n] = i;
            n += 1;
        }
    }
    n
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum LastMinuteDecision {
    Accept,
    Penalize { adjusted_sight: f32 },
}

pub fn last_minute_weighted_decision(
    try_index: i32,
    sights: i32,
    last_trace_same_time: bool,
    last_minute_raw: f32,
) -> LastMinuteDecision {
    if try_index >= MAX_SIGHT_TRACED_SPAWNPOINTS {
        return LastMinuteDecision::Accept;
    }
    if sights > 0 {
        return LastMinuteDecision::Accept;
    }
    if last_trace_same_time {
        return LastMinuteDecision::Accept;
    }
    if last_minute_raw == 0.0 {
        return LastMinuteDecision::Accept;
    }
    LastMinuteDecision::Penalize {
        adjusted_sight: adjust_sight_value(last_minute_raw),
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LastMinutePlayer {
    pub origin: [f32; 3],
    pub team: GameObjectTeam,
    pub session_playing: bool,
    pub is_self: bool,
}

fn dist_sq3(a: [f32; 3], b: [f32; 3]) -> f32 {
    let dx = a[0] - b[0];
    let dy = a[1] - b[1];
    let dz = a[2] - b[2];
    dx * dx + dy * dy + dz * dz
}

pub fn last_minute_closest_two(
    spawn: [f32; 3],
    my_team: GameObjectTeam,
    teambased: bool,
    players: &[LastMinutePlayer],
) -> (Option<usize>, Option<usize>) {
    let mut closest = None;
    let mut closest_d = LAST_MINUTE_DIST_INIT;
    let mut second = None;
    let mut second_d = LAST_MINUTE_DIST_INIT;
    for (i, player) in players.iter().enumerate() {
        if teambased && player.team == my_team {
            continue;
        }
        if !player.session_playing || player.is_self {
            continue;
        }
        let d = dist_sq3(spawn, player.origin);
        if d < closest_d {
            second = closest;
            second_d = closest_d;
            closest = Some(i);
            closest_d = d;
        } else if d < second_d {
            second = Some(i);
            second_d = d;
        }
    }
    (closest, second)
}

pub fn spawnpoint_final_unweighted(
    n: usize,
    last_spawn: Option<usize>,
    would_telefrag: &[bool],
    care_blocked: &[bool],
    has_care_packages: bool,
) -> Option<usize> {
    if n == 0 {
        return None;
    }
    for i in 0..n {
        if last_spawn == Some(i) {
            continue;
        }
        if would_telefrag.get(i).copied().unwrap_or(true) {
            continue;
        }
        if has_care_packages && care_blocked.get(i).copied().unwrap_or(true) {
            continue;
        }
        return Some(i);
    }
    if let Some(last) = last_spawn {
        if last < n && !would_telefrag.get(last).copied().unwrap_or(true) {
            return Some(last);
        }
    }
    None
}

pub fn spawnpoint_final_desperate(
    use_weights: bool,
    n: usize,
    random_index: usize,
) -> Option<usize> {
    if n == 0 {
        return None;
    }
    if use_weights {
        Some(random_index % n)
    } else {
        Some(0)
    }
}
