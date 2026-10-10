mod mines;
mod origins;
mod origins_dig;
mod origins_staff;
mod origins_tools;
mod origins_weather;
mod powerups;
mod rounds;

use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::sync::Arc;

use bevy_ecs::prelude::World;
use glam::Vec3;

use super::entities::{EntityKind, HudAudience};
use crate::frame::FrameWorld;
use crate::script::{Runtime, Value};
use crate::{ClientId, ClientLifecycle, MatchPhase, Tick};

const MAX_ALIVE: usize = 24;
const fn ticks(ms: u32) -> u32 {
    ms.div_ceil(crate::MATCH_TICK_MS)
}
const HULL_MIN: [f32; 3] = [-14.0, -14.0, 0.0];
const HULL_MAX: [f32; 3] = [14.0, 14.0, 64.0];
const MASK: u32 =
    crate::bullet_collision::MASK_PLAYER_SOLID & !crate::bullet_collision::CONTENTS_BODY;

#[derive(Clone, Debug, Default)]
pub(crate) struct Survival {
    pub authored: Arc<Vec<Vec<(String, String)>>>,
    initialized: bool,
    round: u32,
    started: Option<u32>,
    remaining: u32,
    next_spawn: u32,
    next_round: Option<u32>,
    ended: Option<u32>,
    actors: BTreeMap<u64, Actor>,
    survivors: BTreeMap<u32, Survivor>,
    nodes: Arc<Vec<[f32; 3]>>,
    edges: Arc<Vec<Vec<usize>>>,
    purchases: Vec<Purchase>,
    box_claims: BTreeMap<usize, BoxClaim>,
    model: Option<String>,
    walk: Vec<String>,
    idle: Option<String>,
    head: Option<(String, String)>,
    revives: BTreeMap<u32, (ClientId, u32)>,
    spawn_sites: Vec<SpawnSite>,
    starts: Vec<([f32; 3], [f32; 3])>,
    barriers: Vec<Barrier>,
    opened: BTreeSet<String>,
    powered: bool,
    attack: Vec<MeleeAnimation>,
    run_attack: Vec<MeleeAnimation>,
    run: Vec<String>,
    entry: Option<String>,
    entry_curve: Option<Arc<xmodel_runtime::AnimClip>>,
    pending_links: Vec<(u32, [f32; 3])>,
    upgrades: BTreeMap<usize, BoxClaim>,
    rise: Option<String>,
    tear: Option<String>,
    rise_ticks: u32,
    entry_ticks: u32,
    origins: origins::PowerGrid,
    tools: origins_tools::Tools,
    weather: origins_weather::Weather,
    digs: origins_dig::Digs,
    staffs: origins_staff::Staffs,
    powerups: powerups::Powerups,
}

#[derive(Clone, Debug)]
struct Actor {
    origin: [f32; 3],
    path: VecDeque<usize>,
    repath: u32,
    attack_due: u32,
    velocity: [f32; 3],
    barrier: Option<usize>,
    entering: Option<(u32, [f32; 3])>,
    swing: Option<Swing>,
    animation: u8,
    stalled: u32,
    emerge_until: u32,
    walk: Option<String>,
    run: Option<String>,
}

#[derive(Clone, Debug)]
struct MeleeAnimation {
    name: String,
    duration: u32,
    impacts: Arc<[u32]>,
}

#[derive(Clone, Debug)]
struct Swing {
    victim: ClientId,
    start: u32,
    next: usize,
    finish: u32,
    impacts: Arc<[u32]>,
}

#[derive(Clone, Debug, Default)]
struct Survivor {
    guns: Vec<u32>,
    perks: BTreeSet<String>,
    use_held: bool,
    hud: Option<u64>,
    prompt: Option<u64>,
    last_prompt: String,
    round_label: Option<u64>,
    last_round: u32,
    downed_since: Option<u32>,
    solo_revives_bought: u8,
    perk_label: Option<u64>,
    last_perks: String,
    staff_label: Option<u64>,
    last_staffs: String,
    repair_due: u32,
    repair_round: u32,
    repair_points: i32,
    generator_label: Option<u64>,
    last_generators: String,
    powerup_label: Option<u64>,
    last_powerups: String,
}

#[derive(Clone, Debug)]
struct Purchase {
    origin: [f32; 3],
    kind: PurchaseKind,
    price: i32,
    needs_power: bool,
    model: Option<(String, [f32; 3])>,
    object: Option<u64>,
}

#[derive(Clone, Debug)]
enum PurchaseKind {
    Weapon(String),
    Box,
    Upgrade,
    Perk(String),
    Door(String),
    Power,
    Generator(u8),
}

#[derive(Clone, Debug)]
struct SpawnSite {
    origin: [f32; 3],
    barrier: Option<usize>,
    riser: bool,
}

#[derive(Clone, Debug)]
struct Barrier {
    origin: [f32; 3],
    outside: [f32; 3],
    inside: [f32; 3],
    angles: [f32; 3],
    models: Vec<String>,
    objects: Vec<Option<u64>>,
    boards: u8,
    tear_due: u32,
    board_origins: Vec<[f32; 3]>,
    hiding: Vec<Option<u32>>,
}

#[derive(Clone, Debug)]
struct BoxClaim {
    client: ClientId,
    weapon: u32,
    ready: u32,
    expires: u32,
}

pub(crate) fn active(world: &mut World) -> bool {
    FrameWorld::from_world(world).bootstrap_ref().kind == gamemode_iw4::GameModeKind::Zclassic
}

fn field<'a>(pairs: &'a [(String, String)], name: &str) -> &'a str {
    pairs
        .iter()
        .find(|(key, _)| key == name)
        .map_or("", |(_, value)| value)
}

fn point(text: &str) -> Option<[f32; 3]> {
    let values: Vec<f32> = text
        .split_whitespace()
        .map(str::parse)
        .collect::<Result<_, _>>()
        .ok()?;
    let result: [f32; 3] = values.try_into().ok()?;
    result.iter().all(|n| n.is_finite()).then_some(result)
}

fn ground(frame: &FrameWorld, at: [f32; 3]) -> Option<[f32; 3]> {
    let mut start = at;
    start[2] += 18.0;
    let mut end = at;
    end[2] -= 96.0;
    let trace = frame.trace_world(start, end, HULL_MIN, HULL_MAX, MASK);
    (trace.fraction < 1.0 && trace.startsolid == 0 && trace.allsolid == 0 && trace.normal[2] >= 0.7)
        .then(|| [trace.endpos[0], trace.endpos[1], trace.endpos[2] + 0.1])
}

fn clear(frame: &FrameWorld, a: [f32; 3], b: [f32; 3]) -> bool {
    let mut start = a;
    let mut end = b;
    start[2] += 18.0;
    end[2] += 18.0;
    let trace = frame.trace_world(start, end, HULL_MIN, HULL_MAX, MASK);
    trace.fraction >= 1.0 && trace.startsolid == 0 && trace.allsolid == 0
}

struct ActorCollision<'a, 'w>(&'a FrameWorld<'w>);
impl movement_iw4::CollisionBackend for ActorCollision<'_, '_> {
    fn trace(&self, input: movement_iw4::GroundTraceInput) -> trace_iw4::Trace {
        self.0.trace_world(
            input.start,
            input.end,
            input.mins,
            input.maxs,
            input.tracemask,
        )
    }
}

fn traverse(frame: &FrameWorld, actor: &mut Actor, direction: Vec3, speed: f32) {
    let mut motion = playerstate_iw4::PlayerState {
        origin: actor.origin,
        velocity: [direction.x * speed, direction.y * speed, actor.velocity[2]],
        ground_entity_num: playerstate_iw4::ENTITYNUM_NONE,
        ..playerstate_iw4::PlayerState::ZERO
    };
    let mut step = movement_iw4::Pml {
        forward: direction.to_array(),
        right: [0.0; 3],
        up: [0.0, 0.0, 1.0],
        frametime: crate::MATCH_TICK_MS as f32 / 1000.0,
        msec: crate::MATCH_TICK_MS as i32,
        walking: 0,
        ground_plane: 0,
        almost_ground_plane: 0,
        ground_trace: [0; 11],
        previous_origin: actor.origin,
        previous_velocity: actor.velocity,
        holdrand: 0,
        jump_animations: [None; 4],
        mantle_movetype: None,
        landing_animation: false,
        fall_damage: 0,
    };
    let bounds = movement_iw4::MoveBounds {
        mins: HULL_MIN,
        maxs: HULL_MAX,
        tracemask: MASK,
    };
    let collision = ActorCollision(frame);
    movement_iw4::complete_ground_trace(&mut motion, &mut step, bounds, 127, &collision);
    movement_iw4::step_slide_move(
        &mut motion,
        &step,
        &collision,
        HULL_MIN,
        HULL_MAX,
        MASK,
        Some(800.0),
    );
    if motion
        .origin
        .iter()
        .chain(&motion.velocity)
        .all(|value| value.is_finite())
    {
        actor.origin = motion.origin;
        actor.velocity = motion.velocity;
    }
}

fn reachable_node(frame: &FrameWorld, nodes: &[[f32; 3]], at: [f32; 3]) -> Option<usize> {
    let mut candidates: Vec<_> = nodes
        .iter()
        .enumerate()
        .map(|(i, node)| {
            (
                i,
                Vec3::from_array(*node).distance_squared(Vec3::from_array(at)),
            )
        })
        .collect();
    candidates.sort_by(|a, b| a.1.total_cmp(&b.1));
    candidates
        .into_iter()
        .take(12)
        .find(|&(i, distance)| distance <= 256.0 * 256.0 && clear(frame, at, nodes[i]))
        .map(|(i, _)| i)
}

fn route(state: &Survival, start: usize, goal: usize) -> VecDeque<usize> {
    let mut parents = vec![usize::MAX; state.nodes.len()];
    let mut queue = VecDeque::from([start]);
    parents[start] = start;
    while let Some(node) = queue.pop_front() {
        if node == goal {
            break;
        }
        for &next in &state.edges[node] {
            if parents[next] == usize::MAX {
                parents[next] = node;
                queue.push_back(next);
            }
        }
    }
    if parents[goal] == usize::MAX {
        return VecDeque::new();
    }
    let mut path = VecDeque::new();
    let mut node = goal;
    while node != start {
        path.push_front(node);
        node = parents[node];
    }
    path
}

