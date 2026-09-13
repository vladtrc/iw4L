use std::time::{Duration, Instant};

use bevy::{
    audio::{AudioPlayer, AudioSink, AudioSinkPlayback, PlaybackSettings, Volume},
    prelude::*,
};
use frame::ClientSet;

use crate::pcm::{LoopingPcmAudio, LoopingPcmPlayback, PcmAudio};
use crate::voice::reclaim_finished_voices;

const STARTING_TIMEOUT: Duration = Duration::from_millis(250);

#[derive(Resource, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct MatchEpoch(pub u64);

impl MatchEpoch {
    pub fn bump(&mut self) {
        self.0 = self.0.wrapping_add(1);
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AudioScope {
    Menu,
    Match,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum VoiceKind {
    Oneshot,
    Loop,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum VoiceOwner {
    Exclusive,

    Attached,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum VoicePhase {
    Starting,
    Playing,
}

#[derive(Component, Debug)]
pub struct Voice {
    pub epoch: u64,
    pub scope: AudioScope,
    kind: VoiceKind,
    owner: VoiceOwner,
    phase: VoicePhase,
    started_at: Instant,
}

pub(crate) fn register(app: &mut App) {
    app.init_resource::<MatchEpoch>().add_systems(
        Update,
        (cancel_stale_match_voices, advance_voice_phases)
            .chain()
            .before(reclaim_finished_voices)
            .in_set(ClientSet::Effects),
    );
}

pub(crate) fn spawn_oneshot(
    commands: &mut Commands,
    handle: Handle<PcmAudio>,
    volume: Volume,
    speed: f32,
    epoch: u64,
    scope: AudioScope,
) -> Entity {
    commands
        .spawn((
            AudioPlayer(handle),
            PlaybackSettings::ONCE.with_volume(volume).with_speed(speed),
            Voice {
                epoch,
                scope,
                kind: VoiceKind::Oneshot,
                owner: VoiceOwner::Exclusive,
                phase: VoicePhase::Starting,
                started_at: Instant::now(),
            },
        ))
        .id()
}

pub(crate) fn spawn_loop(
    commands: &mut Commands,
    handle: Handle<LoopingPcmAudio>,
    volume: Volume,
    epoch: u64,
    scope: AudioScope,
) -> Entity {
    commands
        .spawn((
            LoopingPcmPlayback::new(handle, volume),
            Voice {
                epoch,
                scope,
                kind: VoiceKind::Loop,
                owner: VoiceOwner::Exclusive,
                phase: VoicePhase::Starting,
                started_at: Instant::now(),
            },
        ))
        .id()
}

pub(crate) fn attach_loop(
    commands: &mut Commands,
    entity: Entity,
    handle: Handle<LoopingPcmAudio>,
    volume: Volume,
    epoch: u64,
) {
    commands.entity(entity).insert((
        LoopingPcmPlayback::new(handle, volume),
        Voice {
            epoch,
            scope: AudioScope::Match,
            kind: VoiceKind::Loop,
            owner: VoiceOwner::Attached,
            phase: VoicePhase::Starting,
            started_at: Instant::now(),
        },
    ));
}

pub(crate) fn detach_loop(commands: &mut Commands, entity: Entity) {
    commands.entity(entity).remove::<(
        AudioPlayer<LoopingPcmAudio>,
        PlaybackSettings,
        AudioSink,
        Voice,
    )>();
}

pub(crate) fn stop(commands: &mut Commands, entity: Entity) {
    commands.entity(entity).try_despawn();
}

fn advance_voice_phases(
    mut voices: Query<(Entity, &mut Voice, Option<&AudioSink>)>,
    mut commands: Commands,
) {
    let now = Instant::now();
    for (entity, mut voice, sink) in &mut voices {
        match voice.phase {
            VoicePhase::Starting => {
                if sink.is_some() {
                    voice.phase = VoicePhase::Playing;
                    continue;
                }
                if now.duration_since(voice.started_at) >= STARTING_TIMEOUT {
                    diag::warn!(
                        Audio,
                        "audio: voice start timed out waiting for sink (typed gap)"
                    );
                    end_voice(&mut commands, entity, voice.owner);
                }
            }
            VoicePhase::Playing => {
                if voice.kind != VoiceKind::Oneshot {
                    continue;
                }
                if sink.is_some_and(|s| s.empty()) {
                    end_voice(&mut commands, entity, voice.owner);
                }
            }
        }
    }
}

fn cancel_stale_match_voices(
    epoch: Res<MatchEpoch>,
    voices: Query<(Entity, &Voice)>,
    mut commands: Commands,
) {
    for (entity, voice) in &voices {
        if voice.scope != AudioScope::Match {
            continue;
        }
        if voice.epoch == epoch.0 {
            continue;
        }
        end_voice(&mut commands, entity, voice.owner);
    }
}

fn end_voice(commands: &mut Commands, entity: Entity, owner: VoiceOwner) {
    match owner {
        VoiceOwner::Exclusive => {
            commands.entity(entity).try_despawn();
        }
        VoiceOwner::Attached => {
            detach_loop(commands, entity);
        }
    }
}
