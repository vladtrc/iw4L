use bevy::ecs::system::SystemParam;
use bevy::prelude::*;
use bevy::tasks::{Task, futures_lite::future};
use frame::ScopeApp;
use net::{
    AUTHORITY_MS, AuthorityClock, AuthorityWorld, ClientActionInbox, ClientCommandInbox,
    LocalPresentClient, ReliableEventHub, authority_should_tick, look_angles_from_degrees,
};
use sim::{ClientAction, ClientLifecycle, SimWorld, Tick};

use crate::nav::{self, NAV_HULL, NAV_SCHEMA, NavGraph};
use crate::query::{Budgeted, TraceBudget};
use crate::roster::{
    BotAddQueue, BotFireQueue, BotHold, BotRoster, BotTpQueue, BotTpTarget, BotTpWhere,
    default_class_index,
};
use crate::sensor;
use frame::{AuthoritySet, BotNavigationReady, ClientSet, RuntimeRole};

const VIEW_PITCH_DOWN: f32 = 85.0;
const TRACE_QUOTA: u32 = 96;
const ASTAR_QUOTA: u32 = 2048;
#[derive(Resource, Default)]
struct BotNav {
    graph: NavGraph,
    pending: Option<(u64, Task<NavGraph>, Option<asset_transport::StageHandle>)>,
}

pub struct BotsPlugin;

impl Plugin for BotsPlugin {
    fn build(&self, app: &mut App) {
        app.scoped::<BotRoster>(frame::MatchScope::Live)
            .init_resource::<BotAddQueue>()
            .init_resource::<BotHold>()
            .init_resource::<BotTpQueue>()
            .init_resource::<BotFireQueue>()
            .scoped::<BotNav>(frame::MatchScope::Live)
            .scoped::<BotNavigationReady>(frame::MatchScope::Live)
            .add_systems(
                PreUpdate,
                (
                    fill_bots_from_rules.in_set(frame::InMatch),
                    drain_bot_add_queue.in_set(frame::InMatch),
                    evict_bots_claiming_local_client.in_set(frame::InMatch),
                    boot_bots.in_set(frame::InMatch),
                    apply_bot_tp.in_set(frame::InMatch),
                )
                    .chain()
                    .after(ClientSet::Load)
                    .before(ClientSet::Receive),
            )
            .add_systems(
                PreUpdate,
                (prepare_navigation.in_set(frame::InMatch))
                    .chain()
                    .in_set(ClientSet::Load),
            )
            .add_systems(
                FixedUpdate,
                (
                    evict_bots_claiming_local_client.in_set(frame::InMatch),
                    think_bots.in_set(frame::InMatch),
                )
                    .chain()
                    .in_set(AuthoritySet::Ingress)
                    .run_if(authority_should_tick),
            );
    }
}

fn fill_bots_from_rules(
    mut roster: ResMut<BotRoster>,
    mut queue: ResMut<BotAddQueue>,
    installed: Option<Res<State<frame::MatchScope>>>,
    rules: Option<Res<frame::HostMatchRules>>,
    world: Option<Res<AuthorityWorld>>,
) {
    if roster.rules_filled
        || !installed.is_some_and(|installed| *installed.get() == frame::MatchScope::Live)
    {
        return;
    }
    let Some(world) = world else {
        return;
    };
    roster.rules_filled = true;
    let Some(rules) = rules else {
        return;
    };
    let count = |dvar: &str| {
        rules
            .0
            .iter()
            .find(|(name, _)| name == dvar)
            .and_then(|(_, value)| value.trim().parse::<u32>().ok())
            .unwrap_or(0)
    };
    let enemies = count(sim::ENEMY_BOTS_DVAR);
    let friends = if world.0.game_mode_kind() == gamemode_iw4::GameModeKind::FreeForAll {
        0
    } else {
        count(sim::FRIENDLY_BOTS_DVAR)
    };
    queue.push_side(enemies, crate::BotSide::Enemy);
    queue.push_side(friends, crate::BotSide::Friendly);
    if enemies > 0 || friends > 0 {
        diag::info!(
            Sim,
            "bots: game rules add {enemies} enemy and {friends} friendly"
        );
    }
}

