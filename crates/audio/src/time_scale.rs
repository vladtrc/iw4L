use asset_audio::ChannelKey;
use asset_core::FamilyId;
use bevy::prelude::*;
use frame::ScopeApp;
use std::{
    collections::HashMap,
    sync::{
        Arc,
        atomic::{AtomicU32, Ordering},
    },
};

#[derive(Resource)]
pub(crate) struct SoundTimeScale {
    factors: HashMap<ChannelKey, f32>,
    rates: Arc<HashMap<ChannelKey, Arc<AtomicU32>>>,
    pending: std::collections::VecDeque<sim::ScriptAudioCommand>,
}
impl Default for SoundTimeScale {
    fn default() -> Self {
        let factors: HashMap<_, _> = asset_audio::audio_families()
            .iter()
            .flat_map(|(namespace, _)| (0..64).map(move |id| (ChannelKey { namespace, id }, 1.0)))
            .collect();
        let rates = Arc::new(
            factors
                .keys()
                .map(|key| (*key, Arc::new(AtomicU32::new(1.0f32.to_bits()))))
                .collect(),
        );
        Self {
            factors,
            rates,
            pending: Default::default(),
        }
    }
}
impl SoundTimeScale {
    pub(crate) fn bindings(&self) -> Arc<HashMap<ChannelKey, Arc<AtomicU32>>> {
        self.rates.clone()
    }
}
fn update(
    mut state: ResMut<SoundTimeScale>,
    mut events: MessageReader<net::SvcScriptAudio>,
    bank: Option<Res<crate::SoundBank>>,
    time: Res<Time<Virtual>>,
) {
    for event in events.read() {
        if matches!(
            event.0,
            sim::ScriptAudioCommand::TimeScaleFactor { .. }
                | sim::ScriptAudioCommand::ResetTimeScale
        ) {
            state.pending.push_back(event.0.clone());
        }
    }
    if let Some(bank) = bank {
        while let Some(command) = state.pending.pop_front() {
            match command {
                sim::ScriptAudioCommand::TimeScaleFactor { channel, factor } => {
                    let channel = bank.0.channels_in(FamilyId::IW4).and_then(|channels| {
                        channels
                            .iter()
                            .position(|row| row.name.eq_ignore_ascii_case(&channel))
                    });
                    if let Some(id) = channel {
                        state.factors.insert(
                            ChannelKey {
                                namespace: FamilyId::IW4,
                                id: id as u32,
                            },
                            factor,
                        );
                    } else {
                        diag::warn!(Audio, "sound timescale: unknown IW4 channel");
                    }
                }
                sim::ScriptAudioCommand::ResetTimeScale => {
                    state.factors.values_mut().for_each(|factor| *factor = 1.0)
                }
                _ => unreachable!(),
            }
        }
    }
    let speed = time.relative_speed();
    let speed = if speed > 0.0 { speed } else { 1.0 };
    for (key, factor) in &state.factors {
        state.rates[key].store((1.0 + factor * (speed - 1.0)).to_bits(), Ordering::Relaxed);
    }
}
pub(crate) fn register(app: &mut App) {
    app.scoped::<SoundTimeScale>(frame::Timeline::Live)
        .add_systems(
            Update,
            update
                .in_set(frame::InMatch)
                .in_set(net::ClientSet::Effects),
        );
}
