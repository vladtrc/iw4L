use frame::ScopeApp;
use std::sync::Arc;

use crate::runtime::AudioRuntime;
use asset_audio::SoundCatalog;
use asset_core::FamilyId;
use bevy::prelude::*;
use frame::{ClientSet, FxSoundPublished};
use net::{LastAdoptedSnapshot, SvcLocalSound};

use crate::clip_store::CueFeedback;
use crate::cue::CueHandle;
use crate::cue_execution::CueTrigger;
use crate::messages::{
    AliasCommand, BoundWeaponSound, Footstep, LandSound, PlayAlias, SND_ENT_LOCAL, SurfaceSound,
    ViewmodelNotetracks, WeaponSound,
};
use crate::start::{SoundClass, StartDecision, StartDecisions, StartFailure, StartOutcome};

#[derive(Component, Default)]
pub struct AmbientListener;

#[derive(Resource, Clone)]
pub struct SoundBank(pub Arc<SoundCatalog>);

#[derive(bevy::ecs::system::SystemParam)]
pub(crate) struct TrackedCuePlayback<'w> {
    runtime: Res<'w, AudioRuntime>,
    feedback: ResMut<'w, CueFeedback>,
}

impl TrackedCuePlayback<'_> {
    pub(crate) fn audio_frame(&self) -> u64 {
        self.runtime.audio_frame()
    }

    pub(crate) fn trigger(&mut self, request: CueTrigger) -> CueHandle {
        let handle = self.runtime.trigger_cue(request);
        self.track(handle)
    }

    pub(crate) fn trigger_faded(&mut self, request: CueTrigger, frames: u64) -> CueHandle {
        let handle = self.runtime.trigger_faded_cue(request, frames);
        self.track(handle)
    }

    fn track(&mut self, handle: CueHandle) -> CueHandle {
        self.feedback.push(CueHandle(handle.0.clone()));
        handle
    }
}

#[derive(Resource, Default, Debug)]
pub struct MissingAliasGaps {
    pub aliases: Vec<String>,
}

impl MissingAliasGaps {
    pub fn record(&mut self, alias: &str) {
        if !self.aliases.iter().any(|a| a == alias) {
            diag::warn!(Audio, "audio: missing alias `{alias}` (typed gap)");
            self.aliases.push(alias.to_owned());
        }
    }

    pub fn len(&self) -> usize {
        self.aliases.len()
    }

    pub fn is_empty(&self) -> bool {
        self.aliases.is_empty()
    }
}

pub(crate) struct PlayerSoundPlugin;