fn drain_bot_add_queue(
    mut queue: ResMut<BotAddQueue>,
    mut roster: ResMut<BotRoster>,
    local: Res<LocalPresentClient>,
    world: Option<Res<AuthorityWorld>>,
    hub: Option<Res<net::UdpAuthorityHub>>,
) {
    let requests = queue.drain();
    if requests.is_empty() {
        return;
    }
    let mut taken = vec![local.0];
    if let Some(world) = world.as_ref() {
        taken.extend(world.0.clients_scoreboard().into_iter().map(|(id, _)| id));
    }
    if let Some(hub) = hub.as_ref() {
        taken.extend(hub.connections.clients().map(sim::ClientId));
    }
    for request in requests {
        let count = request.count;
        let added = roster.add_bots(count, &taken, request.dummy, request.side);
        if added.len() < count as usize {
            diag::warn!(
                Sim,
                "bots: add {count} — only {} minted, the roster is full",
                added.len()
            );
        }
        diag::info!(
            Sim,
            "bots: add {count} (dummy={}) → clients {:?}",
            request.dummy,
            added.iter().map(|id| id.0).collect::<Vec<_>>()
        );
    }
}

// The link hands the local player its real id during signon, which can land
// after a bot was already minted. A slot holding that id would make `is_bot`
// claim the player, so retire it — loudly, because by then it is a bug.
fn evict_bots_claiming_local_client(mut roster: ResMut<BotRoster>, local: Res<LocalPresentClient>) {
    if !roster.is_bot(local.0) {
        return;
    }
    roster.bots.retain(|bot| bot.id != local.0);
    diag::warn!(
        Sim,
        "bots: retired the slot on client {} — that id is the local player",
        local.0.0
    );
}

fn boot_bots(
    mut roster: ResMut<BotRoster>,
    mut actions: ResMut<ClientActionInbox>,
    installed: Option<Res<State<frame::MatchScope>>>,
    world: Option<ResMut<AuthorityWorld>>,
    local: Res<LocalPresentClient>,
) {
    let Some(mut world) = world else {
        return;
    };
    if !installed.is_some_and(|installed| *installed.get() == frame::MatchScope::Live) {
        return;
    }
    let seed = roster.seed;
    let team_based = world.0.game_mode_kind() != gamemode_iw4::GameModeKind::FreeForAll;
    let local_team = match world.0.client_state_team(local.0) {
        Some(entity_iw4::TEAM_AXIS) => Some(("axis", "allies")),
        Some(entity_iw4::TEAM_ALLIES) => Some(("allies", "axis")),
        _ => None,
    };
    for bot in &mut roster.bots {
        if bot.joined {
            continue;
        }
        let team = match bot.side.filter(|_| team_based) {
            None => None,
            Some(side) => {
                let Some((mine, theirs)) = local_team else {
                    continue;
                };
                Some(match side {
                    crate::BotSide::Friendly => mine,
                    crate::BotSide::Enemy => theirs,
                })
            }
        };
        if world.0.gsc_realm() == Some(sim::script::FamilyId::IW4)
            && let Err(error) = world
                .0
                .persistent_data_mut()
                .admit_temporary(bot.id, bot.account)
        {
            diag::warn!(
                Sim,
                "bots: client {} account admission failed: {error:?}",
                bot.id.0
            );
            continue;
        }
        let index = default_class_index(seed, bot.id, bot.class_picks);
        let queued = [
            actions.push(bot.id, ClientAction::JoinMatch),
            team.map_or(Ok(()), |team| {
                match (
                    sim::menu_response_field(sim::TEAM_MENU),
                    sim::menu_response_field(team),
                ) {
                    (Some(menu), Some(response)) => {
                        actions.push(bot.id, ClientAction::MenuResponse { menu, response })
                    }
                    _ => Ok(()),
                }
            }),
            actions.push(bot.id, ClientAction::ChooseDefaultClass { index }),
            actions.push(
                bot.id,
                ClientAction::SetName {
                    name: entity_iw4::pack_client_state_name(bot.name),
                },
            ),
        ];
        if let Some(error) = queued.into_iter().find_map(Result::err) {
            diag::warn!(Sim, "bots: client {} not booted — {error}", bot.id.0);
            continue;
        }
        bot.joined = true;
        bot.class_picks += 1;
        diag::info!(
            Sim,
            "bots: JoinMatch+changeclass client={} pick={index} requested_side={:?} requested_team={team:?} mode={:?}",
            bot.id.0,
            bot.side,
            world.0.game_mode_kind(),
        );
    }
}

