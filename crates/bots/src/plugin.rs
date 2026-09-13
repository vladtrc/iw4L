use bevy::prelude::*;
use net::{
    AUTHORITY_MS, ClientActionInbox, ClientCommandInbox, ClientSet, LocalPresentClient,
    PresentedSnapshot, look_angles_from_degrees,
};
use sim::{ClassId, ClientAction, ClientLifecycle};

use crate::brain::{BotSenses, Brain};
use crate::roster::{
    BotAddQueue, BotClassPool, BotFireQueue, BotHold, BotRoster, BotTpQueue, BotTpTarget,
    BotTpWhere,
};
use crate::unique_loadout::pick_class_id;
use frame::MatchTornDown;

const VIEW_PITCH_DOWN: f32 = 85.0;

pub struct BotsPlugin;

impl Plugin for BotsPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<BotRoster>()
            .init_resource::<BotClassPool>()
            .init_resource::<BotAddQueue>()
            .init_resource::<BotHold>()
            .init_resource::<BotTpQueue>()
            .init_resource::<BotFireQueue>()
            .add_systems(
                Update,
                (
                    drain_bot_add_queue,
                    reset_roster_on_match_torn_down,
                    boot_bots,
                    apply_bot_tp,
                    think_bots.in_set(ClientSet::Input),
                )
                    .chain(),
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

fn drain_bot_add_queue(mut queue: ResMut<BotAddQueue>, mut roster: ResMut<BotRoster>) {
    for count in queue.drain() {
        let added = roster.add_bots(count);
        diag::info!(
            Sim,
            "bots: add {count} → clients {:?}",
            added.iter().map(|id| id.0).collect::<Vec<_>>()
        );
    }
}

fn boot_bots(
    mut roster: ResMut<BotRoster>,
    mut actions: ResMut<ClientActionInbox>,
    mut request_ids: ResMut<net::ActionRequestIds>,
    local: Res<LocalPresentClient>,
    pool: Res<BotClassPool>,
) {
    if !pool.ready {
        return;
    }
    let seed = roster.seed;
    for bot in &mut roster.bots {
        if bot.id == local.0 {
            continue;
        }
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
    presented: Res<PresentedSnapshot>,
    local: Res<LocalPresentClient>,
) {
    let requests = queue.drain();
    if requests.is_empty() {
        return;
    }
    let local_ps = presented.alive_player(local.0);
    for request in requests {
        let ids: Vec<sim::ClientId> = match request.target {
            BotTpTarget::All => roster
                .bots
                .iter()
                .map(|b| b.id)
                .filter(|id| *id != local.0)
                .collect(),
            BotTpTarget::Id(id) => {
                if roster.is_bot(id) && id != local.0 {
                    vec![id]
                } else {
                    diag::warn!(Sim, "bots: tp skipped — client {} is not a bot", id.0);
                    Vec::new()
                }
            }
        };
        for id in ids {
            let current = presented
                .alive_player(id)
                .map(|ps| ps.viewangles)
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

fn think_bots(
    mut roster: ResMut<BotRoster>,
    mut cmds: ResMut<ClientCommandInbox>,
    presented: Res<PresentedSnapshot>,
    hold: Res<BotHold>,
    mut fire: ResMut<BotFireQueue>,
) {
    let Some(snapshot) = presented.snapshot() else {
        return;
    };
    let fires = fire.drain();
    for bot in &mut roster.bots {
        let senses = BotSenses::from_snapshot(snapshot, bot.id);
        if senses.lifecycle != ClientLifecycle::Alive {
            continue;
        }
        let mut cmd = if hold.0 {
            let mut cmd = playerstate_iw4::UserCmd {
                server_time: 0,
                ..playerstate_iw4::UserCmd::default()
            };
            if let Some(ps) = presented.alive_player(bot.id) {
                cmd.angles = look_angles_from_degrees(ps.viewangles);
            }
            cmd
        } else {
            bot.brain.think(&senses, AUTHORITY_MS)
        };
        if fires.iter().any(|target| match target {
            BotTpTarget::All => true,
            BotTpTarget::Id(id) => *id == bot.id,
        }) {
            cmd.buttons |= playerstate_iw4::buttons::ATTACK;
        }

        cmds.push(bot.id, None, cmd, None);
    }
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