fn initialize(world: &mut World, state: &mut Survival) {
    state.tools.initialize(world, &state.authored);
    state.digs.initialize(&state.authored);
    state.staffs.initialize(world, &state.authored);
    let mut frame = FrameWorld::from_world(world);
    for client in frame.client_ids_sorted() {
        let meta = frame.client_meta_mut(client);
        meta.score = 0;
        meta.kills = 0;
        meta.deaths = 0;
    }
    state.model = frame.zombie_body_model();
    let states = frame.script_model_states();
    let candidates = |state_name: &str, fallback: Vec<String>| {
        let clips: Vec<_> = states
            .as_ref()
            .into_iter()
            .flat_map(|table| table.clips(state_name, None))
            .filter(|name| frame.script_model_anim(name).is_some())
            .map(str::to_owned)
            .collect();
        if clips.is_empty() { fallback } else { clips }
    };
    state.walk = candidates(
        "zm_move_walk",
        frame.zombie_walk_anim().into_iter().collect(),
    );
    state.head = state
        .model
        .as_deref()
        .and_then(|model| frame.zombie_head_attachment(model));
    let available = |names: &[&str]| {
        names
            .iter()
            .copied()
            .filter(|name| frame.script_model_anim(name).is_some())
            .map(str::to_owned)
            .collect()
    };
    state.run = candidates(
        "zm_move_run",
        available(&["ai_zombie_run_v2", "ai_zombie_run_v4"]),
    );
    state.idle = candidates(
        "zm_idle",
        available(&["ai_zombie_idle_v1_delta", "ai_zombie_idle_v1"]),
    )
    .into_iter()
    .next();
    let melee = |names: Vec<String>| {
        names
            .into_iter()
            .filter_map(|name| {
                let clip = frame.script_model_clips().get(&name).cloned()?;
                let (duration, impacts) = melee_timing(&clip);
                (!impacts.is_empty() && duration > 0).then(|| MeleeAnimation {
                    name,
                    duration,
                    impacts: impacts.into(),
                })
            })
            .collect()
    };
    state.attack = melee(candidates(
        "zm_walk_melee",
        available(&["ai_zombie_attack_v1", "ai_zombie_attack_v2"]),
    ));
    state.run_attack = melee(candidates("zm_run_melee", Vec::new()));
    state.entry = states
        .as_ref()
        .and_then(|table| {
            table
                .clips("zm_barricade_enter", Some("barrier_walk_m"))
                .find(|name| frame.script_model_anim(name).is_some())
                .map(str::to_owned)
        })
        .or_else(|| {
            ["ai_zombie_traverse_v1", "ai_zombie_traverse_v2"]
                .into_iter()
                .find(|name| frame.script_model_anim(name).is_some())
                .map(str::to_owned)
        });
    state.entry_curve = state
        .entry
        .as_deref()
        .and_then(|name| frame.script_model_clips().get(name).cloned());
    state.powered = !state.authored.iter().any(|pairs| {
        matches!(
            field(pairs, "targetname"),
            "use_elec_switch"
                | "powerswitch_buildable_trigger_power"
                | "afterlife_interact"
                | "s_generator"
        )
    });
    state.rise = states
        .as_ref()
        .and_then(|table| {
            table
                .clips("zm_rise", None)
                .find(|name| frame.script_model_anim(name).is_some())
                .map(str::to_owned)
        })
        .or_else(|| {
            [
                "ai_zombie_traverse_ground_v1_walk",
                "ai_zombie_traverse_ground_climbout_fast",
            ]
            .into_iter()
            .find(|name| frame.script_model_anim(name).is_some())
            .map(str::to_owned)
        });
    state.tear = [
        "ai_zombie_boardtear_aligned_m_1_pull",
        "ai_zombie_boardtear_aligned_m_1_grab",
    ]
    .into_iter()
    .find(|name| frame.script_model_anim(name).is_some())
    .map(str::to_owned);
    let duration = |clip: &Option<String>, fallback| {
        clip.as_deref()
            .and_then(|clip| frame.script_model_anim(clip))
            .filter(|anim| anim.frequency.is_finite() && anim.frequency > 0.0)
            .map_or(ticks(fallback), |anim| {
                ticks((1000.0 / anim.frequency).ceil() as u32)
            })
    };
    state.rise_ticks = duration(&state.rise, 1800);
    state.entry_ticks = duration(&state.entry, 1100);
    let mut barrier_names = BTreeMap::new();
    for pairs in state
        .authored
        .iter()
        .filter(|pairs| field(pairs, "classname").starts_with("zbarrier_zmcore_BasicWoodBarrier"))
    {
        let Some(origin) = point(field(pairs, "origin")) else {
            continue;
        };
        let name = field(pairs, "script_string");
        let linked = state
            .authored
            .iter()
            .filter(|row| {
                field(row, "classname") == "node_negotiation_begin"
                    && field(row, "animscript").contains("mantle_over")
            })
            .filter_map(|row| Some((row, point(field(row, "origin"))?)))
            .filter(|(_, at)| {
                Vec3::from_array(*at).distance_squared(Vec3::from_array(origin)) < 128.0 * 128.0
            })
            .min_by(|(_, a), (_, b)| {
                Vec3::from_array(*a)
                    .distance_squared(Vec3::from_array(origin))
                    .total_cmp(&Vec3::from_array(*b).distance_squared(Vec3::from_array(origin)))
            })
            .and_then(|(begin, at)| {
                state
                    .authored
                    .iter()
                    .find(|row| {
                        field(row, "classname") == "node_negotiation_end"
                            && field(row, "targetname") == field(begin, "target")
                    })
                    .and_then(|end| point(field(end, "origin")))
                    .map(|end| (at, end))
            });
        let outside = state
            .authored
            .iter()
            .filter(|row| {
                !name.is_empty()
                    && field(row, "targetname") == "exterior_goal"
                    && field(row, "script_string") == name
            })
            .filter_map(|row| point(field(row, "origin")))
            .min_by(|a, b| {
                Vec3::from_array(*a)
                    .distance_squared(Vec3::from_array(origin))
                    .total_cmp(&Vec3::from_array(*b).distance_squared(Vec3::from_array(origin)))
            })
            .or(linked.map(|pair| pair.0))
            .unwrap_or(origin);
        let inside = state
            .authored
            .iter()
            .filter(|row| {
                !name.is_empty()
                    && field(row, "classname") == "node_negotiation_end"
                    && field(row, "script_string") == name
            })
            .filter_map(|row| point(field(row, "origin")))
            .min_by(|a, b| {
                Vec3::from_array(*a)
                    .distance_squared(Vec3::from_array(origin))
                    .total_cmp(&Vec3::from_array(*b).distance_squared(Vec3::from_array(origin)))
            })
            .or(linked.map(|pair| pair.1));
        let Some((outside, inside)) =
            ground(&frame, outside).zip(inside.and_then(|at| ground(&frame, at)))
        else {
            continue;
        };
        if Vec3::from_array(outside).distance(Vec3::from_array(inside)) > 192.0 {
            continue;
        }
        let models: Vec<_> = (1..=6)
            .map(|i| field(pairs, &format!("zbarrierboardmodel{i}")).to_owned())
            .filter(|name| !name.is_empty())
            .collect();
        if models.is_empty() {
            continue;
        }
        if !name.is_empty() {
            barrier_names.insert(name.to_owned(), state.barriers.len());
        }
        let angles = point(field(pairs, "angles")).unwrap_or([0.0; 3]);
        let anchors = frame
            .model_capability("p6_anim_zm_barricade_board_collision")
            .flatten()
            .map(|capability| {
                crate::AuthorityDObjState::at_pose(
                    "p6_anim_zm_barricade_board_collision",
                    Some(capability),
                    origin,
                    angles,
                )
            });
        let board_origins = (1..=models.len())
            .map(|i| {
                anchors
                    .as_ref()
                    .and_then(|dobj| dobj.tag_world_pose(&format!("tag_board_{i}")))
                    .map_or(origin, |(at, _)| at)
            })
            .collect();
        state.barriers.push(Barrier {
            origin,
            outside,
            inside,
            angles: point(field(pairs, "angles")).unwrap_or([0.0; 3]),
            boards: models.len() as u8,
            objects: vec![None; models.len()],
            hiding: vec![None; models.len()],
            board_origins,
            models,
            tear_due: 0,
        });
    }
    let mut nodes = Vec::new();
    for pairs in state.authored.iter() {
        let target = field(pairs, "targetname");
        let weapon = field(pairs, "zombie_weapon_upgrade");
        let Some(origin) = point(field(pairs, "origin")) else {
            continue;
        };
        if field(pairs, "classname") == "node_pathnode" {
            if let Some(origin) = ground(&frame, origin)
                && !nodes.iter().any(|&other| {
                    Vec3::from_array(origin).distance_squared(Vec3::from_array(other)) < 64.0
                })
            {
                nodes.push(origin);
            }
        }
        let note = field(pairs, "script_noteworthy");
        if target == "initial_spawn_points"
            && (field(pairs, "script_string").is_empty()
                || field(pairs, "script_string")
                    .split_whitespace()
                    .any(|mode| mode.starts_with("zclassic") || mode == "zstandard_nuked"))
        {
            if let Some(at) = ground(&frame, origin) {
                state
                    .starts
                    .push((at, point(field(pairs, "angles")).unwrap_or([0.0; 3])));
            }
        }
        if matches!(note, "spawn_location" | "riser_location") && target.contains("spawner") {
            if let Some(origin) = ground(&frame, origin) {
                state.spawn_sites.push(SpawnSite {
                    origin,
                    barrier: barrier_names.get(field(pairs, "script_string")).copied(),
                    riser: note == "riser_location",
                });
            }
        }
        let filters = field(pairs, "script_string");
        if target == "zm_perk_machine"
            && !filters.is_empty()
            && !filters
                .split_whitespace()
                .any(|s| s.starts_with("zclassic"))
        {
            continue;
        }
        let kind = if target == "zm_perk_machine" {
            match note {
                "specialty_weapupgrade" => Some(PurchaseKind::Upgrade),
                "specialty_armorvest" => Some(PurchaseKind::Perk("juggernog".into())),
                "specialty_fastreload" => Some(PurchaseKind::Perk("sleight".into())),
                "specialty_quickrevive" => Some(PurchaseKind::Perk("revive".into())),
                "specialty_rof" => Some(PurchaseKind::Perk("doubletap".into())),
                "specialty_longersprint" => Some(PurchaseKind::Perk("staminup".into())),
                "specialty_additionalprimaryweapon" => Some(PurchaseKind::Perk("mulekick".into())),
                "specialty_ads_zombies" | "specialty_deadshot" => {
                    Some(PurchaseKind::Perk("deadshot".into()))
                }
                "specialty_flakjacket" => Some(PurchaseKind::Perk("phd".into())),
                "specialty_grenadepulldeath" => Some(PurchaseKind::Perk("electriccherry".into())),
                "specialty_nomotionsensor" => Some(PurchaseKind::Perk("vultureaid".into())),
                "specialty_scavenger" => Some(PurchaseKind::Perk("whoswho".into())),
                _ => None,
            }
        } else if matches!(target, "zombie_door" | "zombie_debris")
            && field(pairs, "zombie_cost")
                .parse::<i32>()
                .is_ok_and(|cost| cost > 0)
            && !field(pairs, "target").is_empty()
        {
            Some(PurchaseKind::Door(field(pairs, "target").to_owned()))
        } else if target == "s_generator" {
            field(pairs, "script_int")
                .parse::<u8>()
                .ok()
                .filter(|number| (1..=6).contains(number))
                .map(PurchaseKind::Generator)
        } else if target == "use_elec_switch" {
            Some(PurchaseKind::Power)
        } else if target == "perksacola" && field(pairs, "script_sound") == "mx_packa_jingle" {
            Some(PurchaseKind::Upgrade)
        } else if !weapon.is_empty() {
            Some(PurchaseKind::Weapon(weapon.to_owned()))
        } else if target == "treasure_chest_use" {
            Some(PurchaseKind::Box)
        } else if target == "zombie_vending_upgrade" {
            Some(PurchaseKind::Upgrade)
        } else if target.starts_with("zombie_vending_") {
            Some(PurchaseKind::Perk(
                target.trim_start_matches("zombie_vending_").to_owned(),
            ))
        } else {
            None
        };
        if let Some(kind) = kind {
            let fallback = match &kind {
                PurchaseKind::Weapon(name) => match name.as_str() {
                    "m14_zm" | "rottweil72_zm" => 500,
                    "mp5k_zm" | "870mcs_zm" => 1200,
                    "m16_zm" => 1200,
                    _ => 1000,
                },
                PurchaseKind::Door(_) => 750,
                PurchaseKind::Power => 0,
                PurchaseKind::Generator(_) => 200,
                PurchaseKind::Box => 950,
                PurchaseKind::Upgrade => 5000,
                PurchaseKind::Perk(name) => match name.as_str() {
                    "juggernog" => 2500,
                    "sleight" => 3000,
                    "doubletap" | "staminup" | "phd" => 2000,
                    "mulekick" => 4000,
                    _ => 1500,
                },
            };
            let price = field(pairs, "zombie_cost")
                .parse()
                .ok()
                .filter(|n| *n > 0)
                .unwrap_or(fallback);
            state.purchases.push(Purchase {
                origin,
                model: (!field(pairs, "model").is_empty()
                    && matches!(kind, PurchaseKind::Perk(_) | PurchaseKind::Upgrade))
                .then(|| {
                    (
                        field(pairs, "model").to_owned(),
                        point(field(pairs, "angles")).unwrap_or([0.0; 3]),
                    )
                }),
                object: None,
                needs_power: matches!(&kind, PurchaseKind::Perk(name) if name != "revive")
                    || matches!(kind, PurchaseKind::Upgrade),
                kind,
                price,
            });
        }
    }
    state.origins.initialize(&state.authored);
    let mut edges = vec![Vec::new(); nodes.len()];
    for (i, &a) in nodes.iter().enumerate() {
        let mut neighbors: Vec<_> = nodes
            .iter()
            .enumerate()
            .filter_map(|(j, &b)| {
                let distance = Vec3::from_array(a).distance_squared(Vec3::from_array(b));
                (i != j && distance < 256.0 * 256.0 && (a[2] - b[2]).abs() <= 48.0)
                    .then_some((j, distance))
            })
            .collect();
        neighbors.sort_by(|a, b| a.1.total_cmp(&b.1));
        for &(j, _) in neighbors.iter().take(12) {
            if clear(&frame, a, nodes[j]) {
                edges[i].push(j);
            }
        }
    }
    for (index, barrier) in state.barriers.iter().enumerate() {
        state.spawn_sites.push(SpawnSite {
            origin: barrier.outside,
            barrier: Some(index),
            riser: false,
        });
    }
    drop(frame);
    state.weather.initialize(world, state.origins.present());
    state.nodes = Arc::new(nodes);
    state.edges = Arc::new(edges);
    state.initialized = true;
    state.next_round = Some(0);
    diag::info!(
        Sim,
        "zombies initialized nodes={} purchases={} body={:?} walk={:?} sites={} barriers={} starts={} powered={}",
        state.nodes.len(),
        state.purchases.len(),
        state.model,
        state.walk,
        state.spawn_sites.len(),
        state.barriers.len(),
        state.starts.len(),
        state.powered
    );
}