impl Plugin for PlayerSoundPlugin {
    fn build(&self, app: &mut App) {
        app.add_observer(crate::projectile_loops::stop)
            .add_observer(crate::flame_loops::stop);
        app.scoped::<crate::LoadingAudio>(frame::MatchScope::Loading)
            .init_resource::<crate::clip_store::ResidentClipCache>()
            .init_resource::<MissingAliasGaps>()
            .init_resource::<StartDecisions>()
            .scoped::<CueFeedback>(frame::Timeline::Live)
            .scoped::<crate::ambient::MapAmbientBooted>(frame::Timeline::Live)
            .scoped::<crate::ambient::MapSources>(frame::Timeline::Live)
            .scoped::<crate::script_ambient::ScriptAmbientPlayback>(frame::Timeline::Live)
            .scoped::<crate::destructible_loops::DestructibleSources>(frame::Timeline::Live)
            .scoped_message::<crate::FlameFire>(frame::Timeline::Live)
            .scoped::<crate::ambient::ResidentSoundBank>(frame::MatchScope::Loading)
            .scoped::<crate::shellshock::ShellshockSources>(frame::Timeline::Live)
            .scoped::<crate::breath::BreathSources>(frame::Timeline::Live)
            .scoped::<crate::breath::BreathAudio>(frame::Timeline::Live)
            .scoped_message::<AliasCommand>(frame::Timeline::Live)
            .scoped_message::<Footstep>(frame::Timeline::Live)
            .scoped_message::<WeaponSound>(frame::Timeline::Live)
            .scoped_message::<BoundWeaponSound>(frame::Timeline::Live)
            .scoped_message::<ViewmodelNotetracks>(frame::Timeline::Live)
            .scoped::<crate::entity_events::NotetrackSoundTable>(frame::MatchScope::Live)
            .scoped_message::<LandSound>(frame::Timeline::Live)
            .add_systems(
                Update,
                play_weapon_sound_messages
                    .in_set(frame::InMatch)
                    .in_set(ClientSet::Predict)
                    .after(frame::OwnerEventsPublished)
                    .after(crate::backend::publish_audio_context),
            )
            .add_systems(
                Update,
                (
                    crate::ambient::boot_map_ambient_once.in_set(frame::InMatch),
                    crate::script_ambient::update_script_ambient
                        .in_set(frame::InMatch)
                        .after(crate::ambient::boot_map_ambient_once),
                    collect_cue_decisions
                        .in_set(frame::InMatch)
                        .after(play_alias_messages)
                        .before(play_footstep_messages)
                        .before(play_land_sound_messages),
                    apply_svc_local_sound
                        .in_set(frame::InMatch)
                        .before(play_alias_messages)
                        .run_if(resource_exists::<LastAdoptedSnapshot>),
                    crate::shellshock::update_shellshock_tinnitus
                        .in_set(frame::InMatch)
                        .before(play_alias_messages),
                    crate::breath::update
                        .in_set(frame::InMatch)
                        .before(play_alias_messages),
                    play_alias_messages
                        .in_set(frame::InMatch)
                        .after(FxSoundPublished),
                    play_footstep_messages.in_set(frame::InMatch),
                    crate::entity_events::play_viewmodel_notetrack_messages
                        .in_set(frame::InMatch)
                        .before(play_bound_weapon_sounds),
                    play_bound_weapon_sounds
                        .in_set(frame::InMatch)
                        .after(collect_cue_decisions),
                    play_land_sound_messages.in_set(frame::InMatch),
                    crate::destructible_loops::update.in_set(frame::InMatch),
                    crate::projectile_loops::update.in_set(frame::InMatch),
                    crate::flame_loops::update.in_set(frame::InMatch),
                )
                    .in_set(ClientSet::Effects),
            )
            .add_systems(
                PreUpdate,
                (
                    crate::ambient::start_sound_bank_compose
                        .run_if(in_state(frame::MatchScope::Loading)),
                    crate::ambient::install_sound_bank
                        .run_if(in_state(frame::MatchScope::Loading))
                        .after(crate::ambient::start_sound_bank_compose),
                    crate::entity_events::bind_notetrack_sounds
                        .in_set(frame::InMatch)
                        .after(crate::ambient::install_sound_bank),
                )
                    .in_set(ClientSet::Load),
            );
    }
}

fn apply_svc_local_sound(
    mut cmds: MessageReader<SvcLocalSound>,
    adopted: Res<LastAdoptedSnapshot>,
    announcer: Res<crate::match_voices::AnnouncerRoutes>,
    mut play: MessageWriter<crate::AliasCommand>,
) {
    for cmd in cmds.read() {
        let Some((namespace, alias, music)) = adopted
            .sound_alias_name(cmd.index)
            .map(|alias| (announcer.route(alias), announcer.is_music(alias)))
            .map(|((namespace, alias), music)| (namespace, alias.to_owned(), music))
        else {
            diag::warn!(
                Audio,
                "audio: svc local sound CS index {} is unresolved (typed gap)",
                cmd.index
            );
            continue;
        };
        if crate::diagnostics::enabled() {
            crate::diagnostics::emit(format!(
                "audio diag: authority_local_sound alias={alias} stop={} index={} server_tick={:?}",
                cmd.stop,
                cmd.index,
                adopted.next().map(|snapshot| snapshot.tick.0)
            ));
        }
        if cmd.stop {
            play.write(AliasCommand::Stop {
                namespace,
                alias,
                snd_ent: Some(SND_ENT_LOCAL),
            });
        } else {
            let sound = PlayAlias {
                event: None,
                namespace,
                alias,
                fallback: None,
                origin_inches: None,
                snd_ent: Some(SND_ENT_LOCAL),
            };
            play.write(if music {
                AliasCommand::PlayMusic(sound)
            } else {
                AliasCommand::Play(sound)
            });
        }
    }
}

fn sound_entity(number: Option<u32>, local: sim::ClientId) -> Option<u32> {
    number.map(|number| {
        if number == SND_ENT_LOCAL {
            local.0
        } else {
            number
        }
    })
}

