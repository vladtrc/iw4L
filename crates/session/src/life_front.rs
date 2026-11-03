use std::collections::{HashMap, HashSet};

use bevy::prelude::*;
use frame::ScopeApp;
use frame::{
    LifeEndCause, LifeEnded, LifeFrontPublished, LifeStartReason, LifeStarted, PresentedPublished,
};
use net::{ClientSet, PresentedSnapshot};
use sim::ClientLifecycle;

#[derive(Resource, Default)]
struct LifeFrontCursors(HashMap<u32, LifeFrontCursor>);

#[derive(Clone, Copy, Debug)]
struct LifeFrontCursor {
    seq: u32,
    alive: bool,
}

fn publish_life_front(
    presented: Res<PresentedSnapshot>,
    mut cursors: ResMut<LifeFrontCursors>,
    mut started: MessageWriter<LifeStarted>,
    mut ended: MessageWriter<LifeEnded>,
) {
    let Some(snap) = presented.snapshot() else {
        cursors.0.clear();
        return;
    };

    let mut seen = HashSet::new();
    for (id, meta) in &snap.meta.clients {
        seen.insert(id.0);
        let alive = meta.lifecycle == ClientLifecycle::Alive;
        let seq = meta.life_sequence.0;
        let prev = cursors.0.get(&id.0).copied();
        match prev {
            None if alive => emit_started(&mut started, id.0, seq),
            None => {}
            Some(prev) if prev.alive && alive && prev.seq != seq => {
                emit_ended(&mut ended, id.0, prev.seq, LifeEndCause::Replaced);
                emit_started(&mut started, id.0, seq);
            }
            Some(prev) if prev.alive && !alive => {
                emit_ended(&mut ended, id.0, prev.seq, LifeEndCause::LeftAlive);
            }
            Some(prev) if !prev.alive && alive => {
                emit_started(&mut started, id.0, seq);
            }
            Some(_) => {}
        }
        cursors.0.insert(id.0, LifeFrontCursor { seq, alive });
    }

    let dropped: Vec<(u32, LifeFrontCursor)> = cursors
        .0
        .iter()
        .filter(|(client, _)| !seen.contains(client))
        .map(|(client, cursor)| (*client, *cursor))
        .collect();
    for (client, prev) in dropped {
        cursors.0.remove(&client);
        if prev.alive {
            emit_ended(&mut ended, client, prev.seq, LifeEndCause::Dropped);
        }
    }
}

fn emit_started(writer: &mut MessageWriter<LifeStarted>, client: u32, life: u32) {
    writer.write(LifeStarted {
        client,
        life,
        reason: LifeStartReason::BecameAlive,
    });
}

fn emit_ended(writer: &mut MessageWriter<LifeEnded>, client: u32, life: u32, cause: LifeEndCause) {
    writer.write(LifeEnded {
        client,
        life,
        cause,
    });
}

pub fn register_life_front(app: &mut App) {
    app.scoped::<LifeFrontCursors>(frame::MatchScope::Live)
        .add_message::<LifeStarted>()
        .add_message::<LifeEnded>()
        .add_systems(
            Update,
            publish_life_front
                .in_set(frame::InMatch)
                .in_set(PresentedPublished),
        )
        .configure_sets(
            Update,
            LifeFrontPublished
                .in_set(ClientSet::Present)
                .after(publish_life_front),
        );
}
