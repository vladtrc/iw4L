use std::collections::HashMap;

use asset_core::FamilyId;
use bevy::prelude::*;
use net::PresentedSnapshot;

use crate::ambient::SoundBankNamespace;
use crate::backend::AudioScope;
use crate::playback::SoundBank;
use crate::runtime::AudioRuntime;
use crate::sources::{DesiredSource, SourceKey};

struct PresentedLoop {
    alias: String,
    namespace: FamilyId,
    emitter: Option<u32>,
    source: DesiredSource,
}

#[derive(Resource, Default)]
pub(crate) struct DestructibleSources {
    next_version: u64,
    live: HashMap<u32, PresentedLoop>,
    pub desired: Vec<DesiredSource>,
}

pub(crate) fn update(
    presented: Res<PresentedSnapshot>,
    bank: Option<Res<SoundBank>>,
    namespace: Option<Res<SoundBankNamespace>>,
    runtime: Res<AudioRuntime>,
    mut sources: ResMut<DestructibleSources>,
) {
    let Some(snapshot) = presented.snapshot() else {
        return;
    };
    let ns = namespace.map_or(FamilyId::IW4, |map| map.namespace);
    let mut speaking: Vec<_> = snapshot
        .meta
        .world_objects
        .destructible_loop_sounds
        .iter()
        .filter_map(|row| {
            let (_, alias) = snapshot
                .meta
                .sound_aliases
                .iter()
                .find(|(index, _)| *index == row.alias_index)?;
            Some((row, alias.as_str()))
        })
        .collect();
    speaking.sort_unstable_by_key(|(row, _)| row.owner.to_wire());
    speaking.dedup_by_key(|(row, _)| row.owner.to_wire());
    speaking.truncate(crate::runtime::LOGICAL_INSTANCES);
    sources.live.retain(|owner, _| {
        speaking
            .binary_search_by_key(owner, |(row, _)| row.owner.to_wire())
            .is_ok()
    });
    sources.desired.clear();
    for (row, alias) in speaking {
        let (ns, alias) = alias
            .split_once(':')
            .and_then(|(family, name)| FamilyId::parse(family).map(|ns| (ns, name)))
            .unwrap_or((ns, alias));
        let owner = row.owner.to_wire();
        let changed = sources.live.get(&owner).is_none_or(|current| {
            current.alias != alias || current.namespace != ns || current.emitter != row.snd_ent
        });
        if changed {
            sources.live.remove(&owner);
            let Some(bank) = &bank else {
                continue;
            };
            let Some(cue) = runtime.source_cue(crate::sources::SourceCueRequest {
                bank: bank.0.clone(),
                namespace: ns,
                alias: alias.into(),
                emitter: row.snd_ent,
                scope: AudioScope::Match,
                group: None,
            }) else {
                continue;
            };
            sources.next_version = sources
                .next_version
                .checked_add(1)
                .expect("source version exhausted");
            let version = sources.next_version;
            sources.live.insert(
                owner,
                PresentedLoop {
                    alias: alias.into(),
                    namespace: ns,
                    emitter: row.snd_ent,
                    source: DesiredSource {
                        key: SourceKey {
                            scope: AudioScope::Match,
                            object: u64::from(owner),
                            slot: 1,
                        },
                        version,
                        cue,
                        origin_inches: Some(row.origin),
                        start_frame: runtime.audio_frame(),
                        gain: 1.0,
                        rate: 1.0,
                        audible: true,
                    },
                },
            );
        }
        let current = sources.live.get_mut(&owner).expect("presented loop");
        current.source.origin_inches = Some(row.origin);
        let desired = current.source.clone();
        sources.desired.push(desired);
    }
}

impl DestructibleSources {
    pub(crate) fn retire_emitter(&mut self, emitter: u32, runtime: &crate::AudioRuntime) {
        for row in self
            .live
            .values()
            .filter(|row| row.emitter == Some(emitter))
        {
            runtime.stop_source(row.source.key);
        }

        self.live.retain(|_, row| row.emitter != Some(emitter));
        self.desired
            .retain(|source| source.cue.emitter != Some(emitter));
    }
}