fn play_alias_messages(
    mut events: MessageReader<AliasCommand>,
    runtime: Res<AudioRuntime>,
    mut pending: ResMut<CueFeedback>,
    mut decisions: ResMut<StartDecisions>,
    bank: Option<Res<SoundBank>>,
    local: Res<net::LocalPresentClient>,
) {
    for command in events.read() {
        let (event, pitch_scale, class) = match command {
            AliasCommand::StopEntity { snd_ent } => {
                runtime.stop_emitter_cues(*snd_ent);
                continue;
            }
            AliasCommand::Play(event) => (event, 1.0, SoundClass::World),
            AliasCommand::PlayMusic(event) => (event, 1.0, SoundClass::Music),
            AliasCommand::PlayPitched { sound, pitch } => (sound, *pitch, SoundClass::World),
            AliasCommand::Stop {
                namespace,
                alias,
                snd_ent,
            } => {
                let snd_ent = sound_entity(*snd_ent, local.0);
                runtime.stop_cue(*namespace, alias, snd_ent);
                continue;
            }
        };
        let Some(bank) = bank.as_ref() else {
            drop_without_bank(
                std::iter::once((event.alias.as_str(), event.event)),
                &mut decisions,
            );
            continue;
        };
        play_oneshot_recorded(
            &runtime,
            &mut pending,
            &mut decisions,
            crate::cue_execution::CueTrigger {
                event: event.event,
                bank: Arc::clone(&bank.0),
                namespace: event.namespace,
                alias: event.alias.clone(),
                bound: None,
                origin_inches: event.origin_inches,
                emitter: sound_entity(event.snd_ent, local.0),
                class,
                pitch_scale,
                volume_scale: 1.0,
                fallbacks: event.fallback.iter().cloned().collect(),
            },
        );
    }
}

fn play_footstep_messages(
    mut events: MessageReader<Footstep>,
    runtime: Res<AudioRuntime>,
    mut gaps: ResMut<MissingAliasGaps>,
    mut pending: ResMut<CueFeedback>,
    mut decisions: ResMut<StartDecisions>,
    bank: Option<Res<SoundBank>>,
) {
    let Some(bank) = bank else {
        drop_without_bank(
            events.read().map(|e| (e.sound.alias, e.sound.event)),
            &mut decisions,
        );
        return;
    };
    for message in events.read() {
        let event = &message.sound;
        let outcome =
            play_surface_alias_chain(&bank.0, event, &runtime, &mut pending, &mut decisions);
        if outcome.allows_binding_fallback() {
            gaps.record(event.alias);
            gaps.record(event.fallback);
        }
    }
}

fn play_weapon_sound_messages(
    mut events: MessageReader<WeaponSound>,
    runtime: Res<AudioRuntime>,
    mut gaps: ResMut<MissingAliasGaps>,
    mut pending: ResMut<CueFeedback>,
    mut decisions: ResMut<StartDecisions>,
    bank: Option<Res<SoundBank>>,
) {
    let Some(bank) = bank else {
        drop_without_bank(
            events.read().map(|e| (e.alias.as_str(), e.event)),
            &mut decisions,
        );
        return;
    };
    for event in events.read() {
        let outcome = play_oneshot_recorded(
            &runtime,
            &mut pending,
            &mut decisions,
            crate::cue_execution::CueTrigger {
                event: event.event,
                bank: Arc::clone(&bank.0),
                namespace: event.namespace,
                alias: event.alias.clone(),
                bound: None,
                origin_inches: event.origin_inches,
                emitter: event.snd_ent,
                class: SoundClass::Weapon,
                pitch_scale: 1.0,
                volume_scale: 1.0,
                fallbacks: Vec::new(),
            },
        );
        if outcome.allows_binding_fallback() {
            gaps.record(&event.alias);
        }
    }
}

fn play_bound_weapon_sounds(
    mut events: MessageReader<BoundWeaponSound>,
    runtime: Res<AudioRuntime>,
    mut gaps: ResMut<MissingAliasGaps>,
    mut pending: ResMut<CueFeedback>,
    mut decisions: ResMut<StartDecisions>,
    bank: Option<Res<SoundBank>>,
    local: Res<net::LocalPresentClient>,
) {
    let Some(bank) = bank else {
        for _ in events.read() {}
        return;
    };
    for event in events.read() {
        if event.bank_revision != bank.0.revision() {
            continue;
        }
        let Some(alias) = bank.0.name_at(event.index) else {
            continue;
        };
        let Some(namespace) = bank.0.namespace_of_alias(event.index) else {
            continue;
        };
        let outcome = play_oneshot_recorded(
            &runtime,
            &mut pending,
            &mut decisions,
            crate::cue_execution::CueTrigger {
                event: event.event,
                bank: Arc::clone(&bank.0),
                namespace,
                alias: alias.to_owned(),
                bound: Some(event.index),
                origin_inches: event.origin_inches,
                emitter: sound_entity(event.snd_ent, local.0),
                class: SoundClass::Weapon,
                pitch_scale: 1.0,
                volume_scale: 1.0,
                fallbacks: Vec::new(),
            },
        );
        if outcome.allows_binding_fallback() {
            gaps.record(alias);
        }
    }
}