fn weapon_id(frame: &FrameWorld, name: &str) -> Option<u32> {
    frame
        .weapon_script_names()
        .iter()
        .position(|n| n == name)
        .and_then(|n| u32::try_from(n).ok())
        .filter(|&n| frame.weapon_runnable(n))
}

fn score(world: &mut World, client: ClientId, amount: i32) {
    let tick = world.resource::<crate::step::StepRequest>().tick;
    let mut frame = FrameWorld::from_world(world);
    let Some(meta) = frame.client_meta(client) else {
        return;
    };
    let (points, kills, deaths) = (meta.score.saturating_add(amount), meta.kills, meta.deaths);
    frame.client_meta_mut(client).score = points;
    frame.push_event(
        tick,
        crate::EventAudience::All,
        crate::SimEvent::ScoreChanged {
            client,
            score: points,
            kills,
            deaths,
        },
    );
}

fn spawn_player(world: &mut World, state: &mut Survival, client: ClientId, tick: Tick) {
    let mut frame = FrameWorld::from_world(world);
    let avoid: Vec<_> = frame
        .client_ids_sorted()
        .into_iter()
        .filter(|id| *id != client)
        .filter_map(|id| frame.player(id).map(|ps| ps.origin))
        .collect();
    let report = crate::spawn::decide_forced_spawn(
        &frame,
        crate::SpawnPick::Seeded(u64::from(client.0) + u64::from(tick.0)),
        &avoid,
        entity_iw4::TEAM_FREE,
    );
    let authored = (0..state.starts.len())
        .map(|i| state.starts[(i + client.0 as usize) % state.starts.len()])
        .find(|(at, _)| {
            avoid
                .iter()
                .all(|other| Vec3::from_array(*at).distance(Vec3::from_array(*other)) > 48.0)
        });
    let spawn = authored.or_else(|| {
        report
            .accepted
            .map(|spawn| (spawn.traced_origin, spawn.raw_angles))
    });
    let Some((origin, angles)) = spawn else {
        return;
    };
    let starter = if state.origins.present() {
        "c96_zm"
    } else {
        "m1911_zm"
    };
    let Some(pistol) = weapon_id(&frame, starter) else {
        return;
    };
    let pistol_name = frame.weapon_script_name(pistol).to_owned();
    frame.client_meta_mut(client).client_state_team = entity_iw4::TEAM_FREE;
    frame.client_meta_mut(client).max_health = 100;
    crate::script_player::spawn(&mut frame, tick, client, origin, angles, "playing");
    if crate::script_player::give_weapon(&mut frame, client, pistol, false).is_ok() {
        let _ = crate::script_player::set_spawn_weapon(&mut frame, client, pistol);
    }
    if let Some(slot) = frame
        .ecs()
        .resource_mut::<Runtime>()
        .players
        .get_mut(&client.0)
    {
        slot.sessionstate = "playing".into();
        slot.begun = true;
    }
    let survivor = state.survivors.entry(client.0).or_default();
    survivor.guns = vec![pistol];
    survivor.perks.clear();
    survivor.downed_since = None;
    survivor.use_held = false;
    state.revives.remove(&client.0);
    let points = frame.client_meta(client).map_or(0, |meta| meta.score);
    drop(frame);
    if points < 500 {
        score(world, client, 500 - points);
    }
    diag::info!(
        Sim,
        "zombies survivor spawned client={} pistol={pistol_name}",
        client.0
    );
}

fn spawn_actor(
    world: &mut World,
    state: &mut Survival,
    tick: Tick,
    players: &[(ClientId, [f32; 3])],
) -> bool {
    let frame = FrameWorld::from_world(world);
    let fallback: Vec<_> = state
        .nodes
        .iter()
        .map(|&origin| SpawnSite {
            origin,
            barrier: None,
            riser: false,
        })
        .collect();
    let sites = if state.spawn_sites.is_empty() {
        &fallback
    } else {
        &state.spawn_sites
    };
    let offset = tick.0 as usize % sites.len().max(1);
    let site = (0..sites.len())
        .map(|i| &sites[(i + offset) % sites.len()])
        .find(|site| {
            let at = site.origin;
            players.iter().all(|(_, player)| {
                Vec3::from_array(at).distance(Vec3::from_array(*player)) >= 192.0
            }) && players.iter().any(|(_, player)| {
                let distance = Vec3::from_array(at).distance(Vec3::from_array(*player));
                if distance > 1800.0 {
                    return false;
                }
                let start = site
                    .barrier
                    .map_or(at, |index| state.barriers[index].inside);
                if (at[2] - start[2]).abs() > 128.0 || (at[2] - player[2]).abs() > 160.0 {
                    return false;
                }
                if site.barrier.is_some() && reachable_node(&frame, &state.nodes, at).is_none() {
                    return false;
                }
                reachable_node(&frame, &state.nodes, *player)
                    .zip(reachable_node(&frame, &state.nodes, start))
                    .is_some_and(|(goal, start)| {
                        start == goal || !route(state, start, goal).is_empty()
                    })
            }) && state.actors.values().all(|actor| {
                Vec3::from_array(actor.origin).distance_squared(Vec3::from_array(at)) > 48.0 * 48.0
            })
        })
        .cloned();
    let Some(site) = site else { return false };
    drop(frame);
    spawn_actor_site(world, state, tick, site)
}

fn spawn_actor_site(world: &mut World, state: &mut Survival, tick: Tick, site: SpawnSite) -> bool {
    let Some(model) = &state.model else {
        return false;
    };
    let mut frame = FrameWorld::from_world(world);
    let origin = site.origin;
    let walk = choose_animation(&mut frame, &state.walk);
    let run = choose_animation(&mut frame, &state.run);
    drop(frame);
    let Ok(presence) = super::presence::spawn_presence(world, origin) else {
        return false;
    };
    let number = FrameWorld::from_world(world).gentity_number(presence);
    let mut runtime = world.resource_mut::<Runtime>();
    let Ok(object) = runtime.create_entity(EntityKind::Spawned, "actor_zombie") else {
        return false;
    };
    runtime.set_object_field(object, "origin", Value::Vector(origin));
    runtime.set_object_field(object, "model", Value::string(model));
    let health = rounds::health(state.round);
    runtime.set_object_field(object, "health", Value::Int(health));
    let entity = runtime.entities.get_mut(&object).unwrap();
    entity.presence = Some(presence);
    if let Some((head, tag)) = &state.head {
        entity
            .attachments
            .push((head.as_str().into(), tag.as_str().into()));
    }
    entity.contents = crate::bullet_collision::CONTENTS_BODY as i32;
    entity.can_damage = true;
    entity.can_radius_damage = true;
    if let Some(number) = number {
        entity.number = number;
    }
    entity.anim_op = if site.riser {
        state.rise.as_ref().or(walk.as_ref())
    } else {
        walk.as_ref()
    }
    .map(|clip| Some(clip.as_str().into()));
    state.actors.insert(
        object,
        Actor {
            origin,
            path: VecDeque::new(),
            repath: 0,
            attack_due: tick.0 + ticks(1000),
            velocity: [0.0; 3],
            barrier: site.barrier,
            entering: None,
            swing: None,
            animation: if site.riser { 6 } else { 1 },
            stalled: 0,
            emerge_until: if site.riser {
                tick.0 + state.rise_ticks
            } else {
                0
            },
            walk,
            run,
        },
    );
    diag::info!(
        Sim,
        "zombie spawned object={object} round={} health={health} origin={origin:?}",
        state.round
    );
    true
}

fn choose_animation<T: Clone>(frame: &mut FrameWorld, choices: &[T]) -> Option<T> {
    if choices.is_empty() {
        return None;
    }
    let index = frame.combat_rng_mut().next_u32() as usize % choices.len();
    Some(choices[index].clone())
}

fn animate(world: &mut World, object: u64, actor: &mut Actor, mode: u8, clip: Option<&str>) {
    if actor.animation == mode {
        return;
    }
    actor.animation = mode;
    if let Some(clip) = clip {
        if let Some(entity) = world.resource_mut::<Runtime>().entities.get_mut(&object) {
            entity.anim_op = Some(Some(clip.into()));
        }
    }
}

fn melee_timing(clip: &xmodel_runtime::AnimClip) -> (u32, Vec<u32>) {
    let duration = clip.duration();
    if !duration.is_finite() || duration <= 0.0 {
        return (0, Vec::new());
    }
    let end = clip
        .notifies
        .iter()
        .filter(|note| note.name == "end" && note.time.is_finite())
        .map(|note| note.time.clamp(0.0, 1.0))
        .min_by(f32::total_cmp)
        .unwrap_or(1.0);
    let to_ticks =
        |time: f32| (time * duration * 1000.0 / crate::MATCH_TICK_MS as f32).ceil() as u32;
    let mut impacts: Vec<_> = clip
        .notifies
        .iter()
        .filter(|note| {
            note.name == "fire" && note.time.is_finite() && note.time >= 0.0 && note.time <= end
        })
        .map(|note| to_ticks(note.time))
        .collect();
    impacts.sort_unstable();
    (to_ticks(end), impacts)
}

