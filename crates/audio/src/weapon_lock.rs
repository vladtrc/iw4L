use bevy::prelude::*;
use net::{CgFrameClock, LocalPresentClient, PresentedSnapshot};

use crate::{AliasCommand, PlayAlias, SND_ENT_LOCAL};

pub(crate) const ALIASES: [&str; 2] = ["javelin_clu_aquiring_lock", "javelin_clu_lock"];

#[derive(Default)]
struct LockAudio {
    owner: Option<(
        frame::WorldGeneration,
        sim::ClientId,
        sim::LifeSequence,
        u32,
    )>,
    stage: u8,
    last_time: i32,
    next_ping: i32,
}

pub(crate) fn register(app: &mut App) {
    app.add_systems(Update, present.in_set(net::ClientSet::Effects));
}

fn present(
    presented: Res<PresentedSnapshot>,
    local: Res<LocalPresentClient>,
    generation: Res<frame::WorldGeneration>,
    clock: Res<CgFrameClock>,
    mut cursor: Local<LockAudio>,
    mut commands: MessageWriter<AliasCommand>,
) {
    let state = presented.snapshot().and_then(|snapshot| {
        let meta = snapshot.meta.for_client(local.0)?;
        let ps = presented.player(local.0)?;
        (meta.lifecycle == sim::ClientLifecycle::Alive && ps.other_flags & 0x400 == 0).then_some((
            (*generation, local.0, meta.life_sequence, ps.weapon),
            if meta.weapon_lock.weapon == ps.weapon {
                meta.weapon_lock.flags & 3
            } else {
                0
            },
        ))
    });
    let now = clock.time();
    let owner = state.map(|(owner, _)| owner);
    let stage = state.map_or(0, |(_, stage)| stage);
    if cursor.owner != owner || cursor.stage != stage || now < cursor.last_time {
        if cursor.stage != 0 {
            for alias in ALIASES {
                commands.write(AliasCommand::Stop {
                    namespace: assets::AssetNamespace::Iw4,
                    alias: alias.to_owned(),
                    snd_ent: Some(SND_ENT_LOCAL),
                });
            }
        }
        cursor.owner = owner;
        cursor.stage = stage;
        cursor.next_ping = now;
    }
    cursor.last_time = now;
    if stage != 0 && now >= cursor.next_ping {
        commands.write(AliasCommand::Play(PlayAlias {
            namespace: assets::AssetNamespace::Iw4,
            alias: ALIASES[usize::from(stage & 2 != 0)].to_owned(),
            fallback: None,
            origin_inches: None,
            snd_ent: Some(SND_ENT_LOCAL),
        }));
        cursor.next_ping = if stage & 2 != 0 {
            i32::MAX
        } else {
            now.saturating_add(600)
        };
    }
}
