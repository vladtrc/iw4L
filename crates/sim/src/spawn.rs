use crate::frame::FrameWorld;
use entity_iw4::{TEAM_ALLIES, TEAM_AXIS, TEAM_FREE};
use gamemode_iw4::ffa::{SPAWN_CLASSNAME, START_SPAWN_CLASSNAME};
use gamemode_iw4::{
    DomFlagSpawnNode, FlagDescriptorView, GameModeKind, GameObjectTeam, MAX_COLLECTED_SPAWNS,
    SpawnDistancePlayer, SpawnNearbyView, UseCallbackKind, collect_favored_spawn_indices,
    collect_unowned_nearby_favored, dom_near_team_favored, flag_setup,
    get_unowned_flag_nearest_start, near_team_base_weight, spawn_is_favored,
    spawn_point_update_distances,
};
use movement_iw4::{CollisionBackend, GroundTraceInput};
use trace_iw4::Trace;

use crate::bullet_collision::{MASK_PLAYER_SOLID, PLAYER_MAXS, PLAYER_MINS};
use crate::identities::MatchRng;
use crate::input::ClassId;
use crate::match_state::{ClassDef, ClientLifecycle};
use crate::world::SimState;

pub const SPAWN_IDEAL_DIST: f32 = 1600.0;

pub const SPAWN_BAD_DIST: f32 = 1200.0;

#[derive(Clone, Debug, PartialEq)]
pub struct AuthoredSpawnPoint {
    pub classname: String,
    pub origin: [f32; 3],
    pub angles: [f32; 3],

    pub script_linkto: String,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct DomFlagDescriptor {
    pub origin: [f32; 3],
    pub script_linkname: String,
    pub script_linkto: String,
}

#[derive(Clone, Debug, PartialEq)]
pub struct SpawnDecision {
    pub classname: String,
    pub source_index: usize,
    pub raw_origin: [f32; 3],
    pub raw_angles: [f32; 3],
    pub traced_origin: [f32; 3],
}

#[derive(Clone, Debug, PartialEq)]
pub struct MatchBootstrap {
    pub spawns: Vec<AuthoredSpawnPoint>,
    pub flag_descriptors: Vec<DomFlagDescriptor>,
    pub classes: Vec<ClassDef>,

    pub seed: u64,

    pub kind: gamemode_iw4::GameModeKind,

    pub allow_debug_actions: bool,

    pub respawn_delay_ticks: u32,

    pub host_owns_respawn: bool,

    pub score_limit: i32,

    pub score_kill_points: i32,

    pub time_limit_ms: u32,

    pub intermission_view: Option<AuthoredSpawnPoint>,
}

impl Default for MatchBootstrap {
    fn default() -> Self {
        let (score_limit, score_kill_points, time_limit_ms) =
            crate::score::bootstrap_score_defaults();
        Self {
            spawns: Vec::new(),
            flag_descriptors: Vec::new(),
            classes: Vec::new(),
            seed: 0,
            kind: gamemode_iw4::GameModeKind::FreeForAll,
            allow_debug_actions: false,
            respawn_delay_ticks: 0,
            host_owns_respawn: false,
            score_limit,
            score_kill_points,
            time_limit_ms,
            intermission_view: None,
        }
    }
}

impl MatchBootstrap {
    pub fn class(&self, id: ClassId) -> Option<&ClassDef> {
        self.classes.iter().find(|c| c.id == id)
    }
}

pub fn host_game_mode_kind() -> GameModeKind {
    std::env::var("IW4L_GAMETYPE")
        .ok()
        .as_deref()
        .and_then(GameModeKind::parse_ascii_ignore_case)
        .unwrap_or(GameModeKind::FreeForAll)
}

#[derive(bevy_ecs::prelude::Resource, Clone, Copy, Debug, PartialEq, Eq)]
pub struct HostGameModeSelection(GameModeKind);

impl HostGameModeSelection {
    pub const ALL: [Self; 3] = [
        Self(GameModeKind::FreeForAll),
        Self(GameModeKind::Domination),
        Self(GameModeKind::Demolition),
    ];

    pub fn from_token(token: &str) -> Option<Self> {
        GameModeKind::parse_ascii_ignore_case(token).map(Self)
    }

    pub fn from_env() -> Self {
        Self(host_game_mode_kind())
    }

    pub const fn kind(self) -> GameModeKind {
        self.0
    }

    pub fn token(self) -> &'static str {
        self.0.token()
    }

    pub fn display_name(self) -> &'static str {
        self.0.display_name()
    }
}

