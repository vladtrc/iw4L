use std::collections::{HashMap, HashSet};

use bevy::prelude::*;
use frame::{
    AbortKillcam, BeginKillcam, KillcamEnded, LifeEndCause, LifeEnded, LifeFrontPublished,
    LifeStartReason, LifeStarted, PresentedPublished, SpawnedPlayer,
};
use net::{ClientSet, LocalPresentClient, PresentedSnapshot};
use sim::ClientLifecycle;

#[derive(Resource, Debug, Default, Clone)]
pub struct LifeNotifyCensus {
    pub begin_killcam: u32,
    pub abort_killcam: u32,
    pub killcam_ended: u32,
    pub spawned_player: u32,
}

#[derive(Resource, Debug, Default, Clone)]
pub struct LifeFrontCensus {
    pub started: u32,
    pub ended: u32,
    pub tick_started: u32,
    pub tick_ended: u32,

    pub local_seq: Option<u32>,
    cursors: HashMap<u32, LifeFrontCursor>,
}

#[derive(Clone, Copy, Debug)]
struct LifeFrontCursor {
    seq: u32,
    alive: bool,
}

fn on_begin_killcam(notify: On<BeginKillcam>, mut census: ResMut<LifeNotifyCensus>) {
    let _ = notify.event().entity;
    census.begin_killcam = census.begin_killcam.saturating_add(1);
}

fn on_abort_killcam(notify: On<AbortKillcam>, mut census: ResMut<LifeNotifyCensus>) {
    let _ = notify.event().entity;
    census.abort_killcam = census.abort_killcam.saturating_add(1);
}

fn on_killcam_ended(notify: On<KillcamEnded>, mut census: ResMut<LifeNotifyCensus>) {
    let _ = notify.event().entity;
    census.killcam_ended = census.killcam_ended.saturating_add(1);
}

fn on_spawned_player(notify: On<SpawnedPlayer>, mut census: ResMut<LifeNotifyCensus>) {
    let _ = notify.event().entity;
    census.spawned_player = census.spawned_player.saturating_add(1);
}

fn publish_life_front(
    presented: Res<PresentedSnapshot>,
    local: Res<LocalPresentClient>,
    mut census: ResMut<LifeFrontCensus>,
    mut started: MessageWriter<LifeStarted>,
    mut ended: MessageWriter<LifeEnded>,
) {
    census.tick_started = 0;
    census.tick_ended = 0;
    let Some(snap) = presented.snapshot() else {
        census.cursors.clear();
        return;
    };

    let mut seen = HashSet::new();
    for (id, meta) in &snap.meta.clients {
        seen.insert(id.0);
        let alive = meta.lifecycle == ClientLifecycle::Alive;
        let seq = meta.life_sequence.0;
        let prev = census.cursors.get(&id.0).copied();
        match prev {
            None if alive => emit_started(&mut started, &mut census, local.0.0, id.0, seq),
            None => {}
            Some(prev) if prev.alive && alive && prev.seq != seq => {
                emit_ended(
                    &mut ended,
                    &mut census,
                    id.0,
                    prev.seq,
                    LifeEndCause::Replaced,
                );
                emit_started(&mut started, &mut census, local.0.0, id.0, seq);
            }
            Some(prev) if prev.alive && !alive => {
                emit_ended(
                    &mut ended,
                    &mut census,
                    id.0,
                    prev.seq,
                    LifeEndCause::LeftAlive,
                );
            }
            Some(prev) if !prev.alive && alive => {
                emit_started(&mut started, &mut census, local.0.0, id.0, seq);
            }
            Some(_) => {}
        }
        census.cursors.insert(id.0, LifeFrontCursor { seq, alive });
    }

    let dropped: Vec<(u32, LifeFrontCursor)> = census
        .cursors
        .iter()
        .filter(|(client, _)| !seen.contains(client))
        .map(|(client, cursor)| (*client, *cursor))
        .collect();
    for (client, prev) in dropped {
        census.cursors.remove(&client);
        if prev.alive {
            emit_ended(
                &mut ended,
                &mut census,
                client,
                prev.seq,
                LifeEndCause::Dropped,
            );
        }
    }
}

fn emit_started(
    writer: &mut MessageWriter<LifeStarted>,
    census: &mut LifeFrontCensus,
    local: u32,
    client: u32,
    life: u32,
) {
    writer.write(LifeStarted {
        client,
        life,
        reason: LifeStartReason::BecameAlive,
    });
    census.tick_started = census.tick_started.saturating_add(1);
    census.started = census.started.saturating_add(1);
    if client == local {
        census.local_seq = Some(life);
    }
}

fn emit_ended(
    writer: &mut MessageWriter<LifeEnded>,
    census: &mut LifeFrontCensus,
    client: u32,
    life: u32,
    cause: LifeEndCause,
) {
    writer.write(LifeEnded {
        client,
        life,
        cause,
    });
    census.tick_ended = census.tick_ended.saturating_add(1);
    census.ended = census.ended.saturating_add(1);
}

pub fn register_life_front(app: &mut App) {
    app.init_resource::<LifeNotifyCensus>()
        .init_resource::<LifeFrontCensus>()
        .add_message::<LifeStarted>()
        .add_message::<LifeEnded>()
        .add_observer(on_begin_killcam)
        .add_observer(on_abort_killcam)
        .add_observer(on_killcam_ended)
        .add_observer(on_spawned_player)
        .add_systems(Update, publish_life_front.in_set(PresentedPublished))
        .configure_sets(
            Update,
            LifeFrontPublished
                .in_set(ClientSet::Present)
                .after(publish_life_front),
        );
}
