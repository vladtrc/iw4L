use std::collections::{BTreeMap, BTreeSet};

use bevy_ecs::prelude::World;

use super::super::args::{arg, float, string, vector};
use super::super::natives::engine::entity_id;
use super::super::triggers::entity_bounds;
use super::{
    ActorBrain, DEFAULT_GOAL_HEIGHT, DEFAULT_GOAL_RADIUS, GoalTarget, actor, brain, clear_sight,
    cover_script, distance, has_entry, number, origin_of, set_goal,
};
use crate::script::{Namespace, NativeRegistry, RoundScript, Value};

#[derive(Clone, Debug)]
pub(crate) struct BadPlace {
    name: String,
    bounds: super::Bounds,
    teams: Vec<String>,
    until_ms: Option<i64>,
}

pub(super) fn badplaces_for(runtime: &RoundScript, team: &Value, now: i64) -> Vec<super::Bounds> {
    let team = match team {
        Value::String(team) => team.to_string(),
        _ => String::new(),
    };
    runtime
        .campaign
        .badplaces
        .iter()
        .filter(|place| place.until_ms.is_none_or(|until| now < until))
        .filter(|place| place.teams.is_empty() || place.teams.iter().any(|t| *t == team))
        .map(|place| place.bounds)
        .collect()
}

fn add_badplace(
    world: &mut World,
    args: &[Value],
    bounds: super::Bounds,
    teams_from: usize,
) -> Result<Value, String> {
    let name = string(args, 0)?.to_string();
    let duration = number(arg(args, 1)?.clone()).unwrap_or(-1.0);
    let now = super::super::players::now_ms(world);
    let teams = args
        .get(teams_from..)
        .unwrap_or_default()
        .iter()
        .filter_map(|team| match team {
            Value::String(team) => Some(match &*team.to_string() {
                "bad_guys" => "axis".to_owned(),
                "good_guys" => "allies".to_owned(),
                other => other.to_owned(),
            }),
            _ => None,
        })
        .collect();
    let mut runtime = world.resource_mut::<RoundScript>();
    let places = &mut runtime.campaign.badplaces;
    places.retain(|place| place.until_ms.is_none_or(|until| now < until));
    if !name.is_empty() {
        places.retain(|place| place.name != name);
    }
    places.push(BadPlace {
        name,
        bounds,
        teams,
        until_ms: (duration > 0.0).then(|| now + (duration * 1000.0) as i64),
    });
    for goal in runtime.campaign.actor_goals.values_mut() {
        goal.routed_to = None;
    }
    Ok(Value::Undefined)
}

const SHUFFLE_RADIUS: f32 = 256.0;

pub(super) fn begin_traverse(runtime: &mut RoundScript, id: u64, (start, end): (usize, usize)) {
    let (Some(&start), Some(&end)) = (
        runtime.campaign.nodes.get(start),
        runtime.campaign.nodes.get(end),
    ) else {
        return;
    };
    runtime
        .campaign
        .actor_brains
        .entry(id)
        .or_default()
        .traverse = Some((start, end));
}

pub(super) fn traverse_script(world: &mut World, brain: &mut ActorBrain) -> Option<String> {
    let (start, _) = brain.traverse?;
    let anim = match world
        .resource_mut::<RoundScript>()
        .object_field(start, "animscript")
    {
        Value::String(anim) => anim.to_string(),
        _ => String::new(),
    };
    let script = format!("traverse/{anim}");
    if !anim.is_empty() && has_entry(world, &format!("animscripts/{script}::main")) {
        return Some(script);
    }
    brain.traverse = None;
    None
}

pub(super) fn finish_traverse(
    world: &mut World,
    id: u64,
    brain: &mut ActorBrain,
    thread_done: bool,
) {
    let Some((_, end)) = brain.traverse else {
        return;
    };
    if !thread_done || !brain.script.starts_with("traverse/") {
        return;
    }
    let mut runtime = world.resource_mut::<RoundScript>();
    if let Value::Vector(at) = runtime.object_field(end, "origin") {
        runtime.set_object_field(id, "origin", Value::Vector(at));
    }
    brain.traverse = None;
}

#[derive(Clone, Debug, Default)]
pub(crate) struct ThreatBias {
    groups: BTreeSet<String>,
    ignore: BTreeSet<(String, String)>,
    bias: BTreeMap<(String, String), f32>,
}

impl ThreatBias {
    pub(super) fn ignores(&self, viewer: &str, target: &str) -> bool {
        self.ignore
            .contains(&(viewer.to_string(), target.to_string()))
    }

    pub(super) fn bias(&self, viewer: &str, target: &str) -> f32 {
        self.bias
            .get(&(target.to_string(), viewer.to_string()))
            .copied()
            .unwrap_or(0.0)
    }

    fn set_against_all(&mut self, target: &str, bias: f32) {
        for viewer in &self.groups {
            self.bias.insert((target.to_string(), viewer.clone()), bias);
        }
    }
}

pub(super) fn group_of(runtime: &mut RoundScript, id: u64) -> String {
    match runtime.object_field(id, "threatbiasgroup") {
        Value::String(group) => group.to_string(),
        _ => String::new(),
    }
}

