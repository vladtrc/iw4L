use crate::dom_flag_bootstrap::MAX_DOM_FLAGS;
use crate::use_prox::GameObjectTeam;

pub const MAX_ADJ_FLAGS: usize = 4;
pub const MAX_NEARBY_SPAWNS: usize = 32;
pub const MAX_COLLECTED_SPAWNS: usize = 64;
pub const MAX_DESCRIPTORS: usize = 32;

pub const FLAG_DESCRIPTOR: &str = "flag_descriptor";

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DomFlagSpawnNode {
    pub origin: [f32; 3],
    pub owner: GameObjectTeam,
    pub adj: [u8; MAX_ADJ_FLAGS],
    pub adj_len: u8,
    pub nearby: [u8; MAX_NEARBY_SPAWNS],
    pub nearby_len: u8,
}

impl DomFlagSpawnNode {
    pub fn new(origin: [f32; 3], owner: GameObjectTeam) -> Self {
        Self {
            origin,
            owner,
            adj: [0; MAX_ADJ_FLAGS],
            adj_len: 0,
            nearby: [0; MAX_NEARBY_SPAWNS],
            nearby_len: 0,
        }
    }

    pub fn push_adj(&mut self, flag_index: u8) {
        let n = self.adj_len as usize;
        if n < MAX_ADJ_FLAGS {
            self.adj[n] = flag_index;
            self.adj_len += 1;
        }
    }