pub fn spawn_candidate_indices(spawns: &[AuthoredSpawnPoint]) -> Vec<usize> {
    spawn_candidate_indices_for(spawns, GameModeKind::FreeForAll, TEAM_FREE, true)
}

pub fn spawn_candidate_indices_for(
    spawns: &[AuthoredSpawnPoint],
    kind: GameModeKind,
    client_state_team: i32,
    use_start_spawns: bool,
) -> Vec<usize> {
    if kind.is_team() && (client_state_team == TEAM_AXIS || client_state_team == TEAM_ALLIES) {
        let axis = client_state_team == TEAM_AXIS;
        if use_start_spawns {
            if let Some(start) = kind.team_start_classname(axis) {
                let idx = filter_classname(spawns, |c| c == start);
                if !idx.is_empty() {
                    return idx;
                }
            }
        }
        let grid = kind.team_grid_classnames(axis);
        if !grid.is_empty() {
            let idx = filter_classname(spawns, |c| grid.iter().copied().any(|want| want == c));
            if !idx.is_empty() {
                return idx;
            }
        }
        return Vec::new();
    }
    ffa_candidate_indices(spawns)
}

fn ffa_candidate_indices(spawns: &[AuthoredSpawnPoint]) -> Vec<usize> {
    let dm: Vec<_> = spawns
        .iter()
        .enumerate()
        .filter(|(_, p)| p.classname == SPAWN_CLASSNAME)
        .map(|(i, _)| i)
        .collect();
    if !dm.is_empty() {
        return dm;
    }
    let start: Vec<_> = spawns
        .iter()
        .enumerate()
        .filter(|(_, p)| p.classname == START_SPAWN_CLASSNAME)
        .map(|(i, _)| i)
        .collect();
    if !start.is_empty() {
        return start;
    }
    (0..spawns.len()).collect()
}

fn filter_classname(spawns: &[AuthoredSpawnPoint], pred: impl Fn(&str) -> bool) -> Vec<usize> {
    spawns
        .iter()
        .enumerate()
        .filter(|(_, p)| pred(&p.classname))
        .map(|(i, _)| i)
        .collect()
}

fn playing_distance_players(world: &FrameWorld) -> Vec<SpawnDistancePlayer> {
    let mut rows = Vec::new();
    world.visit_players(|id, ps| {
        rows.push((id, ps.origin));
    });
    let mut out = Vec::new();
    for (id, origin) in rows {
        let Some(meta) = world.client_meta(id) else {
            continue;
        };
        if meta.lifecycle != ClientLifecycle::Alive {
            continue;
        }
        let team = match meta.client_state_team {
            TEAM_ALLIES => GameObjectTeam::Allies,
            TEAM_AXIS => GameObjectTeam::Axis,
            _ => continue,
        };
        out.push(SpawnDistancePlayer {
            origin,
            team,
            was_ti: false,
            spawn_age_ms: gamemode_iw4::TI_WINDOW_MS,
            is_sniper: false,
        });
    }
    out
}

fn opposite_playable(team: GameObjectTeam) -> Option<GameObjectTeam> {
    match team {
        GameObjectTeam::Allies => Some(GameObjectTeam::Axis),
        GameObjectTeam::Axis => Some(GameObjectTeam::Allies),
        GameObjectTeam::Neutral | GameObjectTeam::None => None,
    }
}