fn play_land_sound_messages(
    mut events: MessageReader<LandSound>,
    runtime: Res<AudioRuntime>,
    mut gaps: ResMut<MissingAliasGaps>,
    mut pending: ResMut<CueFeedback>,
    mut decisions: ResMut<StartDecisions>,
    bank: Option<Res<SoundBank>>,
) {
    let Some(bank) = bank else {
        drop_without_bank(
            events.read().map(|e| (e.sound.alias, e.sound.event)),
            &mut decisions,
        );
        return;
    };
    for message in events.read() {
        let event = &message.sound;
        let outcome =
            play_surface_alias_chain(&bank.0, event, &runtime, &mut pending, &mut decisions);
        if outcome.allows_binding_fallback() {
            gaps.record(event.alias);
            gaps.record(event.fallback);
        }
    }
}

fn drop_without_bank<'a>(
    aliases: impl Iterator<Item = (&'a str, Option<crate::AudioEvent>)>,
    decisions: &mut StartDecisions,
) {
    for (alias, event) in aliases {
        if !crate::AudioSilent::active() {
            diag::warn!(
                Audio,
                "audio: alias `{alias}` dropped — no SoundBank (typed gap)"
            );
        }
        decisions.record(StartDecision {
            event,
            namespace: FamilyId::IW4,
            alias: alias.to_owned(),
            variant: None,
            loaded_binding_origin: None,
            outcome: StartOutcome::Failed(StartFailure::BankMissing),
            secondary: None,
            detail: None,
        });
    }
}

fn play_surface_alias_chain(
    bank: &Arc<SoundCatalog>,
    sound: &SurfaceSound,
    runtime: &AudioRuntime,
    pending: &mut CueFeedback,
    decisions: &mut StartDecisions,
) -> StartOutcome {
    let candidates = crate::aliases::surface_alias_candidates(sound.alias);
    let Some(first) = candidates.first() else {
        return StartOutcome::Failed(StartFailure::MissingAlias);
    };
    play_oneshot_recorded(
        runtime,
        pending,
        decisions,
        crate::cue_execution::CueTrigger {
            event: sound.event,
            bank: Arc::clone(bank),
            namespace: FamilyId::IW4,
            alias: (*first).to_owned(),
            bound: None,
            origin_inches: sound.origin_inches,
            emitter: sound.snd_ent,
            class: SoundClass::World,
            pitch_scale: 1.0,
            volume_scale: sound.volume_scale,
            fallbacks: candidates[1..]
                .iter()
                .map(|name| (*name).to_owned())
                .collect(),
        },
    )
}

pub fn world_oneshot_pan(ear: Vec3, listener_right: Vec3, emitter: Vec3) -> f32 {
    crate::spatial::pan(
        ear.to_array(),
        listener_right.to_array(),
        emitter.to_array(),
    )
}

pub fn world_oneshot_channel_gains(
    ear: Vec3,
    listener_right: Vec3,
    emitter: Vec3,
    atten: f32,
) -> (f32, f32) {
    crate::spatial::channel_gains(
        ear.to_array(),
        listener_right.to_array(),
        emitter.to_array(),
        atten,
    )
}

fn collect_cue_decisions(
    mut pending: ResMut<CueFeedback>,
    mut decisions: ResMut<StartDecisions>,
    mut gaps: ResMut<MissingAliasGaps>,
) {
    pending.cues.retain(|entry| {
        let Some(decision) = entry.handle.completion() else {
            return true;
        };
        if decision.outcome.allows_binding_fallback() {
            gaps.record(&decision.alias);
        }
        decisions.record(decision);
        false
    });
}

pub(crate) fn play_oneshot_recorded(
    runtime: &AudioRuntime,
    pending: &mut CueFeedback,
    decisions: &mut StartDecisions,
    trigger: crate::cue_execution::CueTrigger,
) -> StartOutcome {
    let event = trigger.event;
    let namespace = trigger.namespace;
    let alias = trigger.alias.clone();
    let handle = runtime.trigger_cue(trigger);
    let outcome = StartOutcome::Pending;
    pending.push(handle);
    decisions.record(StartDecision {
        event,
        namespace,
        alias,
        variant: None,
        loaded_binding_origin: None,
        outcome: outcome.clone(),
        secondary: None,
        detail: None,
    });
    outcome
}