fn window_origin(
    from: [f32; 3],
    to: [f32; 3],
    progress: f32,
    clip: Option<&xmodel_runtime::AnimClip>,
) -> [f32; 3] {
    let progress = progress.clamp(0.0, 1.0);
    let start = Vec3::from_array(from);
    let end = Vec3::from_array(to);
    let linear = start.lerp(end, progress);
    let Some(clip) = clip else {
        return linear.to_array();
    };
    let base = Vec3::from_array(clip.abs_delta_trans(0.0));
    let total = Vec3::from_array(clip.abs_delta_trans(1.0)) - base;
    let current = Vec3::from_array(clip.abs_delta_trans(progress)) - base;
    let residual = current - total * progress;
    let forward = Vec3::new(end.x - start.x, end.y - start.y, 0.0).normalize_or_zero();
    let right = Vec3::new(-forward.y, forward.x, 0.0);
    (linear + forward * residual.x + right * residual.y + Vec3::Z * residual.z).to_array()
}

fn board_visibility(world: &mut World, barrier: &mut Barrier, board: usize, visible: bool) {
    let clip = format!(
        "o_zombie_board_{}_{}",
        board + 1,
        if visible { "repair" } else { "pull" }
    );
    let tick = world.resource::<crate::step::StepRequest>().tick;
    let frame = FrameWorld::from_world(world);
    let duration = frame
        .script_model_anim(&clip)
        .filter(|anim| anim.frequency > 0.0)
        .map_or(0, |anim| {
            ticks((1000.0 / anim.frequency).clamp(0.0, 2500.0) as u32)
        });
    drop(frame);
    barrier.hiding[board] = (!visible).then_some(tick.0 + duration);
    if let Some(object) = barrier.objects.get(board).copied().flatten() {
        if let Some(entity) = world.resource_mut::<Runtime>().entities.get_mut(&object) {
            entity.hidden = !visible && duration == 0;
            entity.anim_op = Some(Some(clip.into()));
        }
    }
}

fn prepare_barriers(world: &mut World, state: &mut Survival, players: &[(ClientId, [f32; 3])]) {
    for barrier in &mut state.barriers {
        let tick = world.resource::<crate::step::StepRequest>().tick;
        for (board, due) in barrier.hiding.iter_mut().enumerate() {
            if due.is_some_and(|due| tick.0 >= due) {
                if let Some(object) = barrier.objects[board]
                    && let Some(entity) = world.resource_mut::<Runtime>().entities.get_mut(&object)
                {
                    entity.hidden = true;
                }
                *due = None;
            }
        }
        if !players.iter().any(|(_, at)| {
            Vec3::from_array(*at).distance_squared(Vec3::from_array(barrier.origin))
                < 1200.0 * 1200.0
        }) {
            continue;
        }
        for board in 0..barrier.models.len() {
            if world.resource::<Runtime>().entities.len() >= super::entities::MAX_SCRIPT_ENTITIES {
                return;
            }
            if barrier.objects[board].is_some() {
                continue;
            }
            let Ok(presence) = super::presence::spawn_presence(world, barrier.board_origins[board])
            else {
                continue;
            };
            let mut runtime = world.resource_mut::<Runtime>();
            let Ok(object) = runtime.create_entity(EntityKind::Spawned, "zombie_barrier_board")
            else {
                continue;
            };
            runtime.set_object_field(
                object,
                "origin",
                Value::Vector(barrier.board_origins[board]),
            );
            runtime.set_object_field(object, "angles", Value::Vector(barrier.angles));
            runtime.set_object_field(object, "model", Value::string(&barrier.models[board]));
            let entity = runtime.entities.get_mut(&object).unwrap();
            entity.presence = Some(presence);
            entity.solid = false;
            entity.contents = 0;
            entity.hidden = board >= barrier.boards as usize && barrier.hiding[board].is_none();
            entity.anim_op = Some(Some(
                format!(
                    "o_zombie_board_{}_{}",
                    board + 1,
                    if barrier.hiding[board].is_some() {
                        "pull"
                    } else {
                        "repair"
                    }
                )
                .into(),
            ));
            barrier.objects[board] = Some(object);
        }
    }
}

fn prepare_machines(world: &mut World, state: &mut Survival, players: &[(ClientId, [f32; 3])]) {
    let power: Vec<_> = state
        .purchases
        .iter()
        .map(|row| purchase_powered(state, row))
        .collect();
    for (index, row) in state.purchases.iter_mut().enumerate() {
        let Some((base, angles)) = &row.model else {
            continue;
        };
        if !players.iter().any(|(_, at)| {
            Vec3::from_array(*at).distance_squared(Vec3::from_array(row.origin)) < 1200.0 * 1200.0
        }) {
            continue;
        }
        let frame = FrameWorld::from_world(world);
        let powered = power[index];
        let on = format!("{base}_on");
        let model = if powered && frame.model_capability(&on).flatten().is_some() {
            on.as_str()
        } else {
            base.as_str()
        };
        if frame.model_capability(model).flatten().is_none() {
            continue;
        }
        drop(frame);
        if row.object.is_none() {
            if world.resource::<Runtime>().entities.len() >= super::entities::MAX_SCRIPT_ENTITIES {
                continue;
            }
            let Ok(presence) = super::presence::spawn_presence(world, row.origin) else {
                continue;
            };
            let mut runtime = world.resource_mut::<Runtime>();
            let Ok(object) = runtime.create_entity(EntityKind::Spawned, "zombie_perk_machine")
            else {
                continue;
            };
            runtime.set_object_field(object, "origin", Value::Vector(row.origin));
            runtime.set_object_field(object, "angles", Value::Vector(*angles));
            let entity = runtime.entities.get_mut(&object).unwrap();
            entity.presence = Some(presence);
            entity.solid = false;
            entity.contents = 0;
            row.object = Some(object);
            diag::info!(
                Sim,
                "zombies machine presented object={object} model={model} origin={:?}",
                row.origin
            );
        }
        world.resource_mut::<Runtime>().set_object_field(
            row.object.unwrap(),
            "model",
            Value::string(model),
        );
    }
}

fn relink_doors(world: &mut World, state: &mut Survival, tick: Tick) {
    let pending = std::mem::take(&mut state.pending_links);
    for (due, origin) in pending {
        if tick.0 < due {
            state.pending_links.push((due, origin));
            continue;
        }
        let frame = FrameWorld::from_world(world);
        let edges = Arc::make_mut(&mut state.edges);
        for (i, &at) in state
            .nodes
            .iter()
            .enumerate()
            .filter(|(_, at)| Vec3::from_array(**at).distance(Vec3::from_array(origin)) < 384.0)
        {
            for (j, &other) in state.nodes.iter().enumerate() {
                if i != j
                    && Vec3::from_array(at).distance(Vec3::from_array(other)) < 256.0
                    && (at[2] - other[2]).abs() < 48.0
                    && clear(&frame, at, other)
                    && !edges[i].contains(&j)
                {
                    edges[i].push(j);
                }
            }
        }
        for actor in state.actors.values_mut() {
            actor.repath = 0;
        }
    }
}

