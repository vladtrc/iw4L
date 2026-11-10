use crate::aliases::namespace_alias;
use crate::ambient::{MAP_BED_SLOT, MapSources};
use crate::cue::CueHandle;
use crate::cue_execution::CueTrigger;
use crate::playback::TrackedCuePlayback;
use crate::{SoundBank, SoundClass};
use bevy::prelude::*;

#[derive(Resource, Default)]
pub(crate) struct ScriptAmbientPlayback {
    target: Option<(bool, sim::ScriptAmbient)>,
    started: bool,
    voices: Vec<CueHandle>,
}

#[derive(bevy::ecs::system::SystemParam)]
pub(crate) struct ScriptAmbientPlayer<'w> {
    bank: Option<Res<'w, SoundBank>>,
    cues: TrackedCuePlayback<'w>,
    playback: ResMut<'w, ScriptAmbientPlayback>,
    map: Option<ResMut<'w, MapSources>>,
}

impl ScriptAmbientPlayer<'_> {
    fn retain_active(&mut self) {
        self.playback.voices.retain(CueHandle::active);
    }

    fn apply(
        &mut self,
        ac130: bool,
        plan: Option<&sim::ScriptAmbient>,
        clock: Option<&net::FrameClock>,
    ) {
        let now = self.cues.audio_frame();
        let Some(plan) = plan.filter(|plan| plan.valid()) else {
            if self.playback.target.take().is_some() {
                for handle in &self.playback.voices {
                    handle.release(now, 0);
                }
            }
            self.playback.started = false;
            return;
        };
        let Some(clock) = clock else { return };
        let frames = plan.end_ms.saturating_sub(clock.time()).max(0) as u64
            * u64::from(crate::render_core::SAMPLE_RATE)
            / 1000;
        if let Some(map) = self.map.as_mut() {
            map.desired.retain(|source| source.key.slot != MAP_BED_SLOT);
        }
        let selected = (ac130, plan.clone());
        if self.playback.target.as_ref() != Some(&selected) {
            let same_source = self.playback.target.as_ref().is_some_and(|(mode, old)| {
                *mode == ac130
                    && old
                        .alias
                        .as_ref()
                        .zip(plan.alias.as_ref())
                        .is_some_and(|(old, new)| old.eq_ignore_ascii_case(new))
            });
            let retained = same_source
                && self
                    .playback
                    .voices
                    .last()
                    .is_some_and(|handle| handle.fade_to(now, 1.0, frames));
            if !retained {
                for handle in &self.playback.voices {
                    handle.release(now, frames);
                }
            }
            self.playback.started = retained;
            self.playback.target = Some(selected);
        }
        let Some(alias) = plan.alias.as_ref().filter(|_| !self.playback.started) else {
            return;
        };
        let Some(bank) = self.bank.as_ref() else {
            return;
        };
        if self.playback.voices.len() == 2 {
            let oldest = self.playback.voices.remove(0);
            oldest.release(now, 0);
        }
        let (namespace, name) = namespace_alias(alias);
        let handle = self.cues.trigger_faded(
            CueTrigger {
                force_master: false,
                event: None,
                bank: bank.0.clone(),
                namespace,
                alias: name.into(),
                bound: None,
                origin_inches: None,
                emitter: None,
                class: SoundClass::Ambience,
                pitch_scale: (f32::from(plan.rate_percent) / 100.0).max(0.01),
                volume_scale: 1.0,
                fallbacks: Vec::new(),
            },
            frames,
        );
        self.playback.voices.push(handle);
        self.playback.started = true;
    }
}

pub(crate) fn update_script_ambient(
    ready: Res<crate::AudioReady>,
    loading: Option<Res<assets::LoadingScreen>>,
    presented: Option<Res<net::PresentedSnapshot>>,
    clock: Option<Res<net::FrameClock>>,
    local: Res<net::LocalPresentClient>,
    mut player: ScriptAmbientPlayer,
) {
    player.retain_active();
    if !ready.0.is_ready() || loading.is_some_and(|screen| !screen.is_complete()) {
        return;
    }
    let Some(presented) = presented else { return };
    let Some(snapshot) = presented.snapshot() else {
        return;
    };
    let ac130 = presented
        .player(local.0)
        .is_some_and(|ps| ps.other_flags & playerstate_iw4::other_flags::AC130 != 0);
    let plan = if ac130 {
        &snapshot.meta.objectives.ac130_ambient
    } else {
        &snapshot.meta.objectives.ambient
    };
    player.apply(ac130, plan.as_ref(), clock.as_deref());
}
