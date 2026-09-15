use bevy::ecs::system::SystemParam;
use bevy::prelude::*;
use net::{
    AUTHORITY_MS, AuthorityClock, AuthorityWorld, ClientActionInbox, ClientCommandInbox,
    LocalPresentClient, authority_should_tick, look_angles_from_degrees,
};
use sim::{ClassId, ClientAction, ClientLifecycle, SimWorld, Tick};

use crate::nav::{self, NAV_HULL, NAV_SCHEMA, NavGraph};
use crate::query::{Budgeted, TraceBudget};
use crate::roster::{
    BotAddQueue, BotClassPool, BotFireQueue, BotHold, BotRoster, BotTpQueue, BotTpTarget,
    BotTpWhere,
};
use crate::sensor;
use crate::unique_loadout::pick_class_id;
use frame::{AuthoritySet, MatchTornDown};

const VIEW_PITCH_DOWN: f32 = 85.0;
const TRACE_QUOTA: u32 = 96;
const ASTAR_QUOTA: u32 = 2048;

#[derive(Resource, Default)]
struct BotNav {
    graph: NavGraph,
}

pub struct BotsPlugin;

impl Plugin for BotsPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<BotRoster>()
            .init_resource::<BotClassPool>()
            .init_resource::<BotAddQueue>()
            .init_resource::<BotHold>()
            .init_resource::<BotTpQueue>()
            .init_resource::<BotFireQueue>()
            .init_resource::<BotNav>()
            .add_systems(
                Update,
                (
                    drain_bot_add_queue,
                    reset_roster_on_match_torn_down,
                    evict_bots_claiming_local_client,
                    boot_bots,
                    apply_bot_tp,
                )
                    .chain(),
            )
            .add_systems(
                FixedUpdate,
                (evict_bots_claiming_local_client, think_bots)
                    .chain()
                    .in_set(AuthoritySet::Ingress)
                    .run_if(authority_should_tick),
            );
    }
}

fn reset_roster_on_match_torn_down(
    mut torn: MessageReader<MatchTornDown>,
    mut roster: ResMut<BotRoster>,
    mut pool: ResMut<BotClassPool>,
) {
    if torn.read().len() == 0 {
        return;
    }
    *roster = BotRoster::default();
    *pool = BotClassPool::default();
}

fn drain_bot_add_queue(
    mut queue: ResMut<BotAddQueue>,
    mut roster: ResMut<BotRoster>,
    local: Res<LocalPresentClient>,
    world: Option<Res<AuthorityWorld>>,
) {
    let requests = queue.drain();
    if requests.is_empty() {
        return;
    }
    let mut taken = vec![local.0];
    if let Some(world) = world.as_ref() {
        taken.extend(world.0.clients_scoreboard().into_iter().map(|(id, _)| id));
    }
    for count in requests {
        let added = roster.add_bots(count, &taken);
        if added.len() < count as usize {
            diag::warn!(
                Sim,
                "bots: add {count} — only {} minted, the roster is full",
                added.len()
            );
        }
        diag::info!(
            Sim,
            "bots: add {count} → clients {:?}",
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
    mut request_ids: ResMut<net::ActionRequestIds>,
    pool: Res<BotClassPool>,
) {
    if !pool.ready {
        return;
    }
    let seed = roster.seed;
    for bot in &mut roster.bots {
        if bot.joined {
            continue;
        }
        let class_id = pick_class_id(&pool.ids, seed, bot.id.0).unwrap_or(ClassId(0));
        let join_id = request_ids.allocate();
        let class_request = request_ids.allocate();
        let name_request = request_ids.allocate();
        let queued = [
            actions.push(
                bot.id,
                ClientAction::JoinMatch {
                    request_id: join_id,
                },
            ),
            actions.push(
                bot.id,
                ClientAction::SelectClass {
                    request_id: class_request,
                    class_id,
                    revision: 1,
                },
            ),
            actions.push(
                bot.id,
                ClientAction::SetName {
                    request_id: name_request,
                    name: entity_iw4::pack_client_state_name("bot"),
                },
            ),
        ];
        if let Some(error) = queued.into_iter().find_map(Result::err) {
            diag::warn!(Sim, "bots: client {} not booted — {error}", bot.id.0);
            continue;
        }
        bot.joined = true;
        bot.class_requested = true;
        diag::info!(
            Sim,
            "bots: JoinMatch+SelectClass client={} class={} request_id={class_request}",
            bot.id.0,
            class_id.0,
        );
    }
}

fn apply_bot_tp(
    mut queue: ResMut<BotTpQueue>,
    roster: Res<BotRoster>,
    mut actions: ResMut<ClientActionInbox>,
    mut request_ids: ResMut<net::ActionRequestIds>,
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
            let request_id = request_ids.allocate();
            if let Err(error) = actions.push(
                id,
                ClientAction::Move {
                    request_id,
                    origin,
                    angles,
                },
            ) {
                diag::warn!(Sim, "bots: tp client={} not queued — {error}", id.0);
                continue;
            }
            diag::info!(
                Sim,
                "bots: tp client={} ({:.1} {:.1} {:.1}) request_id={request_id}",
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
    nav: ResMut<'w, BotNav>,
    roster: ResMut<'w, BotRoster>,
    cmds: ResMut<'w, ClientCommandInbox>,
    hold: Res<'w, BotHold>,
    fire: ResMut<'w, BotFireQueue>,
}

fn think_bots(mut p: ThinkBots) {
    refresh_nav(&mut p.world.0, &mut p.nav.graph);
    let snapshot = p.world.0.snapshot(Tick(p.clock.tick.saturating_sub(1)));
    let fires = p.fire.drain();
    let mut budget = TraceBudget::new(TRACE_QUOTA);
    let mut astar = ASTAR_QUOTA;
    for bot in &mut p.roster.bots {
        let mut queried = Budgeted {
            world: &mut p.world.0,
            budget: &mut budget,
        };
        let Some(obs) = sensor::observe(&snapshot, bot.id, &mut queried) else {
            continue;
        };
        if obs.self_state.lifecycle != ClientLifecycle::Alive {
            continue;
        }
        let mut cmd = if p.hold.0 {
            let mut cmd = playerstate_iw4::UserCmd {
                server_time: p.clock.time_ms,
                ..playerstate_iw4::UserCmd::default()
            };
            cmd.angles = look_angles_from_degrees(obs.self_state.viewangles);
            cmd.weapon = obs.self_state.weapon;
            cmd.weapon_mapped = obs.self_state.weapon;
            cmd
        } else {
            let mut cmd = bot.brain.drive_nav(
                &obs,
                &mut queried,
                Some(&p.nav.graph),
                &mut astar,
                AUTHORITY_MS,
            );
            cmd.server_time = p.clock.time_ms;
            cmd
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

fn refresh_nav(world: &mut SimWorld, graph: &mut NavGraph) {
    let digest = world.content_digest();
    if graph.digest == digest && graph.schema == NAV_SCHEMA && graph.hull == NAV_HULL {
        return;
    }
    let seeds = world.authored_spawn_origins();
    let Some(bounds) = nav::playable_bounds(world.clip_brushes(), &seeds) else {
        *graph = NavGraph {
            digest,
            schema: NAV_SCHEMA,
            hull: NAV_HULL,
            ..NavGraph::default()
        };
        return;
    };
    *graph = nav::bake_seeded(world, bounds, digest, &seeds);
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