fn move_actors(
    world: &mut World,
    state: &mut Survival,
    tick: Tick,
    players: &[(ClientId, [f32; 3])],
) -> Vec<crate::script_player::Hit> {
    let mut hits = Vec::new();
    let actors = std::mem::take(&mut state.actors);
    let crowd: Vec<_> = actors
        .iter()
        .map(|(&id, actor)| (id, actor.origin))
        .collect();
    for (object, mut actor) in actors {
        let target = players
            .iter()
            .filter(|(client, _)| !state.powerups.blood(*client, tick))
            .min_by(|(_, a), (_, b)| {
                Vec3::from_array(*a)
                    .distance_squared(Vec3::from_array(actor.origin))
                    .total_cmp(
                        &Vec3::from_array(*b).distance_squared(Vec3::from_array(actor.origin)),
                    )
            })
            .or_else(|| actor.barrier.is_some().then(|| players.first()).flatten());
        let Some(&(victim, target)) = target else {
            actor.swing = None;
            actor.velocity = [0.0; 3];
            animate(world, object, &mut actor, 7, state.idle.as_deref());
            state.actors.insert(object, actor);
            continue;
        };
        let mut goal_target = target;
        let mut waiting = tick.0 < actor.emerge_until;
        if let Some(index) = actor.barrier {
            let barrier = &mut state.barriers[index];
            goal_target = barrier.outside;
            if let Some((start, from)) = actor.entering {
                let progress =
                    tick.0.saturating_sub(start) as f32 / state.entry_ticks.max(1) as f32;
                actor.origin =
                    window_origin(from, barrier.inside, progress, state.entry_curve.as_deref());
                waiting = true;
                if progress >= 1.0 {
                    actor.barrier = None;
                    actor.entering = None;
                    actor.repath = 0;
                    actor.velocity = [0.0; 3];
                    diag::info!(Sim, "zombie window entered object={object} barrier={index}");
                }
            } else if Vec3::from_array(actor.origin).distance(Vec3::from_array(barrier.outside))
                < 48.0
            {
                waiting = true;
                if barrier.boards > 0 {
                    animate(
                        world,
                        object,
                        &mut actor,
                        3,
                        state
                            .tear
                            .as_deref()
                            .or_else(|| state.attack.first().map(|anim| anim.name.as_str())),
                    );
                    if tick.0 >= barrier.tear_due {
                        barrier.boards -= 1;
                        barrier.tear_due = tick.0 + ticks(1200);
                        board_visibility(world, barrier, barrier.boards as usize, false);
                        actor.animation = 0;
                        diag::info!(
                            Sim,
                            "zombie barrier torn object={object} barrier={index} boards={}",
                            barrier.boards
                        );
                    }
                } else if tick.0 >= barrier.tear_due {
                    barrier.tear_due = tick.0 + state.entry_ticks + ticks(250);
                    animate(world, object, &mut actor, 4, state.entry.as_deref());
                    actor.entering = Some((tick.0, actor.origin));
                }
            }
        }
        let mut frame = FrameWorld::from_world(world);
        let delta = Vec3::from_array(target) - Vec3::from_array(actor.origin);
        let direct = clear(&frame, actor.origin, goal_target);
        if !waiting && actor.barrier.is_none() && actor.swing.is_some() {
            let swing = actor.swing.as_mut().unwrap();
            let locked = swing.victim;
            while swing
                .impacts
                .get(swing.next)
                .is_some_and(|&at| tick.0.saturating_sub(swing.start) >= at)
            {
                if let Some((_, at)) = players
                    .iter()
                    .find(|(id, _)| *id == locked && !state.powerups.blood(*id, tick))
                {
                    let delta = Vec3::from_array(*at) - Vec3::from_array(actor.origin);
                    if delta.length() <= 72.0 && clear(&frame, actor.origin, *at) {
                        hits.push(crate::script_player::Hit {
                            victim: locked,
                            attacker: None,
                            amount: 50,
                            flags: 0,
                            means: "MOD_MELEE",
                            weapon: 0,
                            point: *at,
                            dir: delta.normalize_or_zero().to_array(),
                            hitloc: 0,
                            inflictor: None,
                            commit: None,
                        });
                        diag::info!(
                            Sim,
                            "zombie melee impact object={object} client={}",
                            locked.0
                        );
                    }
                }
                swing.next += 1;
            }
            waiting = tick.0 < swing.finish;
            if !waiting {
                actor.swing = None;
            }
        }
        if !waiting
            && actor.barrier.is_none()
            && delta.length() < 60.0
            && direct
            && tick.0 >= actor.attack_due
            && (!state.attack.is_empty() || !state.run_attack.is_empty())
        {
            let attacks = if !state.run_attack.is_empty()
                && ((35.0 + state.round as f32 * 7.0).min(170.0) >= 100.0
                    || state.attack.is_empty())
            {
                &state.run_attack
            } else {
                &state.attack
            };
            let attack = choose_animation(&mut frame, attacks);
            let Some(attack) = attack else {
                state.actors.insert(object, actor);
                continue;
            };
            drop(frame);
            actor.animation = 0;
            animate(world, object, &mut actor, 2, Some(&attack.name));
            actor.swing = Some(Swing {
                victim,
                start: tick.0,
                next: 0,
                finish: tick.0 + attack.duration,
                impacts: attack.impacts,
            });
            actor.attack_due = tick.0 + attack.duration + ticks(100);
            waiting = true;
            frame = FrameWorld::from_world(world);
        }
        if !waiting {
            if tick.0 >= actor.repath {
                actor.path = reachable_node(&frame, &state.nodes, actor.origin)
                    .zip(reachable_node(&frame, &state.nodes, goal_target))
                    .map(|(start, goal)| route(state, start, goal))
                    .unwrap_or_default();
                actor.repath = tick.0 + ticks(800) + object as u32 % ticks(300);
            }
            while actor.path.front().is_some_and(|&node| {
                Vec3::from_array(state.nodes[node]).distance(Vec3::from_array(actor.origin)) < 24.0
            }) {
                actor.path.pop_front();
            }
            let goal = if direct {
                Some(goal_target)
            } else {
                actor.path.front().map(|&node| state.nodes[node])
            };
            if let Some(goal) = goal {
                let mut direction =
                    Vec3::new(goal[0] - actor.origin[0], goal[1] - actor.origin[1], 0.0)
                        .normalize_or_zero();
                let mut separation = Vec3::ZERO;
                for &(other, at) in &crowd {
                    if other == object || (at[2] - actor.origin[2]).abs() > 48.0 {
                        continue;
                    }
                    let mut away = Vec3::new(actor.origin[0] - at[0], actor.origin[1] - at[1], 0.0);
                    let distance = away.length();
                    if distance < 38.0 {
                        if distance < 0.01 {
                            away = if object < other { Vec3::X } else { -Vec3::X };
                        }
                        separation += away.normalize_or_zero() * (1.0 - distance / 38.0);
                    }
                }
                direction = (direction + separation * 1.4).normalize_or_zero();
                let before = actor.origin;
                let speed = (35.0 + state.round as f32 * 7.0).min(170.0);
                traverse(&frame, &mut actor, direction, speed);
                if Vec3::from_array(before).distance_squared(Vec3::from_array(actor.origin)) < 0.01
                {
                    actor.stalled += 1;
                } else {
                    actor.stalled = 0;
                }
                if actor.stalled >= ticks(500) {
                    actor.repath = 0;
                    actor.path.clear();
                    actor.stalled = 0;
                }
                drop(frame);
                let mode = if speed >= 100.0 { 5 } else { 1 };
                if actor.animation != mode {
                    let movement = if speed >= 100.0 {
                        actor.run.as_deref().or(actor.walk.as_deref())
                    } else {
                        actor.walk.as_deref()
                    }
                    .map(str::to_owned);
                    animate(world, object, &mut actor, mode, movement.as_deref());
                }
                frame = FrameWorld::from_world(world);
            }
        }
        let yaw = if actor.barrier.is_some() && waiting {
            let end = state.barriers[actor.barrier.unwrap()].inside;
            let from = actor.entering.map_or(actor.origin, |(_, from)| from);
            (end[1] - from[1]).atan2(end[0] - from[0]).to_degrees()
        } else {
            delta.y.atan2(delta.x).to_degrees()
        };
        let runtime = frame.ecs().resource_mut::<Runtime>().into_inner();
        runtime.set_object_field(object, "origin", Value::Vector(actor.origin));
        runtime.set_object_field(object, "angles", Value::Vector([0.0, yaw, 0.0]));
        state.actors.insert(object, actor);
    }
    hits
}

fn give_gun(
    world: &mut World,
    survivor: &mut Survivor,
    client: ClientId,
    weapon: u32,
    replace: Option<u32>,
) -> bool {
    let mut frame = FrameWorld::from_world(world);
    if frame.weapon_is_melee_only(weapon) || frame.equipment_facts_for(weapon).is_some() {
        return false;
    }
    if survivor.guns.contains(&weapon) {
        crate::script_player::give_max_ammo(&mut frame, client, weapon);
        return true;
    }
    if crate::script_player::give_weapon(&mut frame, client, weapon, false).is_err() {
        return false;
    }
    let remove = replace.or_else(|| {
        (survivor.guns.len()
            >= if survivor.perks.contains("mulekick") {
                3
            } else {
                2
            })
        .then(|| frame.player(client).unwrap().weapon)
    });
    if let Some(remove) = remove {
        crate::script_player::take_weapon(&mut frame, client, remove);
        survivor.guns.retain(|&gun| gun != remove);
    }
    survivor.guns.push(weapon);
    let _ = crate::script_player::set_spawn_weapon(&mut frame, client, weapon);
    true
}

fn purchase_powered(state: &Survival, row: &Purchase) -> bool {
    if state.origins.present() {
        state.origins.powered(row)
    } else {
        !row.needs_power || state.powered
    }
}

fn purchase(
    world: &mut World,
    state: &mut Survival,
    survivor: &mut Survivor,
    client: ClientId,
    index: usize,
    tick: Tick,
) {
    let row = state.purchases[index].clone();
    if !purchase_powered(state, &row) && !state.upgrades.contains_key(&index) {
        return;
    }
    if let PurchaseKind::Generator(number) = row.kind {
        state.origins.start(world, number, client);
        return;
    }
    if let PurchaseKind::Door(target) = &row.kind {
        if state.opened.contains(target) {
            return;
        }
        let cash = FrameWorld::from_world(world)
            .client_meta(client)
            .map_or(0, |meta| meta.score);
        if cash < row.price {
            return;
        }
        let mut runtime = world.resource_mut::<Runtime>();
        let ids: Vec<_> = runtime.entities.keys().copied().collect();
        let ids: Vec<_> = ids.into_iter().filter(|id| {
            runtime.entities.get(id).is_some_and(|entity| entity.presence.is_some())
                && matches!(runtime.object_field(*id, "targetname"), Value::String(name) if &*name == target.as_str())
        }).collect();
        if ids.is_empty() {
            return;
        }
        for id in ids {
            if let Some(entity) = runtime.entities.get_mut(&id) {
                entity.hidden = true;
                entity.solid = false;
                entity.contents = 0;
            }
        }
        drop(runtime);
        state.opened.insert(target.clone());
        score(world, client, -row.price);
        // Presence publishes the changed brush collision after this host step.
        state.pending_links.push((tick.0 + 2, row.origin));
        diag::info!(
            Sim,
            "zombies door opened target={target} client={} cost={}",
            client.0,
            row.price
        );
        return;
    }
    if matches!(row.kind, PurchaseKind::Power) {
        if !state.powered {
            state.powered = true;
            diag::info!(Sim, "zombies power enabled client={}", client.0);
        }
        return;
    }
    let frame = FrameWorld::from_world(world);
    let cash = frame.client_meta(client).map_or(0, |meta| meta.score);
    let held = frame.player(client).map_or(0, |ps| ps.weapon);
    let resolve = |name: &str| weapon_id(&frame, name);
    let solo = frame.client_ids_sorted().len() == 1;
    let mut price = if matches!(&row.kind, PurchaseKind::Perk(name) if name == "revive") && solo {
        500
    } else {
        row.price
    };
    let gun = match &row.kind {
        PurchaseKind::Door(_) | PurchaseKind::Power | PurchaseKind::Generator(_) => return,
        PurchaseKind::Weapon(name) => {
            let gun = resolve(name);
            if gun.is_some_and(|gun| survivor.guns.contains(&gun)) {
                price /= 2;
            }
            gun
        }
        PurchaseKind::Upgrade => {
            if let Some(claim) = state.upgrades.get(&index) {
                if claim.client != client || tick.0 < claim.ready {
                    return;
                }
                let gun = claim.weapon;
                drop(frame);
                if give_gun(world, survivor, client, gun, None) {
                    state.upgrades.remove(&index);
                    diag::info!(
                        Sim,
                        "zombies upgrade collected client={} weapon={gun}",
                        client.0
                    );
                }
                return;
            }
            let name = frame.weapon_script_name(held);
            if name.contains("_upgraded") || cash < row.price {
                return;
            }
            let Some(gun) = name
                .strip_suffix("_zm")
                .and_then(|base| resolve(&format!("{base}_upgraded_zm")))
            else {
                return;
            };
            drop(frame);
            let mut frame = FrameWorld::from_world(world);
            crate::script_player::take_weapon(&mut frame, client, held);
            drop(frame);
            survivor.guns.retain(|&gun| gun != held);
            if let Some(&remaining) = survivor.guns.first() {
                let _ = crate::script_player::set_spawn_weapon(
                    &mut FrameWorld::from_world(world),
                    client,
                    remaining,
                );
            }
            score(world, client, -row.price);
            state.upgrades.insert(
                index,
                BoxClaim {
                    client,
                    weapon: gun,
                    ready: tick.0 + ticks(5000),
                    expires: tick.0 + ticks(65000),
                },
            );
            diag::info!(
                Sim,
                "zombies upgrade started client={} original={held} upgraded={gun} cost={}",
                client.0,
                row.price
            );
            return;
        }
        PurchaseKind::Box => {
            if let Some(claim) = state.box_claims.get(&index) {
                if claim.client != client || tick.0 < claim.ready {
                    return;
                }
                let weapon = claim.weapon;
                drop(frame);
                if give_gun(world, survivor, client, weapon, None) {
                    state.box_claims.remove(&index);
                }
                return;
            }
            let pool: Vec<_> = frame
                .weapon_script_names()
                .iter()
                .filter(|name| name.ends_with("_zm") && !name.contains("upgraded"))
                .filter_map(|name| resolve(name))
                .filter(|&gun| {
                    !survivor.guns.contains(&gun)
                        && frame
                            .combat_facts_for(gun)
                            .is_some_and(|facts| facts.weap_type == weapon_iw4::WEAPTYPE_BULLET)
                        && frame.equipment_facts_for(gun).is_none()
                        && !frame.weapon_is_melee_only(gun)
                })
                .collect();
            if pool.is_empty() || cash < price {
                return;
            }
            let choice = ((u64::from(tick.0)
                .wrapping_mul(6364136223846793005u64)
                .wrapping_add(u64::from(client.0)))
                % pool.len() as u64) as usize;
            state.box_claims.insert(
                index,
                BoxClaim {
                    client,
                    weapon: pool[choice],
                    ready: tick.0 + ticks(3000),
                    expires: tick.0 + ticks(15000),
                },
            );
            drop(frame);
            score(world, client, -price);
            diag::info!(Sim, "zombies box spin client={} cost={price}", client.0);
            return;
        }
        PurchaseKind::Perk(name) => {
            if !matches!(
                name.as_str(),
                "juggernog"
                    | "sleight"
                    | "revive"
                    | "doubletap"
                    | "staminup"
                    | "deadshot"
                    | "mulekick"
                    | "phd"
            ) || survivor.perks.contains(name)
                || survivor.perks.len() >= 4
                || (name == "revive" && solo && survivor.solo_revives_bought >= 3)
                || cash < price
            {
                return;
            }
            drop(frame);
            let mut frame = FrameWorld::from_world(world);
            if name == "juggernog" {
                frame.client_meta_mut(client).max_health = 250;
                if let Some(ps) = frame.player_mut(client) {
                    ps.max_health = 250;
                    ps.health = 250;
                }
            } else if name == "sleight" {
                crate::script_player::set_perk(&mut frame, client, "specialty_fastreload", true);
            }
            if name == "staminup" {
                crate::script_player::set_perk(&mut frame, client, "specialty_marathon", true);
            }
            if name == "deadshot" {
                crate::script_player::set_perk(
                    &mut frame,
                    client,
                    "specialty_bulletaccuracy",
                    true,
                );
            }
            survivor.perks.insert(name.clone());
            if name == "revive" && solo {
                survivor.solo_revives_bought += 1;
            }
            drop(frame);
            score(world, client, -price);
            diag::info!(
                Sim,
                "zombies perk purchased client={} perk={name} cost={price}",
                client.0
            );
            return;
        }
    };
    let Some(gun) = gun else {
        return;
    };
    if cash < price {
        return;
    }
    let replace = matches!(row.kind, PurchaseKind::Upgrade).then_some(held);
    drop(frame);
    if give_gun(world, survivor, client, gun, replace) {
        score(world, client, -price);
        diag::info!(
            Sim,
            "zombies purchase client={} weapon={gun} cost={price}",
            client.0
        );
    }
}

