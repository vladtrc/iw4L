//! The loops a destructible speaks while it sits in a stage: gas hissing out
//! of a punctured tank, a propane fire burning on the cap. The host publishes
//! which alias each prop is speaking and where; this reconciles that set
//! against the positional emitters that carry it.

use crate::ambient::{MapAmbient, MapEmitter, SoundBankNamespace};
use crate::clip_store::{ClipStore, clip_keys_for_alias};
use crate::pcm::PcmAudio;
use crate::playback::{MissingAliasGaps, SharedPlayAssets, SoundBank};
use assets::AssetNamespace;
use bevy::prelude::*;
use net::PresentedSnapshot;
use sim::DestructibleLoopSound;
use std::sync::Arc;

#[derive(Component)]
pub(crate) struct DestructibleLoop {
    owner: u32,
    alias: String,
}

pub(crate) fn update(
    mut commands: Commands,
    presented: Res<PresentedSnapshot>,
    playing: Query<(Entity, &DestructibleLoop)>,
    bank: Option<Res<SoundBank>>,
    namespace: Option<Res<SoundBankNamespace>>,
    mut clips: Option<ResMut<ClipStore>>,
    mut pcm: ResMut<Assets<PcmAudio>>,
    mut shared: ResMut<SharedPlayAssets>,
    mut gaps: ResMut<MissingAliasGaps>,
) {
    let Some(snapshot) = presented.snapshot() else {
        return;
    };
    let speaking: Vec<(&DestructibleLoopSound, &str)> = snapshot
        .meta
        .world_objects
        .destructible_loop_sounds
        .iter()
        .filter_map(|row| {
            let alias = snapshot
                .meta
                .sound_aliases
                .iter()
                .find(|(index, _)| *index == row.alias_index)?;
            Some((row, alias.1.as_str()))
        })
        .collect();

    for (entity, loop_sound) in &playing {
        if !speaking.iter().any(|(row, alias)| {
            row.owner.to_wire() == loop_sound.owner && *alias == loop_sound.alias
        }) {
            commands.entity(entity).try_despawn();
            diag::info!(
                Audio,
                "audio: destructible loop `{}` stopped",
                loop_sound.alias
            );
        }
    }

    let (Some(bank), Some(clips)) = (bank, clips.as_mut()) else {
        return;
    };
    let ns = namespace.map_or(AssetNamespace::Iw4, |map| map.namespace);
    for (row, alias) in speaking {
        if playing
            .iter()
            .any(|(_, playing)| playing.owner == row.owner.to_wire() && playing.alias == alias)
        {
            continue;
        }
        let Some(key) = clip_keys_for_alias(&bank.0, ns, alias).into_iter().next() else {
            gaps.record(alias);
            continue;
        };
        clips.request(key.clone());
        let Some(Ok(audio)) = clips.ready(&key) else {
            continue;
        };
        let Some(sound) = bank
            .0
            .sound_in(ns, alias)
            .or_else(|| {
                bank.0
                    .index_unique(alias)
                    .and_then(|index| bank.0.sounds.get(index))
            })
            .and_then(|s| s.aliases.first())
        else {
            gaps.record(alias);
            continue;
        };
        let knots = sound
            .volume_falloff
            .as_ref()
            .map(|curve| shared.intern_curve(&curve.name, &curve.knots))
            .unwrap_or_else(|| Arc::from(Vec::<[f32; 2]>::new()));
        if knots.is_empty() {
            diag::warn!(
                Audio,
                "audio: destructible loop `{alias}` has no falloff curve (typed gap)"
            );
        }
        commands.spawn((
            DestructibleLoop {
                owner: row.owner.to_wire(),
                alias: alias.to_owned(),
            },
            MapAmbient,
            MapEmitter {
                origin_inches: row.origin,
                dist_min: sound.dist_min,
                dist_max: sound.dist_max,
                knots,
                base_gain: sound.vol_min.max(0.0),
                pcm: pcm.add(audio),
                live_pan: None,
            },
            Transform::from_translation(Vec3::from_array(row.origin)),
        ));
        diag::info!(Audio, "audio: destructible loop `{alias}`");
    }
}
