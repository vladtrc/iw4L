use bevy::prelude::*;
use frame::ScopeApp;

use crate::cue::CueHandle;
use crate::cue_execution::CueTrigger;
use crate::playback::TrackedCuePlayback;

struct TrackedSound {
    entity: u16,
    alias_index: u16,
    completion_client: sim::ClientId,
    voice: CueHandle,
}

#[derive(Resource, Default)]
struct ScriptSounds {
    pending: Vec<sim::ScriptAudioCommand>,
    voices: Vec<TrackedSound>,
}

#[derive(bevy::ecs::system::SystemParam)]
struct SoundPlayback<'w> {
    ready: Res<'w, crate::AudioReady>,
    loading: Option<Res<'w, assets::LoadingScreen>>,
    bank: Option<Res<'w, crate::SoundBank>>,
    cues: TrackedCuePlayback<'w>,
}

fn update(
    mut events: MessageReader<net::SvcScriptAudio>,
    mut sounds: ResMut<ScriptSounds>,
    mut player: SoundPlayback,
    role: Res<frame::RuntimeRole>,
    local: Res<net::LocalPresentClient>,
    mut actions: Option<ResMut<net::ClientActionInbox>>,
) {
    for event in events.read() {
        if let sim::ScriptAudioCommand::EntitySound { .. } = &event.0 {
            sounds.pending.push(event.0.clone());
        }
    }
    sounds.voices.retain(|sound| {
        if sound.voice.primary_active() {
            return true;
        }
        if *role == frame::RuntimeRole::Replay || local.0 != sound.completion_client {
            return false;
        }
        // A full reliable queue must not lose a completion and strand GSC.
        let Some(actions) = actions.as_mut() else {
            return true;
        };
        actions
            .push(
                local.0,
                sim::ClientAction::SoundDone {
                    entity: sound.entity,
                    alias_index: sound.alias_index,
                },
            )
            .is_err()
    });
    if !player.ready.0.is_ready() || player.loading.as_ref().is_some_and(|s| !s.is_complete()) {
        return;
    }
    let Some(bank) = player.bank.as_ref() else {
        return;
    };
    for command in std::mem::take(&mut sounds.pending) {
        let sim::ScriptAudioCommand::EntitySound {
            entity,
            alias_index,
            alias,
            origin,
            completion_client,
            master,
        } = command
        else {
            continue;
        };
        let (namespace, alias) = crate::aliases::namespace_alias(&alias);
        let voice = player.cues.trigger(CueTrigger {
            force_master: master,
            event: None,
            bank: bank.0.clone(),
            namespace,
            alias: alias.into(),
            bound: None,
            origin_inches: Some(origin),
            emitter: Some(u32::from(entity)),
            class: crate::SoundClass::World,
            pitch_scale: 1.0,
            volume_scale: 1.0,
            fallbacks: Vec::new(),
        });
        sounds.voices.push(TrackedSound {
            entity,
            alias_index,
            completion_client,
            voice,
        });
    }
}

pub(crate) fn register(app: &mut App) {
    app.scoped::<ScriptSounds>(frame::MatchScope::Live)
        .add_systems(
            Update,
            update
                .in_set(frame::InMatch)
                .in_set(net::ClientSet::Effects)
                .run_if(in_state(frame::Timeline::Live)),
        );
}