fn apply_bot_tp(
    mut queue: ResMut<BotTpQueue>,
    roster: Res<BotRoster>,
    mut actions: ResMut<ClientActionInbox>,
    world: Option<Res<AuthorityWorld>>,
    local: Res<LocalPresentClient>,
) {
    let requests = queue.drain();
    if requests.is_empty() {
        return;
    }
    let Some(world) = world else {
        return;
    };
    let snapshot = world.0.snapshot(Tick(0));
    let local_ps = snapshot
        .meta
        .for_client(local.0)
        .is_some_and(|m| m.lifecycle == ClientLifecycle::Alive)
        .then(|| {
            snapshot
                .players
                .iter()
                .find(|(id, _)| *id == local.0)
                .map(|(_, ps)| ps)
        })
        .flatten();
    for request in requests {
        let ids: Vec<sim::ClientId> = match request.target {
            BotTpTarget::All => roster.bots.iter().map(|b| b.id).collect(),
            BotTpTarget::Id(id) => {
                if roster.is_bot(id) {
                    vec![id]
                } else {
                    diag::warn!(Sim, "bots: tp skipped — client {} is not a bot", id.0);
                    Vec::new()
                }
            }
        };
        for id in ids {
            let current = snapshot
                .players
                .iter()
                .find(|(c, _)| *c == id)
                .map(|(_, ps)| ps.viewangles)
                .unwrap_or([0.0, 0.0, 0.0]);
            let (origin, angles) = match request.where_ {
                BotTpWhere::Absolute { origin, yaw, pitch } => {
                    let mut angles = current;
                    if let Some(yaw) = yaw {
                        angles[1] = yaw;
                    }
                    if let Some(pitch) = pitch {
                        angles[0] = pitch;
                    }
                    (origin, angles)
                }
                BotTpWhere::Above { height } => {
                    let Some(ps) = local_ps else {
                        diag::warn!(Sim, "bots: tp above skipped — local not Alive");
                        continue;
                    };
                    let origin = [ps.origin[0], ps.origin[1], ps.origin[2] + height];
                    (origin, aim_viewangles(origin, ps.origin))
                }
            };
            if let Err(error) = actions.push(id, ClientAction::Move { origin, angles }) {
                diag::warn!(Sim, "bots: tp client={} not queued — {error}", id.0);
                continue;
            }
            diag::info!(
                Sim,
                "bots: tp client={} ({:.1} {:.1} {:.1})",
                id.0,
                origin[0],
                origin[1],
                origin[2]
            );
        }
    }
}

#[derive(SystemParam)]
struct ThinkBots<'w> {
    clock: Res<'w, AuthorityClock>,
    world: ResMut<'w, AuthorityWorld>,
    nav: Res<'w, BotNav>,
    roster: ResMut<'w, BotRoster>,
    cmds: ResMut<'w, ClientCommandInbox>,
    actions: ResMut<'w, ClientActionInbox>,
    hold: Res<'w, BotHold>,
    fire: ResMut<'w, BotFireQueue>,
    reliable: ResMut<'w, ReliableEventHub>,
}