pub(super) fn negotiation_node(
    world: &mut World,
    receiver: &Value,
    start: bool,
) -> Result<Value, String> {
    let id = actor(world, receiver)?;
    Ok(match brain(world, id).traverse {
        Some((s, e)) => Value::Object(if start { s } else { e }),
        None => Value::Undefined,
    })
}

enum Area {
    Any,
    Box([f32; 3], [f32; 3]),
    Sphere {
        center: [f32; 3],
        radius: f32,
        height: f32,
    },
}

fn inside(area: &Area, at: [f32; 3]) -> bool {
    match area {
        Area::Any => true,
        Area::Box(lo, hi) => (0..3).all(|k| lo[k] <= at[k] && at[k] <= hi[k]),
        Area::Sphere {
            center,
            radius,
            height,
        } => {
            (0..2)
                .map(|k| (at[k] - center[k]).powi(2))
                .sum::<f32>()
                .sqrt()
                <= *radius
                && (at[2] - center[2]).abs() <= *height
        }
    }
}

pub(super) fn vector_field(runtime: &mut RoundScript, id: u64, name: &str) -> Option<[f32; 3]> {
    match runtime.object_field(id, name) {
        Value::Vector(at) => Some(at),
        _ => None,
    }
}

fn enemy_origin(world: &mut World, id: u64) -> Option<[f32; 3]> {
    let Value::Object(enemy) = world
        .resource_mut::<RoundScript>()
        .object_field(id, "enemy")
    else {
        return None;
    };
    match super::super::players::entity_field(world, enemy, "origin") {
        Value::Vector(at) => Some(at),
        _ => None,
    }
}

pub(super) fn volume_box(world: &mut World, volume: u64) -> ([f32; 3], [f32; 3]) {
    let at = vector_field(&mut world.resource_mut::<RoundScript>(), volume, "origin")
        .unwrap_or([0.0; 3]);
    let (lo, hi) = entity_bounds(world, volume);
    (
        std::array::from_fn(|k| at[k] + lo[k]),
        std::array::from_fn(|k| at[k] + hi[k]),
    )
}