fn make_hud(world: &mut World, client: ClientId, y: f32, scale: f32) -> Option<u64> {
    let Value::Object(object) =
        super::hud::new_hud_elem(world, HudAudience::Client(client.0)).ok()?
    else {
        return None;
    };
    for (name, value) in [
        ("x", Value::Float(24.0)),
        ("y", Value::Float(y)),
        ("fontscale", Value::Float(scale)),
        ("foreground", Value::Int(1)),
    ] {
        let _ = super::hud::store_field(world, object, name, &value);
    }
    Some(object)
}

fn hud_text(world: &mut World, object: u64, text: &str) {
    let Ok(index) = super::hud::string_index(world, &Value::string(text)) else {
        return;
    };
    let Some(&slot) = world.resource::<Runtime>().hud_slots.get(&object) else {
        return;
    };
    if let Some(slot) = FrameWorld::from_world(world)
        .hud_elem_slots_mut()
        .get_mut(slot)
    {
        slot.elem.elem_type = hud_iw4::HE_TYPE_TEXT;
        slot.elem.text = index;
    }
}

fn restore_survivor(world: &mut World, state: &mut Survival, victim: ClientId) {
    if let Some(survivor) = state.survivors.get_mut(&victim.0) {
        survivor.downed_since = None;
        survivor.perks.clear();
    }
    let mut frame = FrameWorld::from_world(world);
    crate::script_player::revive(&mut frame, victim);
    crate::script_player::clear_perks(&mut frame, victim);
    frame.client_meta_mut(victim).max_health = 100;
    if let Some(ps) = frame.player_mut(victim) {
        ps.health = 100;
        ps.max_health = 100;
    }
    state.revives.remove(&victim.0);
    diag::info!(Sim, "zombies survivor revived client={}", victim.0);
}

fn advance_revives(world: &mut World, state: &mut Survival, tick: Tick) {
    let downed: Vec<_> = state
        .survivors
        .iter()
        .filter_map(|(&client, survivor)| {
            Some((
                ClientId(client),
                survivor.downed_since?,
                survivor.perks.contains("revive"),
            ))
        })
        .collect();
    for (victim, since, quick) in downed {
        let mut frame = FrameWorld::from_world(world);
        if !frame
            .client_meta(victim)
            .is_some_and(|meta| meta.lifecycle == ClientLifecycle::Alive)
        {
            state.survivors.get_mut(&victim.0).unwrap().downed_since = None;
            state.revives.remove(&victim.0);
            continue;
        }
        let Some(origin) = frame.player(victim).map(|ps| ps.origin) else {
            continue;
        };
        let living: Vec<_> = frame
            .client_ids_sorted()
            .into_iter()
            .filter(|&client| client != victim)
            .filter(|client| {
                state
                    .survivors
                    .get(&client.0)
                    .is_some_and(|survivor| survivor.downed_since.is_none())
            })
            .filter(|&client| {
                frame
                    .client_meta(client)
                    .is_some_and(|meta| meta.lifecycle == ClientLifecycle::Alive)
            })
            .collect();
        if frame.client_ids_sorted().len() == 1
            && living.is_empty()
            && quick
            && tick.0.saturating_sub(since) >= ticks(7000)
        {
            drop(frame);
            restore_survivor(world, state, victim);
            continue;
        }
        let helper = living.into_iter().find(|&client| {
            let Some(ps) = frame.player(client) else {
                return false;
            };
            let at = ps.origin;
            let nearby =
                Vec3::from_array(at).distance_squared(Vec3::from_array(origin)) <= 80.0 * 80.0;
            let sight = frame
                .trace_world(
                    [at[0], at[1], at[2] + 40.0],
                    [origin[0], origin[1], origin[2] + 20.0],
                    [0.0; 3],
                    [0.0; 3],
                    0x11,
                )
                .fraction
                >= 1.0;
            nearby
                && sight
                && crate::script_player::buttons(&mut frame, client)
                    & (playerstate_iw4::buttons::USE | playerstate_iw4::buttons::USE_RELOAD)
                    != 0
        });
        if let Some(helper) = helper {
            let progress = state.revives.entry(victim.0).or_insert((helper, tick.0));
            if progress.0 != helper {
                *progress = (helper, tick.0);
            }
            let duration = if state
                .survivors
                .get(&helper.0)
                .is_some_and(|survivor| survivor.perks.contains("revive"))
            {
                ticks(1500)
            } else {
                ticks(3000)
            };
            if tick.0.saturating_sub(progress.1) >= duration {
                drop(frame);
                restore_survivor(world, state, victim);
                score(world, helper, state.powerups.reward(tick, 50));
                continue;
            }
        } else {
            state.revives.remove(&victim.0);
        }
        if tick.0.saturating_sub(since) >= ticks(45000) {
            crate::script_player::kill(&mut frame, tick, victim, None, None);
            frame.client_meta_mut(victim).deaths += 1;
            state.survivors.get_mut(&victim.0).unwrap().downed_since = None;
            if let Some(slot) = frame
                .ecs()
                .resource_mut::<Runtime>()
                .players
                .get_mut(&victim.0)
            {
                slot.sessionstate = "dead".into();
            }
            diag::info!(Sim, "zombies survivor bled out client={}", victim.0);
        }
    }
}