fn think_bots(mut p: ThinkBots) {
    // A bot has no client reading its reliable channel to ack it, so nothing
    // else ever drains it — leave this out and match broadcasts (deaths, hit
    // markers, ...) overflow the queue and the bot gets retired as if its
    // connection had died.
    for bot in &p.roster.bots {
        p.reliable.ack_all(bot.id);
    }
    if p.roster.bots.is_empty() {
        p.fire.drain();
        return;
    }

    let snapshot = p.world.0.snapshot(Tick(p.clock.tick.saturating_sub(1)));
    let fires = p.fire.drain();
    let mut budget = TraceBudget::new(TRACE_QUOTA);
    let mut astar = ASTAR_QUOTA;
    let count = p.roster.bots.len();
    let start = (p.clock.tick as usize / 2) % count.max(1);
    let seed = p.roster.seed;
    for offset in 0..count {
        let bot = &mut p.roster.bots[(start + offset) % count];
        if p.world.0.input_program_owns(bot.id) {
            continue;
        }
        let Some(meta) = snapshot.meta.for_client(bot.id) else {
            continue;
        };
        if meta.lifecycle != ClientLifecycle::Alive {
            if let Some(brain) = bot.brain.as_mut() {
                brain.cancel_navigation();
            }
            continue;
        }
        if bot.class_picked_in != Some(meta.life_sequence) {
            bot.class_picked_in = Some(meta.life_sequence);
            let index = default_class_index(seed, bot.id, bot.class_picks);
            match p
                .actions
                .push(bot.id, ClientAction::ChooseDefaultClass { index })
            {
                Ok(()) => bot.class_picks += 1,
                Err(error) => diag::warn!(
                    Sim,
                    "bots: client {} class pick not queued — {error}",
                    bot.id.0
                ),
            }
        }
        let mut cmd = if p.hold.0 || bot.brain.is_none() {
            let Some((_, ps)) = snapshot.players.iter().find(|(id, _)| *id == bot.id) else {
                continue;
            };
            playerstate_iw4::UserCmd {
                server_time: p.clock.time_ms,
                angles: look_angles_from_degrees(std::array::from_fn(|axis| {
                    ps.viewangles[axis] - ps.delta_angles[axis]
                })),
                weapon: ps.weapon as u16,
                weapon_mapped: ps.weapon as u16,
                ..playerstate_iw4::UserCmd::default()
            }
        } else if let Some(brain) = bot.brain.as_mut() {
            let mut queried = Budgeted::new(&mut p.world.0, &mut budget);
            let Some(obs) =
                sensor::observe_focused(&snapshot, bot.id, &mut queried, brain.focus_target())
            else {
                continue;
            };
            let mut cmd = brain.drive_nav(
                &obs,
                &mut queried,
                Some(&p.nav.graph),
                &mut astar,
                AUTHORITY_MS,
            );
            cmd.server_time = p.clock.time_ms;
            cmd
        } else {
            continue;
        };
        if fires.iter().any(|target| match target {
            BotTpTarget::All => true,
            BotTpTarget::Id(id) => *id == bot.id,
        }) {
            cmd.buttons |= playerstate_iw4::buttons::ATTACK;
        }
        p.cmds.push(bot.id, None, cmd, None);
    }
}

fn prepare_navigation(
    world: Option<Res<AuthorityWorld>>,
    installed: Option<Res<State<frame::MatchScope>>>,
    role: Res<RuntimeRole>,
    mut nav: ResMut<BotNav>,
    mut ready: ResMut<BotNavigationReady>,
    load: Option<Res<assets::MapLoadProcess>>,
) {
    ready.0 = frame::WorldReadiness::new(frame::ReadinessState::Pending);
    if !matches!(*role, RuntimeRole::Listen | RuntimeRole::Dedicated) {
        ready.0.state = frame::ReadinessState::Ready;
        return;
    }
    if !installed.is_some_and(|installed| *installed.get() == frame::MatchScope::Live) {
        return;
    }
    let Some(world) = world else {
        return;
    };
    if refresh_nav(&world.0, &mut nav, load.as_deref()) {
        ready.0.state = frame::ReadinessState::Ready;
    }
}

