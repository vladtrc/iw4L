use asset_game::WeaponSoundSlot;
use bevy::prelude::*;

use crate::backend::AudioScope;
use crate::sources::{DesiredSource, SourceKey};
use crate::{AudioRuntime, FlameFire};

const STREAM_GRACE_SECS: f32 = 0.25;

#[derive(Component, Default)]
pub(crate) struct FlameStream {
    weapon: u32,
    player_view: bool,
    origin_inches: [f32; 3],
    last_fire: f32,
    pub source: Option<DesiredSource>,
}

fn slot_pair(
    player_view: bool,
    world: WeaponSoundSlot,
    player: WeaponSoundSlot,
) -> [WeaponSoundSlot; 2] {
    if player_view {
        [player, world]
    } else {
        [world, player]
    }
}

pub(crate) fn update(
    mut commands: Commands,
    mut fires: MessageReader<FlameFire>,
    mut weapon_audio: crate::weapon_sounds::WeaponSoundOutput,
    time: Res<Time>,
    runtime: Res<AudioRuntime>,
    mut entities: Query<(Entity, &net::CEntity, Option<&mut FlameStream>)>,
) {
    if !weapon_audio.is_available() {
        fires.clear();
        for (entity, _, stream) in &mut entities {
            if stream.is_some() {
                commands.entity(entity).remove::<FlameStream>();
            }
        }
        return;
    }
    let fires: Vec<_> = fires.read().collect();
    let now = time.elapsed_secs();
    for (entity, identity, mut current) in &mut entities {
        let number = u32::from(identity.number());
        let had_stream = current.is_some();
        let mut created = (!had_stream && fires.iter().any(|fire| fire.entity == entity))
            .then(FlameStream::default);
        let Some(stream) = current.as_deref_mut().or(created.as_mut()) else {
            continue;
        };
        for fire in fires.iter().filter(|fire| fire.entity == entity) {
            if stream.weapon != fire.weapon {
                if let Some(source) = stream.source.take() {
                    runtime.stop_source(source.key);
                }
                *stream = FlameStream {
                    weapon: fire.weapon,
                    player_view: fire.player_view,
                    origin_inches: fire.origin_inches,
                    last_fire: now,
                    source: None,
                };
                weapon_audio.write_slots(
                    stream.weapon,
                    slot_pair(
                        stream.player_view,
                        WeaponSoundSlot::FireStart,
                        WeaponSoundSlot::FireStartPlayer,
                    ),
                    (!stream.player_view).then_some(stream.origin_inches),
                    Some(number),
                );
            }
            stream.player_view = fire.player_view;
            stream.origin_inches = fire.origin_inches;
            stream.last_fire = now;
        }
        if now - stream.last_fire > STREAM_GRACE_SECS {
            weapon_audio.write_slots(
                stream.weapon,
                slot_pair(
                    stream.player_view,
                    WeaponSoundSlot::FireStop,
                    WeaponSoundSlot::FireStopPlayer,
                ),
                (!stream.player_view).then_some(stream.origin_inches),
                Some(number),
            );
            if let Some(source) = stream.source.take() {
                runtime.stop_source(source.key);
            }
            if had_stream {
                commands.entity(entity).remove::<FlameStream>();
            }
            continue;
        }
        if stream.source.is_none()
            && let Some(cue) = weapon_audio.source_cue(
                &runtime,
                stream.weapon,
                slot_pair(
                    stream.player_view,
                    WeaponSoundSlot::FireLoop,
                    WeaponSoundSlot::FireLoopPlayer,
                ),
                number,
            )
        {
            stream.source = Some(DesiredSource {
                key: SourceKey {
                    scope: AudioScope::Match,
                    object: entity.to_bits(),
                    slot: 8,
                },
                version: 1,
                cue,
                origin_inches: None,
                start_frame: runtime.audio_frame(),
                gain: 1.0,
                rate: 1.0,
                audible: true,
            });
        }
        if let Some(source) = stream.source.as_mut() {
            source.origin_inches = (!stream.player_view).then_some(stream.origin_inches);
        }
        if let Some(created) = created {
            commands.entity(entity).insert(created);
        }
    }
}

pub(crate) fn stop(
    event: On<Remove<FlameStream>>,
    streams: Query<&FlameStream>,
    runtime: Res<AudioRuntime>,
) {
    if let Ok(stream) = streams.get(event.entity)
        && let Some(source) = &stream.source
    {
        runtime.stop_source(source.key);
    }
}
