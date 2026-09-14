use math_iw4::angle_vectors;
use sim::{ClientId, ClientLifecycle, Snapshot};

use crate::observation::{
    BotObservation, Contact, DEFAULT_OBJECTIVE_RADIUS, KnowledgeSource, ModeObjective, SelfState,
};
use crate::query::{SightSample, WorldQuery};

pub const SIGHT_RANGE_IN: f32 = 5906.0;
pub const SIGHT_FOV_DEG: f32 = 90.0;
const PROBE_CHEST_Z: f32 = 48.0;
const PROBE_HEAD_Z: f32 = 64.0;
const PROBE_FEET_Z: f32 = 8.0;

pub fn observe(
    snapshot: &Snapshot,
    bot: ClientId,
    world: &mut impl WorldQuery,
) -> Option<BotObservation> {
    let meta = snapshot.meta.for_client(bot)?;
    let ps = snapshot
        .players
        .iter()
        .find(|(id, _)| *id == bot)
        .map(|(_, ps)| ps)?;
    let team = meta.client_state_team;
    let self_state = SelfState {
        id: bot,
        lifecycle: meta.lifecycle,
        origin: ps.origin,
        viewangles: ps.viewangles,
        view_height: ps.view_height_current,
        stance: crate::observation::Stance::from_view_height(ps.view_height_current),
        health: ps.health,
        weapon: ps.weapon as u16,
        weapon_class: world.weapon_class(ps.weapon as u16),
        ammo_clip: meta.ammo_clip,
        ammo_stock: meta.ammo_stock,
        team,
    };
    let mut seen = Vec::new();
    if meta.lifecycle == ClientLifecycle::Alive {
        let eye = [
            ps.origin[0],
            ps.origin[1],
            ps.origin[2]
                + if ps.view_height_current > 1.0 {
                    ps.view_height_current
                } else {
                    60.0
                },
        ];
        let (forward, _, _) = angle_vectors(ps.viewangles);
        let half = (SIGHT_FOV_DEG * 0.5).to_radians().cos();
        for (id, other) in &snapshot.players {
            if *id == bot {
                continue;
            }
            let Some(other_meta) = snapshot.meta.for_client(*id) else {
                continue;
            };
            if other_meta.lifecycle != ClientLifecycle::Alive {
                continue;
            }
            if snapshot.meta.kind.is_team() && other_meta.client_state_team == team {
                continue;
            }
            if !in_range_and_fov(ps.origin, other.origin, forward, half) {
                continue;
            }
            if probe_visible(world, bot, eye, other.origin, *id) {
                seen.push(Contact {
                    id: *id,
                    origin: other.origin,
                    source: KnowledgeSource::CurrentlySeen,
                    seen_tick: snapshot.tick.0,
                    confidence: 1.0,
                });
            }
        }
    }
    let objectives = public_objectives(snapshot, bot, team);
    Some(BotObservation {
        tick: snapshot.tick.0,
        time_ms: snapshot.tick.0 as i32 * 50,
        self_state,
        seen,
        events: Vec::new(),
        objective: objectives
            .iter()
            .copied()
            .min_by(|a, b| dist2(ps.origin, a.origin).total_cmp(&dist2(ps.origin, b.origin))),
        objectives,
    })
}

fn in_range_and_fov(from: [f32; 3], to: [f32; 3], forward: [f32; 3], min_dot: f32) -> bool {
    let d = [to[0] - from[0], to[1] - from[1], to[2] - from[2]];
    let len2 = d[0] * d[0] + d[1] * d[1] + d[2] * d[2];
    if len2 > SIGHT_RANGE_IN * SIGHT_RANGE_IN || len2 <= 0.0001 {
        return false;
    }
    let len = len2.sqrt();
    let dir = [d[0] / len, d[1] / len, d[2] / len];
    dir[0] * forward[0] + dir[1] * forward[1] + dir[2] * forward[2] >= min_dot
}

fn probe_visible(
    world: &mut impl WorldQuery,
    bot: ClientId,
    eye: [f32; 3],
    origin: [f32; 3],
    target: ClientId,
) -> bool {
    let probes = [
        [origin[0], origin[1], origin[2] + PROBE_CHEST_Z],
        [origin[0], origin[1], origin[2] + PROBE_HEAD_Z],
        [origin[0], origin[1], origin[2] + PROBE_FEET_Z],
    ];
    for probe in probes {
        match world.sight_ray(eye, probe, bot) {
            SightSample::Clear => return true,
            SightSample::HitPlayer { client, .. } if client == target => return true,
            SightSample::Unknown | SightSample::Blocked { .. } | SightSample::HitPlayer { .. } => {}
        }
    }
    false
}

fn public_objectives(snapshot: &Snapshot, bot: ClientId, team: i32) -> Vec<ModeObjective> {
    snapshot
        .meta
        .objectives
        .flags
        .iter()
        .filter(|f| f.owner as i32 != team)
        .map(|f| ModeObjective {
            origin: f.origin,
            touching: f.users.contains(&bot),
            use_button: false,
            radius: DEFAULT_OBJECTIVE_RADIUS,
        })
        .chain(
            snapshot
                .meta
                .objectives
                .bombs
                .iter()
                .filter(|b| {
                    !b.destroyed
                        && (b.planted_at_ms.is_some()
                            != (snapshot.meta.objectives.attackers as i32 == team))
                })
                .map(|b| ModeObjective {
                    origin: b.view.origin,
                    touching: b.view.users.contains(&bot),
                    use_button: true,
                    radius: DEFAULT_OBJECTIVE_RADIUS,
                }),
        )
        .collect()
}

fn dist2(a: [f32; 3], b: [f32; 3]) -> f32 {
    let dx = a[0] - b[0];
    let dy = a[1] - b[1];
    let dz = a[2] - b[2];
    dx * dx + dy * dy + dz * dz
}