fn refresh_nav(world: &SimWorld, nav: &mut BotNav, load: Option<&assets::MapLoadProcess>) -> bool {
    let digest = nav_digest(world);
    if nav.graph.digest == digest && nav.graph.schema == NAV_SCHEMA && nav.graph.hull == NAV_HULL {
        return true;
    }
    if let Some((pending_digest, task, stage)) = nav.pending.as_mut()
        && *pending_digest == digest
    {
        if let Some(graph) = future::block_on(future::poll_once(task)) {
            nav.graph = graph;
            // The bake is only navigation once this graph is the one the bots
            // will read, which is here and not on the worker.
            if let Some(stage) = stage.take() {
                // What the walk actually produced: the links bots route over.
                // Nodes alone say how finely the grid was sampled; edges say
                // how much of the map turned out to be connected.
                let edges: usize = nav.graph.adj.iter().map(Vec::len).sum();
                stage.set_completed(edges as u64);
                stage.done();
            }
            nav.pending = None;
            return true;
        }
        return false;
    }
    // A bake is one indivisible walk of the grid, so nothing counts up while it
    // runs; the edge count is written once, when the graph it produced is the
    // one the bots read.
    let stage = load.map(|load| {
        load.progress
            .begin(asset_transport::StageId::Navigation, None)
    });
    let mut seeds = world.authored_spawn_origins();
    seeds.extend(
        world
            .objectives
            .compass
            .iter()
            .map(|objective| objective.origin),
    );
    let content = world.content();
    let atlas = content.atlas().cloned();
    if let Some(atlas) = &atlas {
        seeds.extend(
            atlas
                .portals
                .iter()
                .flat_map(|pair| [pair.a, pair.b])
                .filter(|door| door.world == sim::world_graph::WorldId::ROOT)
                .map(nav::portal_entrance),
        );
    }
    let bounds = nav::playable_bounds(world.clip_brushes(), &seeds);
    let collision = world.collision_scene();
    let generation = nav.graph.generation.wrapping_add(1);
    nav.pending = Some((
        digest,
        assets::load_pool().spawn(async move {
            let mut graph = navigation_for(&collision, bounds, &seeds, digest);
            if let Some(atlas) = &atlas {
                for (index, world) in atlas.worlds.iter().enumerate() {
                    let world_id = index + 1;
                    let offset = sim::world_graph::world_offset(world_id);
                    let door_seeds: Vec<_> = atlas
                        .portals
                        .iter()
                        .flat_map(|pair| [pair.a, pair.b])
                        .filter(|door| door.world.index() == world_id)
                        .map(nav::portal_entrance)
                        .collect();
                    let native: Vec<_> = door_seeds
                        .iter()
                        .map(|p| std::array::from_fn(|i| p[i] - offset[i]))
                        .collect();
                    let bounds = nav::playable_bounds(&world.brushes, &native).map(|(lo, hi)| {
                        (
                            std::array::from_fn(|i| lo[i] + offset[i]),
                            std::array::from_fn(|i| hi[i] + offset[i]),
                        )
                    });
                    let world_digest =
                        assets::fnv1a64_more(digest, &(world_id as u64).to_le_bytes());
                    let extra = navigation_for(&collision, bounds, &door_seeds, world_digest);
                    if !nav::append_world(&mut graph, extra) {
                        break;
                    }
                }
                nav::link_portals(&collision, &mut graph, atlas);
                diag::info!(
                    Sim,
                    "bots: atlas navigation: {} nodes, {} portal edges",
                    graph.nodes.len(),
                    graph
                        .adj
                        .iter()
                        .flatten()
                        .filter(|edge| edge.kind == nav::TraversalKind::Portal)
                        .count()
                );
            }
            graph.generation = generation;
            graph
        }),
        stage,
    ));
    false
}

const NAV_CACHE_KIND: &str = "nav";