fn collect_dom_favored(
    world: &mut FrameWorld,
    my_team: GameObjectTeam,
    out: &mut [u8; MAX_COLLECTED_SPAWNS],
) -> Option<usize> {
    if world.bootstrap_ref().kind != GameModeKind::Domination {
        return None;
    }
    let enemy = opposite_playable(my_team)?;
    let live: Vec<(u32, GameObjectTeam, [f32; 3])> = world
        .use_objects()
        .iter()
        .filter(|object| object.callback_kind == UseCallbackKind::DomFlag)
        .map(|object| (object.id, object.owner_team, object.script_origin))
        .collect();
    if live.is_empty() {
        return None;
    }
    let flags = if world.dom_spawn_graph().len() == live.len() {
        let mut flags = world.dom_spawn_graph().to_vec();
        for (node, (_, owner, origin)) in flags.iter_mut().zip(live.iter()) {
            node.owner = *owner;
            node.origin = *origin;
        }
        flags
    } else {
        let mut flags: Vec<DomFlagSpawnNode> = live
            .iter()
            .map(|(_, owner, origin)| DomFlagSpawnNode::new(*origin, *owner))
            .collect();
        let origins: Vec<[f32; 3]> = world
            .bootstrap_ref()
            .spawns
            .iter()
            .map(|point| point.origin)
            .collect();
        let axis = my_team == GameObjectTeam::Axis;
        let start_name = world.bootstrap_ref().kind.team_start_classname(axis)?;
        let start = world
            .bootstrap_ref()
            .spawns
            .iter()
            .find(|point| point.classname == start_name)
            .map(|point| point.origin)?;
        return collect_unowned_nearby_favored(&mut flags, &origins, start, out);
    };
    let owned = flags.iter().filter(|flag| flag.owner == my_team).count();
    let kind = dom_near_team_favored(owned, flags.len())?;
    let axis = my_team == GameObjectTeam::Axis;
    let start_name = world.bootstrap_ref().kind.team_start_classname(axis)?;
    let start = world
        .bootstrap_ref()
        .spawns
        .iter()
        .find(|point| point.classname == start_name)
        .map(|point| point.origin);
    let (enemy_best, nearby_flag) = match kind {
        gamemode_iw4::DomNearTeamFavored::BoundingAvoidEnemyBest => {
            let id = world.best_spawn_flag(enemy)?;
            let idx = live.iter().position(|(oid, _, _)| *oid == id)?;
            (Some(idx), None)
        }
        gamemode_iw4::DomNearTeamFavored::BoundaryOwned => (None, None),
        gamemode_iw4::DomNearTeamFavored::FlagNearby => {
            let unowned = start.and_then(|s| get_unowned_flag_nearest_start(&flags, s, None));
            let idx = unowned.or_else(|| {
                world
                    .best_spawn_flag(my_team)
                    .and_then(|id| live.iter().position(|(oid, _, _)| *oid == id))
            })?;
            let object = live[idx].0;
            world.set_best_spawn_flag(my_team, object);
            (None, Some(idx))
        }
    };
    collect_favored_spawn_indices(kind, &flags, my_team, enemy_best, nearby_flag, out)
}

pub fn rebuild_dom_spawn_graph(world: &mut SimState) {
    if world.bootstrap_ref().kind != GameModeKind::Domination {
        world.set_dom_spawn_graph(Vec::new());
        return;
    }
    let mut flags: Vec<DomFlagSpawnNode> = world
        .use_objects()
        .iter()
        .filter(|object| object.callback_kind == UseCallbackKind::DomFlag)
        .map(|object| DomFlagSpawnNode::new(object.script_origin, GameObjectTeam::Neutral))
        .collect();
    if flags.is_empty() {
        world.set_dom_spawn_graph(Vec::new());
        return;
    }
    {
        let descriptors: Vec<FlagDescriptorView<'_>> = world
            .bootstrap_ref()
            .flag_descriptors
            .iter()
            .map(|row| FlagDescriptorView {
                origin: row.origin,
                script_linkname: row.script_linkname.as_str(),
                script_linkto: row.script_linkto.as_str(),
            })
            .collect();
        let spawns: Vec<SpawnNearbyView<'_>> = world
            .bootstrap_ref()
            .spawns
            .iter()
            .map(|row| SpawnNearbyView {
                origin: row.origin,
                script_linkto: row.script_linkto.as_str(),
            })
            .collect();
        if flag_setup(&mut flags, &descriptors, &spawns).is_err() {
            let _ = flag_setup(&mut flags, &[], &spawns);
        }
    }
    let start_allies = world
        .bootstrap_ref()
        .kind
        .team_start_classname(false)
        .and_then(|name| {
            world
                .bootstrap_ref()
                .spawns
                .iter()
                .find(|point| point.classname == name)
                .map(|point| point.origin)
        });
    let start_axis = world
        .bootstrap_ref()
        .kind
        .team_start_classname(true)
        .and_then(|name| {
            world
                .bootstrap_ref()
                .spawns
                .iter()
                .find(|point| point.classname == name)
                .map(|point| point.origin)
        });
    let ids: Vec<u32> = world
        .use_objects()
        .iter()
        .filter(|object| object.callback_kind == UseCallbackKind::DomFlag)
        .map(|object| object.id)
        .collect();
    if let Some(start) = start_allies
        && let Some(i) = get_unowned_flag_nearest_start(&flags, start, None)
        && let Some(&id) = ids.get(i)
    {
        world.set_best_spawn_flag(GameObjectTeam::Allies, id);
    }
    let allies_flag = world
        .best_spawn_flag(GameObjectTeam::Allies)
        .and_then(|id| ids.iter().position(|&oid| oid == id));
    if let Some(start) = start_axis
        && let Some(i) = get_unowned_flag_nearest_start(&flags, start, allies_flag)
        && let Some(&id) = ids.get(i)
    {
        world.set_best_spawn_flag(GameObjectTeam::Axis, id);
    }
    world.set_dom_spawn_graph(flags);
}