    fn push_nearby(&mut self, spawn_index: u8) {
        let n = self.nearby_len as usize;
        if n < MAX_NEARBY_SPAWNS {
            self.nearby[n] = spawn_index;
            self.nearby_len += 1;
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DomSpawnPool {
    TeamStart,
    SpawnAll,
}

pub fn dom_spawn_pool(use_start_spawns: bool) -> DomSpawnPool {
    if use_start_spawns {
        DomSpawnPool::TeamStart
    } else {
        DomSpawnPool::SpawnAll
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DomNearTeamFavored {
    BoundingAvoidEnemyBest,

    BoundaryOwned,

    FlagNearby,
}

pub fn dom_near_team_favored(owned: usize, flag_count: usize) -> Option<DomNearTeamFavored> {
    if flag_count == 0 {
        return None;
    }
    if owned == flag_count {
        Some(DomNearTeamFavored::BoundingAvoidEnemyBest)
    } else if owned > 0 {
        Some(DomNearTeamFavored::BoundaryOwned)
    } else {
        Some(DomNearTeamFavored::FlagNearby)
    }
}

pub fn distance_squared(a: [f32; 3], b: [f32; 3]) -> f32 {
    let dx = a[0] - b[0];
    let dy = a[1] - b[1];
    let dz = a[2] - b[2];
    dx * dx + dy * dy + dz * dz
}

pub fn get_unowned_flag_nearest_start(
    flags: &[DomFlagSpawnNode],
    start: [f32; 3],
    exclude: Option<usize>,
) -> Option<usize> {
    let mut best = None;
    let mut best_d = 0.0;
    for (i, flag) in flags.iter().enumerate() {
        if flag.owner != GameObjectTeam::Neutral {
            continue;
        }
        if exclude == Some(i) {
            continue;
        }
        let d = distance_squared(flag.origin, start);
        if best.is_none() || d < best_d {
            best = Some(i);
            best_d = d;
        }
    }
    best
}

pub fn assign_nearbyspawns_by_distance(flags: &mut [DomFlagSpawnNode], spawn_origins: &[[f32; 3]]) {
    for flag in flags.iter_mut() {
        flag.nearby_len = 0;
    }
    for (si, origin) in spawn_origins.iter().enumerate() {
        if si > u8::MAX as usize {
            break;
        }
        let mut best = None;
        let mut best_d = 0.0;
        for (fi, flag) in flags.iter().enumerate() {
            let d = distance_squared(flag.origin, *origin);
            if best.is_none() || d < best_d {
                best = Some(fi);
                best_d = d;
            }
        }
        if let Some(fi) = best {
            flags[fi].push_nearby(si as u8);
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FlagSetupError {
    DescriptorShared,
    MissingAdjLink,
    SelfAdj,
    SpawnBadLink,
    TooManyDescriptors,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FlagSetupOutcome {
    DistanceOnly,
    Linked,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FlagDescriptorView<'a> {
    pub origin: [f32; 3],
    pub script_linkname: &'a str,
    pub script_linkto: &'a str,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SpawnNearbyView<'a> {
    pub origin: [f32; 3],
    pub script_linkto: &'a str,
}

fn lookup_desc(descriptors: &[FlagDescriptorView<'_>], name: &str) -> Option<usize> {
    descriptors.iter().position(|d| d.script_linkname == name)
}

pub fn flag_setup(
    flags: &mut [DomFlagSpawnNode],
    descriptors: &[FlagDescriptorView<'_>],
    spawns: &[SpawnNearbyView<'_>],
) -> Result<FlagSetupOutcome, FlagSetupError> {
    for flag in flags.iter_mut() {
        flag.adj_len = 0;
        flag.nearby_len = 0;
    }
    if descriptors.is_empty() {
        let mut origins = [[0.0f32; 3]; MAX_COLLECTED_SPAWNS];
        let n = spawns.len().min(MAX_COLLECTED_SPAWNS);
        for (i, spawn) in spawns.iter().take(n).enumerate() {
            origins[i] = spawn.origin;
        }
        assign_nearbyspawns_by_distance(flags, &origins[..n]);
        return Ok(FlagSetupOutcome::DistanceOnly);
    }
    if descriptors.len() > MAX_DESCRIPTORS {
        return Err(FlagSetupError::TooManyDescriptors);
    }
    let mut flag_desc = [None; MAX_DOM_FLAGS];
    let mut desc_taken = [false; MAX_DESCRIPTORS];
    for (fi, flag) in flags.iter().enumerate() {
        let mut best = None;
        let mut best_d = 0.0;
        for (di, desc) in descriptors.iter().enumerate() {
            let d = distance_squared(flag.origin, desc.origin);
            if best.is_none() || d < best_d {
                best = Some(di);
                best_d = d;
            }
        }
        let di = best.ok_or(FlagSetupError::MissingAdjLink)?;
        if desc_taken[di] {
            return Err(FlagSetupError::DescriptorShared);
        }
        desc_taken[di] = true;
        flag_desc[fi] = Some(di);
    }
    for fi in 0..flags.len() {
        let Some(di) = flag_desc[fi] else {
            continue;
        };
        for name in descriptors[di].script_linkto.split_whitespace() {
            let other = lookup_desc(descriptors, name).ok_or(FlagSetupError::MissingAdjLink)?;
            let Some(oj) = (0..flags.len()).find(|&j| flag_desc[j] == Some(other)) else {
                return Err(FlagSetupError::MissingAdjLink);
            };
            if oj == fi {
                return Err(FlagSetupError::SelfAdj);
            }
            flags[fi].push_adj(oj as u8);
        }
    }
    for (si, spawn) in spawns.iter().enumerate() {
        if si > u8::MAX as usize {
            break;
        }
        let nearest = if spawn.script_linkto.is_empty() {
            let mut best = None;
            let mut best_d = 0.0;
            for (fi, flag) in flags.iter().enumerate() {
                let d = distance_squared(flag.origin, spawn.origin);
                if best.is_none() || d < best_d {
                    best = Some(fi);
                    best_d = d;
                }
            }
            best
        } else {
            let di = lookup_desc(descriptors, spawn.script_linkto)
                .ok_or(FlagSetupError::SpawnBadLink)?;
            (0..flags.len()).find(|&j| flag_desc[j] == Some(di))
        };
        if let Some(fi) = nearest {
            flags[fi].push_nearby(si as u8);
        } else if !spawn.script_linkto.is_empty() {
            return Err(FlagSetupError::SpawnBadLink);
        }
    }
    Ok(FlagSetupOutcome::Linked)
}

fn adj_owner_differs(flags: &[DomFlagSpawnNode], i: usize, j: usize) -> bool {
    flags[i].owner != flags[j].owner
}

pub fn get_boundary_flag_indices(flags: &[DomFlagSpawnNode], out: &mut [usize]) -> usize {
    let mut n = 0;
    for i in 0..flags.len() {
        let mut hit = false;
        for k in 0..flags[i].adj_len as usize {
            let j = flags[i].adj[k] as usize;
            if j < flags.len() && adj_owner_differs(flags, i, j) {
                hit = true;
                break;
            }
        }
        if hit {
            if n < out.len() {
                out[n] = i;
                n += 1;
            }
        }
    }
    n
}

fn append_nearby(flag: &DomFlagSpawnNode, out: &mut [u8], n: usize) -> usize {
    let mut written = n;
    for k in 0..flag.nearby_len as usize {
        if written >= out.len() {
            break;
        }
        out[written] = flag.nearby[k];
        written += 1;
    }
    written
}

pub fn get_boundary_flag_spawns(
    flags: &[DomFlagSpawnNode],
    team: GameObjectTeam,
    out: &mut [u8],
) -> usize {
    let mut bounds = [0usize; 8];
    let bn = get_boundary_flag_indices(flags, &mut bounds);
    let mut n = 0;
    for i in 0..bn {
        let fi = bounds[i];
        if flags[fi].owner != team {
            continue;
        }
        n = append_nearby(&flags[fi], out, n);
    }
    n
}

pub fn get_spawns_bounding_flag(flags: &[DomFlagSpawnNode], avoid: usize, out: &mut [u8]) -> usize {
    let mut n = 0;
    for (i, flag) in flags.iter().enumerate() {
        if i == avoid {
            continue;
        }
        let mut bounding = false;
        for k in 0..flag.adj_len as usize {
            if flag.adj[k] as usize == avoid {
                bounding = true;
                break;
            }
        }
        if !bounding {
            continue;
        }
        n = append_nearby(flag, out, n);
    }
    n
}

pub fn get_owned_flag_spawns(
    flags: &[DomFlagSpawnNode],
    team: GameObjectTeam,
    out: &mut [u8],
) -> usize {
    let mut n = 0;
    for flag in flags {
        if flag.owner != team {
            continue;
        }
        n = append_nearby(flag, out, n);
    }
    n
}

pub fn collect_favored_spawn_indices(
    kind: DomNearTeamFavored,
    flags: &[DomFlagSpawnNode],
    my_team: GameObjectTeam,
    enemy_best: Option<usize>,
    nearby_flag: Option<usize>,
    out: &mut [u8],
) -> Option<usize> {
    match kind {
        DomNearTeamFavored::BoundingAvoidEnemyBest => {
            Some(get_spawns_bounding_flag(flags, enemy_best?, out))
        }
        DomNearTeamFavored::BoundaryOwned => Some(get_boundary_flag_spawns(flags, my_team, out)),
        DomNearTeamFavored::FlagNearby => {
            let i = nearby_flag?;
            let flag = flags.get(i)?;
            Some(append_nearby(flag, out, 0))
        }
    }
}

pub fn spawn_is_favored(spawn_idx: u8, favored: &[u8], n: usize) -> bool {
    favored.get(..n).is_some_and(|s| s.contains(&spawn_idx))
}

pub fn collect_unowned_nearby_favored(
    flags: &mut [DomFlagSpawnNode],
    spawn_origins: &[[f32; 3]],
    start: [f32; 3],
    out: &mut [u8],
) -> Option<usize> {
    assign_nearbyspawns_by_distance(flags, spawn_origins);
    let i = get_unowned_flag_nearest_start(flags, start, None)?;
    collect_favored_spawn_indices(
        DomNearTeamFavored::FlagNearby,
        flags,
        GameObjectTeam::None,
        None,
        Some(i),
        out,
    )
}