fn interactions(
    world: &mut World,
    state: &mut Survival,
    tick: Tick,
    players: &[(ClientId, [f32; 3])],
) {
    state.box_claims.retain(|_, claim| tick.0 < claim.expires);
    state.upgrades.retain(|_, claim| tick.0 < claim.expires);
    for &(client, origin) in players {
        let mut survivor = state.survivors.remove(&client.0).unwrap_or_default();
        let mut frame = FrameWorld::from_world(world);
        let held = crate::script_player::buttons(&mut frame, client)
            & (playerstate_iw4::buttons::USE | playerstate_iw4::buttons::USE_RELOAD)
            != 0;
        let selected = state
            .purchases
            .iter()
            .enumerate()
            .filter(|(_, row)| {
                !matches!(&row.kind, PurchaseKind::Door(target) if state.opened.contains(target))
                    && !(matches!(row.kind, PurchaseKind::Power) && state.powered)
                    && !matches!(row.kind, PurchaseKind::Generator(number) if !state.origins.available(number))
            })
            .filter(|(_, row)| {
                Vec3::from_array(row.origin).distance_squared(Vec3::from_array(origin))
                    <= 96.0 * 96.0
                    && frame
                        .trace_world(
                            [origin[0], origin[1], origin[2] + 50.0],
                            [
                                row.origin[0],
                                row.origin[1],
                                row.origin[2]
                                    + if matches!(
                                        row.kind,
                                        PurchaseKind::Door(_) | PurchaseKind::Power | PurchaseKind::Generator(_)
                                    ) {
                                        0.0
                                    } else {
                                        40.0
                                    },
                            ],
                            [0.0; 3],
                            [0.0; 3],
                            0x11,
                        )
                        .fraction
                        >= 0.95
            })
            .min_by(|(_, a), (_, b)| {
                Vec3::from_array(a.origin)
                    .distance_squared(Vec3::from_array(origin))
                    .total_cmp(
                        &Vec3::from_array(b.origin).distance_squared(Vec3::from_array(origin)),
                    )
            })
            .map(|(i, _)| i);
        let revival = state
            .survivors
            .iter()
            .filter(|(_, survivor)| survivor.downed_since.is_some())
            .find_map(|(&id, _)| {
                let at = frame.player(ClientId(id))?.origin;
                (Vec3::from_array(at).distance_squared(Vec3::from_array(origin)) <= 80.0 * 80.0
                    && frame
                        .trace_world(
                            [origin[0], origin[1], origin[2] + 40.0],
                            [at[0], at[1], at[2] + 20.0],
                            [0.0; 3],
                            [0.0; 3],
                            0x11,
                        )
                        .fraction
                        >= 1.0)
                    .then_some(id)
            });
        let repair = state
            .barriers
            .iter()
            .enumerate()
            .filter(|(_, barrier)| {
                barrier.boards < barrier.models.len() as u8
                    && Vec3::from_array(barrier.inside).distance_squared(Vec3::from_array(origin))
                        <= 96.0 * 96.0
                    && clear(&frame, origin, barrier.inside)
            })
            .min_by(|(_, a), (_, b)| {
                Vec3::from_array(a.inside)
                    .distance_squared(Vec3::from_array(origin))
                    .total_cmp(
                        &Vec3::from_array(b.inside).distance_squared(Vec3::from_array(origin)),
                    )
            })
            .map(|(index, _)| index);
        let solo = frame.client_ids_sorted().len() == 1;
        let cash = frame.client_meta(client).map_or(0, |meta| meta.score);
        drop(frame);
        let shovel = state.tools.selected(world, client, origin);
        let dig = state.digs.selected(world, origin, client, tick);
        let staff = state.staffs.selected(world, client, origin);
        let pedestal = state.staffs.selected_pedestal(world, client, origin);
        let tank_key = state.staffs.selected_tank_key(world, client, origin);
        if held && !survivor.use_held && revival.is_none() && selected.is_none() {
            if let Some(index) = shovel {
                state.tools.take(world, client, index);
            } else if let Some(index) = pedestal {
                state.staffs.place_staff(world, client, index, tick);
            } else if let Some(index) = tank_key {
                state.staffs.take_tank_key(world, client, index, tick);
            } else if let Some(index) = staff {
                state.staffs.craft(world, client, index);
            } else if let Some(index) = dig {
                let mut digs = std::mem::take(&mut state.digs);
                digs.dig(world, state, &mut survivor, client, index, tick);
                state.digs = digs;
            } else {
                // Check if player has a staff equipped and try to fire ability
                let frame = FrameWorld::from_world(world);
                let current_weapon = frame.player(client).map_or(0, |ps| ps.weapon);
                let weapon_name = frame.weapon_script_name(current_weapon).to_owned();
                drop(frame);

                // Check if current weapon is a staff
                for kind in [
                    origins_staff::StaffKind::Fire,
                    origins_staff::StaffKind::Ice,
                    origins_staff::StaffKind::Lightning,
                    origins_staff::StaffKind::Gas,
                ] {
                    if weapon_name.contains(kind.name()) && state.staffs.has(client, kind) {
                        // Get player's forward direction for ability direction
                        let frame = FrameWorld::from_world(world);
                        let forward = if let Some(ps) = frame.player(client) {
                            let angles = ps.viewangles;
                            let pitch = angles[0].to_radians();
                            let yaw = angles[1].to_radians();
                            [
                                pitch.cos() * yaw.cos(),
                                pitch.cos() * yaw.sin(),
                                -pitch.sin(),
                            ]
                        } else {
                            [1.0, 0.0, 0.0]
                        };
                        drop(frame);

                        state
                            .staffs
                            .fire_ability(world, client, kind, origin, forward, tick);
                        break;
                    }
                }
            }
        }
        if survivor.repair_round != state.round {
            survivor.repair_round = state.round;
            survivor.repair_points = 0;
        }
        if held && revival.is_none() && selected.is_none() {
            if let Some(index) = repair {
                if !survivor.use_held {
                    survivor.repair_due = tick.0 + ticks(1000);
                }
                if tick.0 >= survivor.repair_due {
                    let barrier = &mut state.barriers[index];
                    let board = barrier.boards as usize;
                    barrier.boards += 1;
                    board_visibility(world, barrier, board, true);
                    survivor.repair_due = tick.0 + ticks(1000);
                    let cap = (state.round.min(10) as i32 * 40).max(40);
                    if survivor.repair_points < cap {
                        score(world, client, state.powerups.reward(tick, 10));
                        survivor.repair_points += 10;
                    }
                    diag::info!(
                        Sim,
                        "zombie barrier repaired client={} barrier={index} boards={}",
                        client.0,
                        barrier.boards
                    );
                }
            }
        }
        if held
            && revival.is_none()
            && !survivor.use_held
            && let Some(index) = selected
        {
            purchase(world, state, &mut survivor, client, index, tick);
        }
        survivor.use_held = held;
        if state.origins.present() {
            if survivor.generator_label.is_none() {
                survivor.generator_label = make_hud(world, client, 290.0, 1.0);
            }
            let status = state.origins.status();
            if survivor.last_generators != status {
                if let Some(object) = survivor.generator_label {
                    hud_text(world, object, &status);
                }
                survivor.last_generators = status;
            }
        }
        if survivor.perk_label.is_none() {
            survivor.perk_label = make_hud(world, client, 320.0, 1.0);
        }
        let perks = format!(
            "PERKS: {}",
            survivor
                .perks
                .iter()
                .map(String::as_str)
                .collect::<Vec<_>>()
                .join(" ")
        );
        if survivor.last_perks != perks {
            if let Some(object) = survivor.perk_label {
                hud_text(world, object, &perks);
            }
            survivor.last_perks = perks;
        }
        if survivor.staff_label.is_none() {
            survivor.staff_label = make_hud(world, client, 340.0, 1.0);
        }
        let staffs = state.staffs.status(client);
        if survivor.last_staffs != staffs {
            if let Some(object) = survivor.staff_label {
                hud_text(world, object, &staffs);
            }
            survivor.last_staffs = staffs;
        }
        if survivor.hud.is_none() {
            survivor.hud = make_hud(world, client, 400.0, 1.8);
        }
        if survivor.prompt.is_none() {
            survivor.prompt = make_hud(world, client, 260.0, 1.0);
        }
        if survivor.round_label.is_none() {
            survivor.round_label = make_hud(world, client, 360.0, 1.4);
        }
        if let Some(object) = survivor.hud {
            let slot = world.resource::<Runtime>().hud_slots.get(&object).copied();
            let mut frame = FrameWorld::from_world(world);
            if let Some(slot) = slot.and_then(|slot| frame.hud_elem_slots_mut().get_mut(slot)) {
                slot.elem.elem_type = hud_iw4::HE_TYPE_VALUE;
                slot.elem.value = cash as f32;
            }
        }
        let prompt = selected
            .filter(|_| revival.is_none())
            .map(|index| {
                let row = &state.purchases[index];
                if !purchase_powered(state, &row) && !state.upgrades.contains_key(&index) {
                    return "USE: Requires power".to_owned();
                }
                match &row.kind {
                    PurchaseKind::Door(_) => {
                        format!("USE: Open door / clear debris [{} points]", row.price)
                    }
                    PurchaseKind::Power => "USE: Turn on power".to_owned(),
                    PurchaseKind::Generator(number) => state.origins.prompt(
                        *number,
                        FrameWorld::from_world(world).client_ids_sorted().len(),
                    ),
                    PurchaseKind::Weapon(name) => {
                        let frame = FrameWorld::from_world(world);
                        let owned =
                            weapon_id(&frame, name).is_some_and(|gun| survivor.guns.contains(&gun));
                        if owned {
                            format!("USE: {name} ammo  [{} points]", row.price / 2)
                        } else {
                            format!("USE: {name}  [{} points]", row.price)
                        }
                    }
                    PurchaseKind::Box => match state.box_claims.get(&index) {
                        Some(claim) if tick.0 < claim.ready => "Mystery Box spinning...".to_owned(),
                        Some(claim) if claim.client == client => {
                            "USE: take Mystery Box weapon".to_owned()
                        }
                        Some(_) => "Mystery Box in use".to_owned(),
                        None => format!("USE: Mystery Box  [{} points]", row.price),
                    },
                    PurchaseKind::Upgrade => match state.upgrades.get(&index) {
                        Some(claim) if tick.0 < claim.ready => {
                            "Pack-a-Punch upgrading...".to_owned()
                        }
                        Some(claim) if claim.client == client => {
                            "USE: Take upgraded weapon".to_owned()
                        }
                        Some(_) => "Pack-a-Punch in use".to_owned(),
                        None => "USE: Pack-a-Punch  [5000 points]".to_owned(),
                    },
                    PurchaseKind::Perk(name)
                        if matches!(
                            name.as_str(),
                            "juggernog"
                                | "sleight"
                                | "revive"
                                | "doubletap"
                                | "staminup"
                                | "deadshot"
                                | "mulekick"
                                | "phd"
                        ) =>
                    {
                        let label = match name.as_str() {
                            "juggernog" => "Jugger-Nog",
                            "sleight" => "Speed Cola",
                            "revive" => "Quick Revive",
                            "doubletap" => "Double Tap",
                            "staminup" => "Stamin-Up",
                            "deadshot" => "Deadshot",
                            "mulekick" => "Mule Kick",
                            _ => "PhD Flopper",
                        };
                        let price = if name == "revive" && solo {
                            500
                        } else {
                            row.price
                        };
                        format!("USE: {label}  [{price} points]")
                    }
                    PurchaseKind::Perk(_) => "Perk: Work in Progress".to_owned(),
                }
            })
            .unwrap_or_else(|| {
                let Some(victim) = revival else {
                    return if repair.is_some() {
                        "USE: Rebuild barrier (hold)".into()
                    } else {
                        if shovel.is_some() {
                            "USE: Pick up shovel".into()
                        } else if let Some(pedestal) = pedestal {
                            state.staffs.pedestal_prompt(pedestal, client)
                        } else if tank_key.is_some() {
                            state.staffs.tank_key_prompt().into()
                        } else if let Some(staff) = staff {
                            state.staffs.prompt(staff, client)
                        } else if let Some(dig) = dig {
                            state.digs.prompt(dig, state.tools.owned(client)).into()
                        } else {
                            String::new()
                        }
                    };
                };
                let duration = if survivor.perks.contains("revive") {
                    ticks(1500)
                } else {
                    ticks(3000)
                };
                match state
                    .revives
                    .get(&victim)
                    .filter(|(helper, _)| *helper == client)
                {
                    Some((_, start)) => format!(
                        "USE: reviving teammate {}%",
                        tick.0
                            .saturating_sub(*start)
                            .saturating_mul(100)
                            .checked_div(duration)
                            .unwrap_or(0)
                            .min(100)
                    ),
                    None => "USE: revive teammate (hold)".into(),
                }
            });
        if survivor.last_prompt != prompt {
            if let Some(object) = survivor.prompt {
                hud_text(world, object, &prompt);
            }
            survivor.last_prompt = prompt;
        }
        if survivor.last_round != state.round {
            if let Some(object) = survivor.round_label {
                hud_text(world, object, &format!("ROUND {}", state.round));
            }
            survivor.last_round = state.round;
        }
        state.survivors.insert(client.0, survivor);
    }
}