fn dist_sq(a: [f32; 3], b: [f32; 3]) -> f32 {
    let dx = a[0] - b[0];
    let dy = a[1] - b[1];
    let dz = a[2] - b[2];
    dx * dx + dy * dy + dz * dz
}

fn min_clearance(origin: [f32; 3], avoid: &[[f32; 3]]) -> f32 {
    avoid
        .iter()
        .map(|o| dist_sq(origin, *o).sqrt())
        .fold(f32::INFINITY, f32::min)
}

fn score_spawn(origin: [f32; 3], avoid: &[[f32; 3]]) -> f32 {
    let clearance = min_clearance(origin, avoid);
    if clearance.is_infinite() {
        return SPAWN_IDEAL_DIST;
    }
    if clearance < SPAWN_BAD_DIST {
        clearance - SPAWN_BAD_DIST
    } else {
        clearance.min(SPAWN_IDEAL_DIST)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SpawnReject {
    NoAuthoredCandidates,
    NoGroundHit,
    StartSolid,
    UnsupportedCoverage,
    AllRejected,
}

impl SpawnReject {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::NoAuthoredCandidates => "no_authored_candidates",
            Self::NoGroundHit => "no_ground_hit",
            Self::StartSolid => "start_solid",
            Self::UnsupportedCoverage => "unsupported_coverage",
            Self::AllRejected => "all_rejected",
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct SpawnAttemptReport {
    pub tried: u32,
    pub rejected: Vec<(usize, SpawnReject)>,
    pub accepted: Option<SpawnDecision>,
}

const PLACE_SPAWN_UP: f32 = 128.0;

const PLACE_SPAWN_DOWN: f32 = 262_144.0;

fn lerp3(a: [f32; 3], b: [f32; 3], f: f32) -> [f32; 3] {
    [
        a[0] + (b[0] - a[0]) * f,
        a[1] + (b[1] - a[1]) * f,
        a[2] + (b[2] - a[2]) * f,
    ]
}

struct SpawnClip<'a>(&'a FrameWorld<'a>);

impl CollisionBackend for SpawnClip<'_> {
    fn trace(&self, input: GroundTraceInput) -> Trace {
        self.0.trace_clip(
            input.start,
            input.end,
            input.mins,
            input.maxs,
            input.tracemask,
        )
    }
}

pub(crate) fn ground_spawn(world: &FrameWorld, feet: [f32; 3]) -> Result<[f32; 3], SpawnReject> {
    if !world.has_world_clip() {
        return Err(SpawnReject::UnsupportedCoverage);
    }
    let up_end = [feet[0], feet[1], feet[2] + PLACE_SPAWN_UP];
    let up = world.trace_clip(feet, up_end, PLAYER_MINS, PLAYER_MAXS, MASK_PLAYER_SOLID);
    let raised = lerp3(feet, up_end, up.fraction);
    let down_end = [raised[0], raised[1], raised[2] - PLACE_SPAWN_DOWN];
    let down = world.trace_clip(
        raised,
        down_end,
        PLAYER_MINS,
        PLAYER_MAXS,
        MASK_PLAYER_SOLID,
    );
    if down.fraction >= 1.0 {
        return Err(if down.startsolid != 0 || down.allsolid != 0 {
            SpawnReject::StartSolid
        } else {
            SpawnReject::NoGroundHit
        });
    }
    let landed = lerp3(raised, down_end, down.fraction);
    let stuck = world.trace_clip(landed, landed, PLAYER_MINS, PLAYER_MAXS, MASK_PLAYER_SOLID);
    if stuck.startsolid != 0 || stuck.allsolid != 0 {
        if let Some(fixed) =
            SpawnClip(world).correct_solid(landed, PLAYER_MINS, PLAYER_MAXS, MASK_PLAYER_SOLID)
        {
            return Ok(fixed.origin);
        }
        return Err(SpawnReject::StartSolid);
    }
    Ok(landed)
}

pub(crate) fn decide_spawn_seeded_report(
    world: &mut FrameWorld,
    rng: &mut MatchRng,
    avoid: &[[f32; 3]],
    client_state_team: i32,
) -> SpawnAttemptReport {
    let kind = world.bootstrap_ref().kind;
    let spawn_team = if kind == GameModeKind::Demolition
        && world.objectives.attackers == gamemode_iw4::Team::Axis
    {
        if client_state_team == TEAM_AXIS {
            TEAM_ALLIES
        } else {
            TEAM_AXIS
        }
    } else {
        client_state_team
    };
    let mut candidates = spawn_candidate_indices_for(
        &world.bootstrap_ref().spawns,
        kind,
        spawn_team,
        world.use_start_spawns(),
    );
    if kind == GameModeKind::Demolition && !world.use_start_spawns() {
        let planted: Vec<_> = world
            .objectives
            .bombs
            .iter()
            .filter(|site| site.planted_at_ms.is_some())
            .collect();
        if planted.len() == 1 {
            let suffix = format!("_{}", planted[0].view.label.to_ascii_lowercase());
            let general = if spawn_team == TEAM_AXIS {
                gamemode_iw4::dd::SPAWN_DEFENDER
            } else {
                gamemode_iw4::dd::SPAWN_ATTACKER
            };
            candidates.retain(|index| {
                let name = &world.bootstrap_ref().spawns[*index].classname;
                name == general || name.ends_with(&suffix)
            });
        }
    }
    if candidates.is_empty() {
        return SpawnAttemptReport {
            tried: 0,
            rejected: vec![(0, SpawnReject::NoAuthoredCandidates)],
            accepted: None,
        };
    }

    let players = playing_distance_players(world);
    let use_near_team = kind.is_team() && !world.use_start_spawns();
    let my_team = match client_state_team {
        TEAM_AXIS => Some(GameObjectTeam::Axis),
        TEAM_ALLIES => Some(GameObjectTeam::Allies),
        _ => None,
    };
    let mut favored = [0u8; MAX_COLLECTED_SPAWNS];
    let favored_n = if use_near_team {
        if let Some(team) = my_team {
            collect_dom_favored(world, team, &mut favored).unwrap_or(0)
        } else {
            0
        }
    } else {
        0
    };
    let spawns = world.bootstrap_ref().spawns.clone();
    let mut scored: Vec<(f32, usize)> = candidates
        .iter()
        .map(|&idx| {
            let score = if use_near_team {
                if let Some(team) = my_team {
                    let d = spawn_point_update_distances(spawns[idx].origin, &players);
                    let is_favored =
                        idx <= u8::MAX as usize && spawn_is_favored(idx as u8, &favored, favored_n);
                    near_team_base_weight(&d, team, false, is_favored, false)
                } else {
                    score_spawn(spawns[idx].origin, avoid)
                }
            } else {
                score_spawn(spawns[idx].origin, avoid)
            };
            (score, idx)
        })
        .collect();
    scored.sort_by(|a, b| {
        b.0.partial_cmp(&a.0)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| a.1.cmp(&b.1))
    });

    let order = order_by_score_bands(&scored, rng);
    let mut report = SpawnAttemptReport::default();
    for &source_index in &order {
        report.tried += 1;
        let point = &spawns[source_index];
        match ground_spawn(world, point.origin) {
            Ok(traced_origin) => {
                report.accepted = Some(SpawnDecision {
                    classname: point.classname.clone(),
                    source_index,
                    raw_origin: point.origin,
                    raw_angles: point.angles,
                    traced_origin,
                });
                return report;
            }
            Err(reason) => report.rejected.push((source_index, reason)),
        }
    }
    if report.accepted.is_none() && !report.rejected.is_empty() {
        report.rejected.push((usize::MAX, SpawnReject::AllRejected));
    }
    report
}

fn order_by_score_bands(scored: &[(f32, usize)], rng: &mut MatchRng) -> Vec<usize> {
    let mut out = Vec::with_capacity(scored.len());
    let mut i = 0;
    while i < scored.len() {
        let score = scored[i].0;
        let mut band = Vec::new();
        while i < scored.len() && (scored[i].0 - score).abs() <= f32::EPSILON {
            band.push(scored[i].1);
            i += 1;
        }

        for j in (1..band.len()).rev() {
            let k = rng.next_index(j + 1);
            band.swap(j, k);
        }
        out.extend(band);
    }
    out
}

pub fn pick_ffa_spawn(spawns: &[AuthoredSpawnPoint]) -> Option<(usize, &AuthoredSpawnPoint)> {
    spawns
        .iter()
        .enumerate()
        .find(|(_, p)| p.classname == START_SPAWN_CLASSNAME)
        .or_else(|| {
            spawns
                .iter()
                .enumerate()
                .find(|(_, p)| p.classname == SPAWN_CLASSNAME)
        })
        .or_else(|| spawns.first().map(|p| (0, p)))
}