fn goal_area(world: &mut World, id: u64) -> Area {
    let volume = match world
        .resource_mut::<RoundScript>()
        .object_field(id, "goalvolume")
    {
        Value::Object(volume) => Some(volume),
        _ => None,
    };
    if let Some(volume) = volume {
        let (lo, hi) = volume_box(world, volume);
        return Area::Box(lo, hi);
    }
    let mut runtime = world.resource_mut::<RoundScript>();
    let Some(center) = vector_field(&mut runtime, id, "goalpos") else {
        return Area::Any;
    };
    Area::Sphere {
        center,
        radius: number(runtime.object_field(id, "goalradius")).unwrap_or(DEFAULT_GOAL_RADIUS),
        height: number(runtime.object_field(id, "goalheight")).unwrap_or(DEFAULT_GOAL_HEIGHT),
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct Cover {
    id: u64,
    origin: [f32; 3],
    yaw: f32,
    forward: [f32; 3],
    bearings: (f32, f32),
    min_enemy_dist: f32,
    priority: bool,
    dangerous: bool,
    clear_of_player: bool,
    other_threats: u8,
}

const WIDE_COVER: i32 = 0x200;
const PRIORITY_COVER: i32 = 0x40;

fn cover_arc(kind: &str, spawnflags: i32) -> (f32, f32, f32) {
    let wide = spawnflags & WIDE_COVER != 0;
    match kind {
        "Cover Right" if wide => (310.0, 45.0, 0.0),
        "Cover Left" if wide => (315.0, 50.0, 0.0),
        "Cover Right" => (310.0, 15.0, 0.0),
        "Cover Left" => (345.0, 50.0, 0.0),
        "Cover Prone" | "Conceal Prone" => (315.0, 45.0, 800.0),
        "Cover Stand" | "Cover Crouch" | "Conceal Stand" | "Conceal Crouch" => (315.0, 45.0, 0.0),
        "Cover Crouch Window" => (330.0, 30.0, 0.0),
        _ => (300.0, 60.0, 0.0),
    }
}

fn covers_against(cover: &Cover, enemy: [f32; 3]) -> bool {
    if distance(cover.origin, enemy) < cover.min_enemy_dist {
        return false;
    }
    let bearing = ((enemy[1] - cover.origin[1])
        .atan2(enemy[0] - cover.origin[0])
        .to_degrees()
        - cover.yaw)
        .rem_euclid(360.0);
    let (from, to) = cover.bearings;
    if from <= to {
        (from..=to).contains(&bearing)
    } else {
        bearing >= from || bearing <= to
    }
}

const SCORE_DISTANCE: f32 = 16.0;
const SCORE_TARGET_DIR: f32 = 8.0;
const SCORE_VISIBILITY: f32 = 8.0;
const SCORE_NODE_ANGLE: f32 = 4.0;
const SCORE_PRIORITY: f32 = 8.0;
const SCORE_DANGEROUS: f32 = 32.0;
const SCORE_PLAYER_LOS: f32 = 8.0;
const SCORE_FAR: f32 = 1300.0;
const SCORE_TARGET_DIR_MIN: f32 = 64.0;

#[derive(Clone, Copy, Debug)]
struct Scoring {
    from: [f32; 3],
    enemy: Option<([f32; 3], f32)>,
}

fn engagement_pull(runtime: &mut RoundScript, id: u64, dist: f32) -> f32 {
    let mut field = |name| number(runtime.object_field(id, name)).unwrap_or(0.0);
    if dist < field("engageminfalloffdist") {
        -1.0
    } else if dist < field("engagemindist") {
        -0.5
    } else if dist > field("engagemaxfalloffdist") {
        1.0
    } else if dist > field("engagemaxdist") {
        0.5
    } else {
        0.0
    }
}

fn cover_score(cover: &Cover, scoring: &Scoring, sees_enemy: bool) -> f32 {
    let far = SCORE_FAR * SCORE_FAR;
    let near = (0..3)
        .map(|k| (cover.origin[k] - scoring.from[k]).powi(2))
        .sum::<f32>()
        .min(far);
    let mut score = SCORE_DISTANCE * (1.0 - near / far);
    if cover.priority {
        score += SCORE_PRIORITY;
    }
    if cover.dangerous {
        score -= SCORE_DANGEROUS;
    }
    if cover.clear_of_player {
        score += SCORE_PLAYER_LOS;
    }
    score += SCORE_NODE_ANGLE * f32::from(cover.other_threats);
    if sees_enemy {
        score += SCORE_VISIBILITY;
    }
    if let Some((enemy, pull)) = scoring.enemy {
        if covers_against(cover, enemy) {
            score += SCORE_NODE_ANGLE;
        }
        let to_enemy = glam::Vec2::new(enemy[0] - scoring.from[0], enemy[1] - scoring.from[1])
            .normalize_or_zero();
        let step = glam::Vec2::new(
            cover.origin[0] - scoring.from[0],
            cover.origin[1] - scoring.from[1],
        );
        if step.length() >= SCORE_TARGET_DIR_MIN {
            let toward = to_enemy.dot(step);
            if toward > 0.2 {
                score += SCORE_TARGET_DIR * pull;
            } else if toward < -0.2 {
                score -= SCORE_TARGET_DIR * pull;
            }
        }
    }
    score
}

fn best_cover(scoring: &Scoring, covers: &[(Cover, bool)]) -> Option<u64> {
    covers
        .iter()
        .max_by(|a, b| cover_score(&a.0, scoring, a.1).total_cmp(&cover_score(&b.0, scoring, b.1)))
        .map(|(cover, _)| cover.id)
}

fn free_for(runtime: &mut RoundScript, id: u64, node: u64) -> bool {
    match runtime.object_field(node, "claimedby") {
        Value::Object(claimer) if claimer != id && runtime.live(&claimer) => {
            runtime.object_field(claimer, "node") == Value::Object(node)
        }
        _ => true,
    }
}

const COVER_SEARCH_MAX: usize = 256;
const COVER_SEARCH_RADIUS: f32 = 716.8;

fn cover_search(runtime: &mut RoundScript, id: u64) -> Area {
    let origin = vector_field(runtime, id, "origin").unwrap_or([0.0; 3]);
    let goal = vector_field(runtime, id, "goalpos").unwrap_or(origin);
    let radius = number(runtime.object_field(id, "goalradius")).unwrap_or(DEFAULT_GOAL_RADIUS);
    let height = number(runtime.object_field(id, "goalheight")).unwrap_or(DEFAULT_GOAL_HEIGHT);
    let (center, radius) = search_centre(origin, goal, radius);
    Area::Sphere {
        center,
        radius,
        height,
    }
}

fn search_centre(origin: [f32; 3], goal: [f32; 3], radius: f32) -> ([f32; 3], f32) {
    if radius <= COVER_SEARCH_RADIUS {
        return (goal, radius);
    }
    let to_goal = glam::Vec3::from(goal) - glam::Vec3::from(origin);
    let pull = (to_goal.length() - (radius - COVER_SEARCH_RADIUS)).max(0.0);
    let at = glam::Vec3::from(origin) + to_goal.normalize_or_zero() * pull;
    ([at.x, at.y, goal[2]], COVER_SEARCH_RADIUS)
}

fn search_covers(runtime: &mut RoundScript, search: &Area) -> Vec<Cover> {
    let center = match search {
        Area::Sphere { center, .. } => Some(*center),
        _ => None,
    };
    let mut covers: Vec<Cover> = runtime
        .campaign
        .nodes
        .clone()
        .into_iter()
        .filter_map(|node| cover_at(runtime, node))
        .filter(|cover| inside(search, cover.origin))
        .collect();
    if let Some(center) = center {
        let planar = |at: [f32; 3]| (at[0] - center[0]).hypot(at[1] - center[1]);
        covers.sort_by(|a, b| planar(a.origin).total_cmp(&planar(b.origin)));
    }
    covers.truncate(COVER_SEARCH_MAX);
    covers
}

fn find_cover(
    world: &mut World,
    id: u64,
    area: &Area,
    search: &Area,
    skip: Option<u64>,
) -> Option<u64> {
    let now = super::super::players::now_ms(world);
    let enemy = enemy_origin(world, id);
    let known = brain(world, id).known;
    let enemy_eye = enemy.map(|at| [at[0], at[1], at[2] + super::EYE_HEIGHT]);
    let mut runtime = world.resource_mut::<RoundScript>();
    let from = vector_field(&mut runtime, id, "origin").unwrap_or([0.0; 3]);
    let team = runtime.object_field(id, "team");
    let min_dist = number(runtime.object_field(id, "engagemindist")).unwrap_or(0.0);
    let current = runtime.object_field(id, "enemy");
    let others: Vec<[f32; 3]> = known
        .into_iter()
        .filter(|(other, seen, _)| {
            now - seen < super::ENEMY_MEMORY_MS
                && Value::Object(*other) != current
                && runtime.live(other)
        })
        .map(|(_, _, at)| at)
        .collect();
    let scoring = Scoring {
        from,
        enemy: enemy.map(|at| {
            let dist = (at[0] - from[0]).hypot(at[1] - from[1]);
            (at, engagement_pull(&mut runtime, id, dist))
        }),
    };
    let mut covers = Vec::new();
    for mut cover in search_covers(&mut runtime, search) {
        let node = cover.id;
        if Some(node) == skip || !free_for(&mut runtime, id, node) {
            continue;
        }
        if inside(area, cover.origin) {
            cover.dangerous = node_marked(&runtime, node, &team, now);
            cover.clear_of_player = team == Value::string("allies")
                && runtime
                    .campaign
                    .node_los
                    .get(&node)
                    .is_none_or(|until| *until < now);
            let covered = others
                .iter()
                .filter(|at| {
                    distance(cover.origin, **at) > min_dist && covers_against(&cover, **at)
                })
                .count();
            cover.other_threats = covered.min(u8::MAX.into()) as u8;
            covers.push(cover);
        }
    }
    drop(runtime);
    let covers: Vec<(Cover, bool)> = covers
        .into_iter()
        .map(|cover| {
            let eye = [
                cover.origin[0],
                cover.origin[1],
                cover.origin[2] + super::EYE_HEIGHT,
            ];
            let sees = enemy_eye.is_none_or(|enemy| clear_sight(world, eye, enemy));
            (cover, sees)
        })
        .collect();
    best_cover(&scoring, &covers)
}

fn cover_at(runtime: &mut RoundScript, node: u64) -> Option<Cover> {
    let Value::String(kind) = runtime.object_field(node, "type") else {
        return None;
    };
    let origin = vector_field(runtime, node, "origin")?;
    let angles = vector_field(runtime, node, "angles")?;
    if cover_script(&kind).is_none() {
        return None;
    }
    let spawnflags = match runtime.object_field(node, "spawnflags") {
        Value::Int(flags) => flags,
        _ => 0,
    };
    let (from, to, min_enemy_dist) = cover_arc(&kind, spawnflags);
    let radians = angles[1].to_radians();
    Some(Cover {
        id: node,
        origin,
        yaw: angles[1],
        forward: [radians.cos(), radians.sin(), 0.0],
        bearings: (from, to),
        min_enemy_dist,
        priority: spawnflags & PRIORITY_COVER != 0,
        dangerous: false,
        clear_of_player: false,
        other_threats: 0,
    })
}

pub(super) fn covered_from(world: &mut World, id: u64, at: [f32; 3]) -> bool {
    if super::near_claim_node(world, &Value::Object(id), false) != Ok(Value::Int(1)) {
        return false;
    }
    let mut runtime = world.resource_mut::<RoundScript>();
    let Value::Object(node) = runtime.object_field(id, "node") else {
        return false;
    };
    cover_at(&mut runtime, node).is_some_and(|cover| covers_against(&cover, at))
}

const LOS_HALF_WIDTH: f32 = 25.0;
const LOS_RANGE: f32 = 150.0;
const LOS_STEADY_COS: f32 = 0.9848;
const LOS_PERIOD_MS: i64 = 750;
const LOS_MARK_MS: i64 = 1500;
const LOS_HEIGHT: f32 = 80.0;

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub(crate) struct PlayerLos {
    origin: [f32; 3],
    dir: [f32; 2],
    marked_ms: i64,
}

impl PlayerLos {
    fn steady(&mut self, origin: [f32; 3], dir: [f32; 2], now: i64) -> bool {
        let moved = (origin[0] - self.origin[0]).hypot(origin[1] - self.origin[1]);
        let turned = self.dir[0] * dir[0] + self.dir[1] * dir[1];
        if moved > LOS_HALF_WIDTH || turned < LOS_STEADY_COS {
            *self = PlayerLos {
                origin,
                dir,
                marked_ms: now,
            };
            return false;
        }
        for (eased, to) in self.origin.iter_mut().zip(origin) {
            *eased += (to - *eased) * 0.25;
        }
        for (eased, to) in self.dir.iter_mut().zip(dir) {
            *eased += (to - *eased) * 0.25;
        }
        if now - self.marked_ms < LOS_PERIOD_MS {
            return false;
        }
        self.marked_ms = now;
        true
    }
}

fn segment_distance(p: [f32; 3], a: [f32; 3], b: [f32; 3]) -> f32 {
    let (p, a, b) = (
        glam::Vec2::new(p[0], p[1]),
        glam::Vec2::new(a[0], a[1]),
        glam::Vec2::new(b[0], b[1]),
    );
    let ab = b - a;
    let t = if ab.length_squared() > 0.0 {
        ((p - a).dot(ab) / ab.length_squared()).clamp(0.0, 1.0)
    } else {
        0.0
    };
    p.distance(a + ab * t)
}

pub(super) fn player_attackers(world: &mut World) {
    if !crate::frame::FrameWorld::from_world(world)
        .bootstrap_ref()
        .campaign
    {
        return;
    }
    let mut runtime = world.resource_mut::<RoundScript>();
    let players: Vec<u64> = runtime.players.values().map(|slot| slot.object).collect();
    if players.is_empty() {
        return;
    }
    let mut counts = vec![0; players.len()];
    for id in super::live_actors(&runtime) {
        if super::health(&mut runtime, id) <= 0 {
            continue;
        }
        if let Value::Object(enemy) = runtime.object_field(id, "enemy")
            && let Some(k) = players.iter().position(|p| *p == enemy)
        {
            counts[k] += 1;
        }
    }
    for (player, count) in players.into_iter().zip(counts) {
        runtime.set_object_field(player, "attackercount", Value::Int(count));
        if runtime.object_field(player, "ignoreme") == Value::Undefined {
            runtime.set_object_field(player, "ignoreme", Value::Int(0));
        }
    }
}

pub(super) fn mark_player_los(world: &mut World) {
    let now = super::super::players::now_ms(world);
    let clients: Vec<u32> = world
        .resource::<RoundScript>()
        .players
        .keys()
        .copied()
        .collect();
    for client in clients {
        let Some((origin, height, angles)) = crate::frame::FrameWorld::from_world(world)
            .player(crate::world::ClientId(client))
            .map(|ps| (ps.origin, ps.view_height_current, ps.viewangles))
        else {
            continue;
        };
        let yaw = angles[1].to_radians();
        let steady = world
            .resource_mut::<RoundScript>()
            .campaign
            .player_los
            .entry(client)
            .or_default()
            .steady(origin, [yaw.cos(), yaw.sin()], now);
        if !steady {
            continue;
        }
        let eye = [origin[0], origin[1], origin[2] + height];
        let (forward, _, _) = math_iw4::angle_vectors(angles);
        let end: [f32; 3] = std::array::from_fn(|k| eye[k] + forward[k] * LOS_RANGE);
        let trace = super::super::presence::settled(world).trace_world(
            eye,
            end,
            [0.0; 3],
            [0.0; 3],
            crate::bullet_collision::MASK_SHOT,
        );
        let end: [f32; 3] = std::array::from_fn(|k| eye[k] + (end[k] - eye[k]) * trace.fraction);
        let mid_z = (eye[2] + end[2]) * 0.5;
        let mut runtime = world.resource_mut::<RoundScript>();
        let campaign = &mut runtime.campaign;
        for (node, at) in campaign.nodes.iter().zip(&campaign.node_origins) {
            if (at[2] - mid_z).abs() <= LOS_HEIGHT
                && segment_distance(*at, eye, end) <= LOS_HALF_WIDTH
            {
                campaign.node_los.insert(*node, now + LOS_MARK_MS);
            }
        }
    }
}

const DANGER_MARK_MS: i64 = 15_000;
const DANGER_REMARK_MS: i64 = 60_000;
const DANGER_NODE_RANGE: f32 = 192.0;

fn danger_team(team: &Value) -> Option<String> {
    match team {
        Value::String(team) => {
            let team = team.to_string();
            matches!(team.as_str(), "axis" | "allies" | "neutral").then_some(team)
        }
        _ => None,
    }
}

fn remark(until: Option<i64>, now: i64) -> i64 {
    now + if until.is_some_and(|until| until > now) {
        DANGER_REMARK_MS
    } else {
        DANGER_MARK_MS
    }
}

fn dangerous_until(until: Option<i64>, now: i64) -> bool {
    until.is_some_and(|until| until > now + DANGER_MARK_MS)
}

fn node_marked(runtime: &RoundScript, node: u64, team: &Value, now: i64) -> bool {
    danger_team(team).is_some_and(|team| {
        runtime
            .campaign
            .node_danger
            .get(&(node, team))
            .is_some_and(|until| *until > now)
    })
}

fn node_dangerous(runtime: &RoundScript, node: u64, team: &Value, now: i64) -> bool {
    danger_team(team).is_some_and(|team| {
        dangerous_until(
            runtime.campaign.node_danger.get(&(node, team)).copied(),
            now,
        )
    })
}

pub(super) fn mark_death_danger(world: &mut World, id: u64) {
    let now = super::super::players::now_ms(world);
    let mut runtime = world.resource_mut::<RoundScript>();
    let Some(team) = danger_team(&runtime.object_field(id, "team")) else {
        return;
    };
    let Some(at) = vector_field(&mut runtime, id, "origin") else {
        return;
    };
    let Some(index) = super::awareness::nearest_node(&runtime, at, DANGER_NODE_RANGE)
        .and_then(|node| runtime.campaign.nodes.iter().position(|n| *n == node))
    else {
        return;
    };
    let campaign = &mut runtime.campaign;
    let links = campaign.node_links.get(index).into_iter().flatten();
    let marked: Vec<u64> = std::iter::once(index)
        .chain(links.map(|k| usize::from(*k)))
        .filter_map(|k| campaign.nodes.get(k).copied())
        .collect();
    for node in marked {
        let until = campaign
            .node_danger
            .entry((node, team.clone()))
            .or_default();
        *until = remark(Some(*until), now);
    }
}

pub(super) fn mark_pain_danger(world: &mut World, id: u64) {
    let now = super::super::players::now_ms(world);
    let mut runtime = world.resource_mut::<RoundScript>();
    let Some(team) = danger_team(&runtime.object_field(id, "team")) else {
        return;
    };
    if let Value::Object(node) = runtime.object_field(id, "node") {
        runtime
            .campaign
            .node_danger
            .insert((node, team), now + DANGER_MARK_MS);
    }
}

pub(super) fn register(registry: &mut NativeRegistry) {
    use Namespace::{Function, Method};
    registry.register(Function, "badplace_cylinder", |world, _, args| {
        let at = vector(args, 2)?;
        let radius = number(arg(args, 3)?.clone()).unwrap_or(0.0);
        let height = number(arg(args, 4)?.clone()).unwrap_or(0.0);
        let bounds = (
            [at[0] - radius, at[1] - radius, at[2]],
            [at[0] + radius, at[1] + radius, at[2] + height],
        );
        add_badplace(world, args, bounds, 5)
    });
    registry.register(Function, "badplace_arc", |world, _, args| {
        let at = vector(args, 2)?;
        let radius = number(arg(args, 3)?.clone()).unwrap_or(0.0);
        let height = number(arg(args, 4)?.clone()).unwrap_or(0.0);
        let bounds = (
            [at[0] - radius, at[1] - radius, at[2]],
            [at[0] + radius, at[1] + radius, at[2] + height],
        );
        let teams_from = if args.len() > 6 { 7 } else { 5 };
        add_badplace(world, args, bounds, teams_from)
    });
    registry.register(Function, "badplace_brush", |world, _, args| {
        let id = entity_id(world, arg(args, 2)?)?;
        let (lo, hi) = entity_bounds(world, id);
        let at = match world
            .resource_mut::<RoundScript>()
            .object_field(id, "origin")
        {
            Value::Vector(at) => at,
            _ => [0.0; 3],
        };
        let bounds = (
            std::array::from_fn(|k| at[k] + lo[k]),
            std::array::from_fn(|k| at[k] + hi[k]),
        );
        add_badplace(world, args, bounds, 3)
    });
    registry.register(Function, "badplace_delete", |world, _, args| {
        let name = match arg(args, 0)? {
            Value::Int(n) => n.to_string(),
            _ => string(args, 0)?.to_string(),
        };
        let mut runtime = world.resource_mut::<RoundScript>();
        runtime
            .campaign
            .badplaces
            .retain(|place| place.name != name);
        for goal in runtime.campaign.actor_goals.values_mut() {
            goal.routed_to = None;
        }
        Ok(Value::Undefined)
    });
    registry.register(Function, "createthreatbiasgroup", |world, _, args| {
        let name = string(args, 0)?;
        world
            .resource_mut::<RoundScript>()
            .campaign
            .threat
            .groups
            .insert(name);
        Ok(Value::Undefined)
    });
    registry.register(Function, "threatbiasgroupexists", |world, _, args| {
        let name = string(args, 0)?;
        let runtime = world.resource::<RoundScript>();
        Ok(Value::Int(
            runtime.campaign.threat.groups.contains(&name).into(),
        ))
    });
    registry.register(Function, "setignoremegroup", |world, _, args| {
        let pair = (string(args, 0)?, string(args, 1)?);
        world
            .resource_mut::<RoundScript>()
            .campaign
            .threat
            .ignore
            .insert(pair);
        Ok(Value::Undefined)
    });
    registry.register(Function, "setthreatbias", |world, _, args| {
        let key = (string(args, 0)?, string(args, 1)?);
        let bias = number(arg(args, 2)?.clone()).ok_or("setthreatbias expects a number")?;
        world
            .resource_mut::<RoundScript>()
            .campaign
            .threat
            .bias
            .insert(key, bias);
        Ok(Value::Undefined)
    });
    registry.register(Function, "setthreatbiasagainstall", |world, _, args| {
        let target = string(args, 0)?;
        let bias =
            number(arg(args, 1)?.clone()).ok_or("setthreatbiasagainstall expects a number")?;
        world
            .resource_mut::<RoundScript>()
            .campaign
            .threat
            .set_against_all(&target, bias);
        Ok(Value::Undefined)
    });
    registry.register(Function, "getthreatbias", |world, _, args| {
        let (target, viewer) = (string(args, 0)?, string(args, 1)?);
        let runtime = world.resource::<RoundScript>();
        Ok(Value::Float(runtime.campaign.threat.bias(&viewer, &target)))
    });
    registry.register(Method, "setthreatbiasgroup", |world, receiver, args| {
        let id = entity_id(world, receiver)?;
        let group = string(args, 0)?;
        world.resource_mut::<RoundScript>().set_object_field(
            id,
            "threatbiasgroup",
            Value::string(&group),
        );
        Ok(Value::Undefined)
    });
    registry.register(Method, "getthreatbiasgroup", |world, receiver, _| {
        let id = entity_id(world, receiver)?;
        let group = group_of(&mut world.resource_mut::<RoundScript>(), id);
        Ok(Value::string(&group))
    });
    registry.register(Method, "setpotentialthreat", |world, receiver, args| {
        let id = actor(world, receiver)?;
        let threat_ent = entity_id(world, arg(args, 0)?)?;
        world.resource_mut::<RoundScript>().set_object_field(
            id,
            "potential_threat",
            Value::Object(threat_ent),
        );
        Ok(Value::Undefined)
    });
    registry.register(Method, "setcorpseremovetimer", |world, receiver, args| {
        let id = actor(world, receiver)?;
        let seconds = float(args, 0)?;
        world.resource_mut::<RoundScript>().set_object_field(
            id,
            "corpse_remove_timer",
            Value::Float(seconds),
        );
        Ok(Value::Undefined)
    });
    registry.register(Method, "disablebreaching", |world, receiver, _| {
        let id = actor(world, receiver)?;
        world.resource_mut::<RoundScript>().set_object_field(
            id,
            "breaching_disabled",
            Value::Int(1),
        );
        Ok(Value::Undefined)
    });
    registry.register(Method, "enablebreaching", |world, receiver, _| {
        let id = actor(world, receiver)?;
        world.resource_mut::<RoundScript>().set_object_field(
            id,
            "breaching_disabled",
            Value::Int(0),
        );
        Ok(Value::Undefined)
    });
    registry.register(Method, "castshadows", |world, receiver, _| {
        let id = entity_id(world, receiver)?;
        world
            .resource_mut::<RoundScript>()
            .set_object_field(id, "cast_shadows", Value::Int(1));
        Ok(Value::Undefined)
    });
    registry.register(Method, "dontcastshadows", |world, receiver, _| {
        let id = entity_id(world, receiver)?;
        world
            .resource_mut::<RoundScript>()
            .set_object_field(id, "cast_shadows", Value::Int(0));
        Ok(Value::Undefined)
    });
    registry.register(Method, "startragdollfromimpact", |world, receiver, _| {
        let _ = actor(world, receiver)?;
        Ok(Value::Undefined)
    });
    registry.register(Method, "settalktospecies", |world, receiver, args| {
        let id = actor(world, receiver)?;
        let species = string(args, 0)?;
        world.resource_mut::<RoundScript>().set_object_field(
            id,
            "talk_to_species",
            Value::string(&species),
        );
        Ok(Value::Undefined)
    });
    registry.register(Method, "makefakeai", |world, receiver, _| {
        let id = entity_id(world, receiver)?;
        world
            .resource_mut::<RoundScript>()
            .set_object_field(id, "is_fake_ai", Value::Int(1));
        Ok(Value::Undefined)
    });
    registry.register(Method, "findbestcovernode", |world, receiver, _| {
        let id = actor(world, receiver)?;
        let area = goal_area(world, id);
        let search = cover_search(&mut world.resource_mut::<RoundScript>(), id);
        Ok(find_cover(world, id, &area, &search, None).map_or(Value::Undefined, Value::Object))
    });
    registry.register(Method, "findshufflecovernode", |world, receiver, _| {
        let id = actor(world, receiver)?;
        let mut runtime = world.resource_mut::<RoundScript>();
        let center = vector_field(&mut runtime, id, "origin").unwrap_or([0.0; 3]);
        let skip = match runtime.object_field(id, "node") {
            Value::Object(node) => Some(node),
            _ => None,
        };
        drop(runtime);
        let area = Area::Sphere {
            center,
            radius: SHUFFLE_RADIUS,
            height: DEFAULT_GOAL_HEIGHT,
        };
        Ok(find_cover(world, id, &area, &area, skip).map_or(Value::Undefined, Value::Object))
    });
    registry.register(Method, "usecovernode", |world, receiver, args| {
        let id = actor(world, receiver)?;
        let (node, at) = origin_of(world, arg(args, 0)?)?;
        let mut runtime = world.resource_mut::<RoundScript>();
        let is_cover = matches!(runtime.object_field(node, "type"), Value::String(kind) if cover_script(&kind).is_some());
        if !is_cover || !free_for(&mut runtime, id, node) {
            return Ok(Value::Int(0));
        }
        runtime.set_object_field(node, "claimedby", Value::Object(id));
        drop(runtime);
        set_goal(world, receiver, GoalTarget::Pos(at), at, Some(node))?;
        Ok(Value::Int(1))
    });
    registry.register(Method, "atdangerousnode", |world, receiver, _| {
        let id = actor(world, receiver)?;
        if super::near_claim_node(world, receiver, false)? != Value::Int(1) {
            return Ok(Value::Int(0));
        }
        let now = super::super::players::now_ms(world);
        let mut runtime = world.resource_mut::<RoundScript>();
        let (Value::Object(node), team) = (
            runtime.object_field(id, "node"),
            runtime.object_field(id, "team"),
        ) else {
            return Ok(Value::Int(0));
        };
        Ok(Value::Int(
            node_dangerous(&runtime, node, &team, now).into(),
        ))
    });
    registry.register(Method, "iscovervalidagainstenemy", |world, receiver, _| {
        let id = actor(world, receiver)?;
        let Some(enemy) = enemy_origin(world, id) else {
            return Ok(Value::Int(0));
        };
        let mut runtime = world.resource_mut::<RoundScript>();
        let Value::Object(node) = runtime.object_field(id, "node") else {
            return Ok(Value::Int(0));
        };
        let valid = cover_at(&mut runtime, node).is_some_and(|cover| covers_against(&cover, enemy));
        Ok(Value::Int(valid.into()))
    });
    registry.register(Method, "getvalidcoverpeekouts", |world, receiver, _| {
        let node = entity_id(world, receiver)?;
        let cover = cover_at(&mut world.resource_mut::<RoundScript>(), node);
        let modes = cover.map_or_else(Vec::new, |cover| peekouts(world, &cover));
        super::super::arrays::new_array(world, modes.into_iter().map(Value::string).collect())
    });
    registry.register(Method, "isingoal", |world, receiver, args| {
        let id = actor(world, receiver)?;
        let at = vector(args, 0)?;
        let area = goal_area(world, id);
        Ok(Value::Int(inside(&area, at).into()))
    });
    registry.register(Method, "setgoalvolume", set_goal_volume);
    registry.register(Method, "setgoalvolumeauto", set_goal_volume);
    registry.register(Method, "getgoalvolume", |world, receiver, _| {
        let id = actor(world, receiver)?;
        Ok(world
            .resource_mut::<RoundScript>()
            .object_field(id, "goalvolume"))
    });
    registry.register(Method, "cleargoalvolume", |world, receiver, _| {
        let id = actor(world, receiver)?;
        world
            .resource_mut::<RoundScript>()
            .set_object_field(id, "goalvolume", Value::Undefined);
        Ok(Value::Undefined)
    });
    registry.register(Method, "setfixednodesafevolume", |world, receiver, args| {
        let id = actor(world, receiver)?;
        let vol = args.get(0).cloned().unwrap_or(Value::Undefined);
        world
            .resource_mut::<RoundScript>()
            .set_object_field(id, "fixednodesafevolume", vol);
        Ok(Value::Undefined)
    });
    registry.register(Method, "clearfixednodesafevolume", |world, receiver, _| {
        let id = actor(world, receiver)?;
        world.resource_mut::<RoundScript>().set_object_field(
            id,
            "fixednodesafevolume",
            Value::Undefined,
        );
        Ok(Value::Undefined)
    });
    registry.register(
        Method,
        "startusingheroonlylighting",
        |world, receiver, _| {
            let id = entity_id(world, receiver)?;
            world.resource_mut::<RoundScript>().set_object_field(
                id,
                "hero_only_lighting",
                Value::Int(1),
            );
            Ok(Value::Undefined)
        },
    );
    registry.register(
        Method,
        "startusinglessfrequentlighting",
        |world, receiver, _| {
            let id = entity_id(world, receiver)?;
            world.resource_mut::<RoundScript>().set_object_field(
                id,
                "less_frequent_lighting",
                Value::Int(1),
            );
            Ok(Value::Undefined)
        },
    );
    registry.register(Method, "getdebugeye", |world, receiver, _| {
        let id = actor(world, receiver)?;
        let mut runtime = world.resource_mut::<RoundScript>();
        let origin = vector_field(&mut runtime, id, "origin").unwrap_or([0.0; 3]);
        Ok(Value::Vector([origin[0], origin[1], origin[2] + 60.0]))
    });
    registry.register(Method, "getmuzzlesideoffsetpos", |world, receiver, _| {
        let id = actor(world, receiver)?;
        let mut runtime = world.resource_mut::<RoundScript>();
        let origin = vector_field(&mut runtime, id, "origin").unwrap_or([0.0; 3]);
        let angles = vector_field(&mut runtime, id, "angles").unwrap_or([0.0; 3]);
        let axis = math_iw4::angles_to_axis(angles);
        let side = [
            origin[0] + axis[1][0] * 12.0,
            origin[1] + axis[1][1] * 12.0,
            origin[2] + 40.0,
        ];
        Ok(Value::Vector(side))
    });
}

const PEEK_STEP: f32 = 32.0;
const PEEK_HEIGHT: f32 = 40.0;

fn peekouts(world: &mut World, cover: &Cover) -> Vec<&'static str> {
    let [fx, fy, _] = cover.forward;
    let from = [
        cover.origin[0],
        cover.origin[1],
        cover.origin[2] + PEEK_HEIGHT,
    ];
    [("left", -1.0), ("right", 1.0)]
        .into_iter()
        .filter(|(_, sign)| {
            let side = [
                from[0] + fy * sign * PEEK_STEP,
                from[1] - fx * sign * PEEK_STEP,
                from[2],
            ];
            let ahead = [side[0] + fx * PEEK_STEP, side[1] + fy * PEEK_STEP, side[2]];
            clear_sight(world, from, side) && clear_sight(world, side, ahead)
        })
        .map(|(mode, _)| mode)
        .collect()
}

fn set_goal_volume(world: &mut World, receiver: &Value, args: &[Value]) -> Result<Value, String> {
    let id = actor(world, receiver)?;
    let volume = entity_id(world, arg(args, 0)?)?;
    world
        .resource_mut::<RoundScript>()
        .set_object_field(id, "goalvolume", Value::Object(volume));
    let (lo, hi) = volume_box(world, volume);
    let mut runtime = world.resource_mut::<RoundScript>();
    let goal = vector_field(&mut runtime, id, "goalpos");
    let origin = vector_field(&mut runtime, id, "origin").unwrap_or([0.0; 3]);
    drop(runtime);
    let area = Area::Box(lo, hi);
    if goal.is_some_and(|at| inside(&area, at)) {
        return Ok(Value::Undefined);
    }
    let at: [f32; 3] = std::array::from_fn(|k| origin[k].clamp(lo[k], hi[k]));
    set_goal(world, receiver, GoalTarget::Pos(at), at, None)
}