pub(crate) fn advance(world: &mut World) {
    let tick = world.resource::<crate::step::StepRequest>().tick;
    let mut state = std::mem::take(&mut world.resource_mut::<Runtime>().zombies);
    if !state.initialized {
        initialize(world, &mut state);
    }
    let connected: BTreeSet<_> = FrameWorld::from_world(world)
        .client_ids_sorted()
        .into_iter()
        .map(|id| id.0)
        .collect();
    let departed: Vec<_> = state
        .survivors
        .keys()
        .copied()
        .filter(|id| !connected.contains(id))
        .collect();
    for id in departed {
        if let Some(survivor) = state.survivors.remove(&id) {
            for object in [
                survivor.hud,
                survivor.prompt,
                survivor.round_label,
                survivor.perk_label,
                survivor.generator_label,
                survivor.powerup_label,
            ]
            .into_iter()
            .flatten()
            {
                super::hud::destroy(world, object);
            }
        }
        state.revives.remove(&id);
        state.revives.retain(|_, (helper, _)| helper.0 != id);
        state.box_claims.retain(|_, claim| claim.client.0 != id);
        state.upgrades.retain(|_, claim| claim.client.0 != id);
    }
    if let Some(ended) = state.ended {
        world.resource_mut::<Runtime>().zombies = state;
        if tick.0.saturating_sub(ended) >= ticks(5000) {
            world.resource_mut::<Runtime>().pending_restart = Some(false);
            super::restart::restart_level(world, tick);
        }
        return;
    }
    let mut hits = Vec::new();
    let mut frame = FrameWorld::from_world(world);
    if let Some(started) = state.started {
        frame.set_match_elapsed_ms(
            tick.0
                .saturating_sub(started)
                .saturating_mul(crate::MATCH_TICK_MS),
        );
    }
    if frame.phase() == MatchPhase::Warmup {
        frame.set_phase(MatchPhase::Playing);
    }
    let spawn: Vec<_> = frame
        .client_ids_sorted()
        .into_iter()
        .filter(|&id| {
            frame.client_meta(id).is_some_and(|meta| {
                matches!(
                    meta.lifecycle,
                    ClientLifecycle::Connecting | ClientLifecycle::ChoosingClass
                ) || (meta.lifecycle == ClientLifecycle::Spectating
                    && !state.survivors.contains_key(&id.0))
            })
        })
        .collect();
    drop(frame);
    for client in spawn {
        spawn_player(world, &mut state, client, tick);
    }
    relink_doors(world, &mut state, tick);
    advance_revives(world, &mut state, tick);
    if state.next_round.is_some_and(|due| tick.0 >= due) && state.round > 0 {
        let frame = FrameWorld::from_world(world);
        let returning: Vec<_> = frame
            .client_ids_sorted()
            .into_iter()
            .filter(|&id| {
                frame
                    .client_meta(id)
                    .is_some_and(|meta| meta.lifecycle == ClientLifecycle::Dead)
            })
            .collect();
        drop(frame);
        for client in returning {
            spawn_player(world, &mut state, client, tick);
        }
    }
    let frame = FrameWorld::from_world(world);
    let players: Vec<_> = frame
        .client_ids_sorted()
        .into_iter()
        .filter(|&id| {
            frame
                .client_meta(id)
                .is_some_and(|meta| meta.lifecycle == ClientLifecycle::Alive)
        })
        .filter(|id| {
            state
                .survivors
                .get(&id.0)
                .is_none_or(|survivor| survivor.downed_since.is_none())
        })
        .filter_map(|id| frame.player(id).map(|ps| (id, ps.origin)))
        .collect();
    drop(frame);
    if players.is_empty() {
        let auto_revive_pending = connected.len() == 1
            && state.survivors.values().any(|survivor| {
                survivor.downed_since.is_some() && survivor.perks.contains("revive")
            });
        if !state.survivors.is_empty() && !auto_revive_pending {
            state.ended = Some(tick.0);
            let mut frame = FrameWorld::from_world(world);
            frame.set_phase(MatchPhase::Intermission);
            for client in frame.client_ids_sorted() {
                frame.client_meta_mut(client).lifecycle = ClientLifecycle::Intermission;
            }
            diag::info!(Sim, "zombies game over round={}", state.round);
        }
    } else {
        if state.next_round.is_some_and(|due| tick.0 >= due) {
            state.started.get_or_insert(tick.0);
            state.round = (state.round + 1).min(255);
            state.powerups.new_round();
            mines::new_round(world);
            state.remaining = rounds::population(
                state.round,
                FrameWorld::from_world(world).client_ids_sorted().len(),
            );
            state.next_round = None;
            state.next_spawn = tick.0 + ticks(2000);
            diag::info!(
                Sim,
                "zombies round={} total={}",
                state.round,
                state.remaining
            );
        }
        if state.remaining > 0 && state.actors.len() < MAX_ALIVE && tick.0 >= state.next_spawn {
            if spawn_actor(world, &mut state, tick, &players) {
                state.remaining -= 1;
            }
            state.next_spawn = tick.0 + ticks(rounds::spawn_delay_ms(state.round));
        }
        hits = move_actors(world, &mut state, tick, &players);
        mines::advance(world, &state, tick);
        prepare_barriers(world, &mut state, &players);
        state.origins.advance(world, &players);
        if state.origins.present() {
            state.powered = state.origins.all_active();
        }
        prepare_machines(world, &mut state, &players);
        state.tools.advance(world, &players, &state.digs);
        state.staffs.advance(world, &players);
        state.staffs.advance_pedestals(world, &players);
        state.weather.advance(world, tick, &players);
        let mut digs = std::mem::take(&mut state.digs);
        digs.advance(world, &mut state, tick, &players);
        state.digs = digs;
        interactions(world, &mut state, tick, &players);
        powerups::advance(world, &mut state, tick);
        if state.remaining == 0 && state.actors.is_empty() && state.next_round.is_none() {
            state.weather.end_round(world, state.round, tick);
            state.next_round = Some(tick.0 + ticks(10000));
        }
    }
    world.resource_mut::<Runtime>().zombies = state;
    for hit in hits {
        player_damage(world, tick, &hit);
    }
}

pub(crate) fn entity_damage_amount(
    world: &mut World,
    object: u64,
    hit: &super::entity_damage::EntityHit,
) -> i32 {
    let runtime = world.resource::<Runtime>();
    if hit.amount > 0
        && hit.attacker.is_some()
        && runtime.zombies.actors.contains_key(&object)
        && runtime
            .zombies
            .powerups
            .instant(world.resource::<crate::step::StepRequest>().tick)
    {
        return match world
            .resource_mut::<Runtime>()
            .object_field(object, "health")
        {
            Value::Int(health) => health.max(hit.amount),
            _ => hit.amount,
        };
    }
    let boosted = world
        .resource::<Runtime>()
        .zombies
        .actors
        .contains_key(&object)
        && hit.attacker.is_some_and(|client| {
            world
                .resource::<Runtime>()
                .zombies
                .survivors
                .get(&client.0)
                .is_some_and(|survivor| survivor.perks.contains("doubletap"))
        })
        && matches!(hit.means, "MOD_RIFLE_BULLET" | "MOD_PISTOL_BULLET");
    if hit.amount > 0
        && hit.means == "MOD_MELEE"
        && world
            .resource::<Runtime>()
            .zombies
            .actors
            .contains_key(&object)
    {
        hit.amount.max(150)
    } else if boosted {
        hit.amount.saturating_mul(2)
    } else {
        hit.amount
    }
}

pub(crate) fn entity_damage(
    world: &mut World,
    object: u64,
    hit: &super::entity_damage::EntityHit,
    before: i32,
    after: i32,
    headshot: bool,
) {
    // Check if this is the giant robot
    let tick = world.resource::<crate::step::StepRequest>().tick;
    let is_robot = world
        .resource::<Runtime>()
        .zombies
        .staffs
        .is_robot(object);
    
    if is_robot && hit.amount > 0 {
        let mut zombies = std::mem::take(&mut world.resource_mut::<Runtime>().zombies);
        zombies.staffs.damage_robot(world, hit.amount, tick);
        world.resource_mut::<Runtime>().zombies = zombies;
        
        if let Some(client) = hit.attacker {
            let reward = world
                .resource_mut::<Runtime>()
                .zombies
                .powerups
                .reward(tick, 50);
            score(world, client, reward);
        }
        diag::info!(
            Sim,
            "origins robot hit object={object} damage={} before={} after={}",
            hit.amount,
            before,
            after
        );
        return;
    }
    
    if !world
        .resource::<Runtime>()
        .zombies
        .actors
        .contains_key(&object)
        || before <= 0
        || hit.amount <= 0
    {
        return;
    }
    let killed = after <= 0;
    let origin = if killed {
        let mut runtime = world.resource_mut::<Runtime>();
        let origin = runtime
            .zombies
            .actors
            .remove(&object)
            .map(|actor| actor.origin);
        runtime.pending_deletes.push(object);
        origin
    } else {
        None
    };
    if let Some(client) = hit.attacker {
        if killed {
            let mut frame = FrameWorld::from_world(world);
            if frame.client_meta(client).is_some() {
                frame.client_meta_mut(client).kills += 1;
            }
        }
        let reward = if !killed {
            10
        } else if hit.means == "MOD_MELEE" {
            130
        } else if headshot {
            100
        } else {
            60
        };
        let tick = world.resource::<crate::step::StepRequest>().tick;
        let reward = world
            .resource_mut::<Runtime>()
            .zombies
            .powerups
            .reward(tick, reward);
        score(world, client, reward);
        diag::info!(
            Sim,
            "zombies hit object={object} before={before} after={after} points={reward} client={}",
            client.0
        );
        if let Some(origin) = origin {
            let mut runtime = world.resource_mut::<Runtime>();
            let destroyed = runtime
                .zombies
                .barriers
                .iter()
                .filter(|barrier| barrier.boards == 0 && !barrier.models.is_empty())
                .count();
            let mut powers = std::mem::take(&mut runtime.zombies.powerups);
            drop(runtime);
            powers.killed(world, origin, destroyed);
            world.resource_mut::<Runtime>().zombies.powerups = powers;
        }
    }
}

pub(crate) fn player_damage(world: &mut World, tick: Tick, hit: &crate::script_player::Hit) {
    if world
        .resource::<Runtime>()
        .zombies
        .survivors
        .get(&hit.victim.0)
        .is_some_and(|survivor| survivor.perks.contains("phd"))
        && matches!(
            hit.means,
            "MOD_GRENADE"
                | "MOD_GRENADE_SPLASH"
                | "MOD_EXPLOSIVE"
                | "MOD_PROJECTILE"
                | "MOD_PROJECTILE_SPLASH"
                | "MOD_FALLING"
        )
    {
        return;
    }
    if hit.attacker.is_some_and(|client| client != hit.victim) {
        return;
    }
    let mut frame = FrameWorld::from_world(world);
    if frame.phase() != MatchPhase::Playing
        || !frame
            .client_meta(hit.victim)
            .is_some_and(|meta| meta.lifecycle == ClientLifecycle::Alive)
    {
        return;
    }
    if frame
        .player(hit.victim)
        .is_some_and(|ps| ps.pm_flags & playerstate_iw4::pm_flags::LAST_STAND != 0)
    {
        return;
    }
    if crate::script_player::finish_damage(&mut frame, hit.victim, hit.amount, Some(hit.dir))
        == crate::script_player::Finish::Killed
    {
        let teammate_alive = frame.client_ids_sorted().into_iter().any(|client| {
            client != hit.victim
                && frame
                    .client_meta(client)
                    .is_some_and(|meta| meta.lifecycle == ClientLifecycle::Alive)
                && frame
                    .player(client)
                    .is_some_and(|ps| ps.pm_flags & playerstate_iw4::pm_flags::LAST_STAND == 0)
        });
        let quick = frame.client_ids_sorted().len() == 1
            && frame
                .ecs()
                .resource::<Runtime>()
                .zombies
                .survivors
                .get(&hit.victim.0)
                .is_some_and(|survivor| survivor.perks.contains("revive"));
        if teammate_alive || quick {
            crate::script_player::clear_perks(&mut frame, hit.victim);
            frame.client_meta_mut(hit.victim).max_health = 100;
            if let Some(ps) = frame.player_mut(hit.victim) {
                ps.health = 1;
                ps.max_health = 100;
                ps.pm_type = playerstate_iw4::PM_TYPE_LAST_STAND;
                ps.pm_flags |= playerstate_iw4::pm_flags::LAST_STAND;
                ps.view_height_target = movement_iw4::view_height::LAST_STAND;
            }
            let third = frame
                .ecs()
                .resource::<Runtime>()
                .zombies
                .survivors
                .get(&hit.victim.0)
                .and_then(|survivor| (survivor.guns.len() > 2).then(|| survivor.guns[2]));
            if let Some(gun) = third {
                let replacement = frame
                    .ecs()
                    .resource::<Runtime>()
                    .zombies
                    .survivors
                    .get(&hit.victim.0)
                    .and_then(|survivor| survivor.guns.first().copied());
                crate::script_player::take_weapon(&mut frame, hit.victim, gun);
                if frame.player(hit.victim).is_some_and(|ps| ps.weapon == 0)
                    && let Some(weapon) = replacement
                {
                    let _ = crate::script_player::set_spawn_weapon(&mut frame, hit.victim, weapon);
                }
            }
            if let Some(survivor) = frame
                .ecs()
                .resource_mut::<Runtime>()
                .zombies
                .survivors
                .get_mut(&hit.victim.0)
            {
                survivor.guns.truncate(2);
                survivor.downed_since = Some(tick.0);
                survivor.perks.retain(|name| name == "revive");
            }
            diag::info!(Sim, "zombies survivor downed client={}", hit.victim.0);
            return;
        }
        crate::script_player::kill(&mut frame, tick, hit.victim, hit.attacker, hit.commit);
        frame.client_meta_mut(hit.victim).deaths += 1;
        if let Some(slot) = frame
            .ecs()
            .resource_mut::<Runtime>()
            .players
            .get_mut(&hit.victim.0)
        {
            slot.sessionstate = "dead".into();
        }
        diag::info!(Sim, "zombies survivor died client={}", hit.victim.0);
    }
}