fn nav_digest(world: &SimWorld) -> u64 {
    let parts = world.content_components();
    let mut digest = assets::fnv1a64_more(
        assets::fnv1a64(&parts.map.to_le_bytes()),
        &parts.models.to_le_bytes(),
    );
    if let Some(atlas) = world.content().atlas() {
        for world in &atlas.worlds {
            digest = assets::fnv1a64_more(digest, world.zone.as_bytes());
        }
        for pair in &atlas.portals {
            for door in [pair.a, pair.b] {
                digest = assets::fnv1a64_more(digest, &(door.world.index() as u64).to_le_bytes());
                for value in door
                    .base
                    .into_iter()
                    .chain(door.normal)
                    .chain([door.width, door.height])
                {
                    digest = assets::fnv1a64_more(digest, &value.to_le_bytes());
                }
            }
            digest = assets::fnv1a64_more(digest, &[u8::from(pair.flip)]);
        }
    }
    digest
}

/// The digest, schema and hull that decide in-memory reuse are the whole key,
/// so the same map read back from disk is the same graph the walk would bake.
fn nav_cache_key(digest: u64) -> String {
    format!("{digest:016x}-{NAV_SCHEMA}-{NAV_HULL:08x}")
}

/// Walking the grid costs seconds of load, and a match teardown drops the graph
/// with the roster, so the second start of a map would pay it again. Keep the
/// bake in the artifact cache under its own key and only walk on a miss.
fn navigation_for(
    collision: &sim::CollisionScene,
    bounds: Option<([f32; 3], [f32; 3])>,
    seeds: &[[f32; 3]],
    digest: u64,
) -> NavGraph {
    let key = nav_cache_key(digest);
    if let Some(graph) = assets::cache_get(NAV_CACHE_KIND, &key)
        .as_deref()
        .and_then(NavGraph::cache_decode)
        && graph.digest == digest
        && graph.schema == NAV_SCHEMA
        && graph.hull == NAV_HULL
    {
        diag::info!(
            Sim,
            "bots: navigation read from cache: {} nodes, {} drops{}",
            graph.nodes.len(),
            graph.drops,
            if graph.truncated { ", truncated" } else { "" }
        );
        return graph;
    }
    let started = std::time::Instant::now();
    let mut graph = NavGraph::default();
    bake_navigation(collision, bounds, seeds, digest, &mut graph);
    diag::info!(
        Sim,
        "bots: navigation prepared in {:.1}ms on worker",
        started.elapsed().as_secs_f64() * 1000.0
    );
    if let Err(error) = assets::cache_put(NAV_CACHE_KIND, &key, &graph.cache_encode()) {
        diag::warn!(Sim, "bots: navigation cache store {key}: {error}");
    }
    graph
}

fn bake_navigation(
    collision: &sim::CollisionScene,
    bounds: Option<([f32; 3], [f32; 3])>,
    seeds: &[[f32; 3]],
    digest: u64,
    graph: &mut NavGraph,
) {
    if graph.digest == digest && graph.schema == NAV_SCHEMA && graph.hull == NAV_HULL {
        return;
    }
    let generation = graph.generation.wrapping_add(1);
    let Some(bounds) = bounds else {
        *graph = NavGraph {
            generation,
            digest,
            schema: NAV_SCHEMA,
            hull: NAV_HULL,
            ..NavGraph::default()
        };
        return;
    };
    *graph = nav::bake_seeded(collision, bounds, digest, seeds);
    graph.generation = generation;
}

fn aim_viewangles(from: [f32; 3], target: [f32; 3]) -> [f32; 3] {
    let dir = [
        target[0] - from[0],
        target[1] - from[1],
        target[2] - from[2],
    ];
    if dir[0] == 0.0 && dir[1] == 0.0 {
        let pitch = if dir[2] < 0.0 {
            VIEW_PITCH_DOWN
        } else {
            -VIEW_PITCH_DOWN
        };
        return [pitch, 0.0, 0.0];
    }
    let mut angles = math_iw4::vect_to_angles(dir);
    angles[0] = math_iw4::angle_normalize_360(angles[0]);
    angles[1] = math_iw4::angle_normalize_360(angles[1]);
    if angles[0] > 180.0 {
        angles[0] -= 360.0;
    }
    angles[0] = angles[0].clamp(-VIEW_PITCH_DOWN, VIEW_PITCH_DOWN);
    angles
}
