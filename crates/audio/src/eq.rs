use bevy::prelude::*;
use frame::ScopeApp;
use std::sync::{
    Mutex,
    atomic::{AtomicU64, Ordering},
};

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Coefficients([f32; 5]);
impl Default for Coefficients {
    fn default() -> Self {
        Self([1.0, 0.0, 0.0, 0.0, 0.0])
    }
}

impl Coefficients {
    fn compile(band: sim::ScriptEqBand) -> Option<Self> {
        if !band.valid() {
            return None;
        }
        let phase = band.frequency * f32::from_bits(0x3909421f);
        let c = f64::from(phase).cos() as f32;
        let alpha = (f64::from(phase).sin() as f32) / (2.0 * band.q);
        let a = 10.0_f64.powf(f64::from(band.gain_db * 0.025)) as f32;
        let beta = 2.0 * a.sqrt() * alpha;
        let (b, denominator) = match band.kind {
            0 => (
                [(1.0 - c) * 0.5, 1.0 - c, (1.0 - c) * 0.5],
                [1.0 + alpha, -2.0 * c, 1.0 - alpha],
            ),
            1 => (
                [(1.0 + c) * 0.5, -1.0 - c, (1.0 + c) * 0.5],
                [1.0 + alpha, -2.0 * c, 1.0 - alpha],
            ),
            2 => (
                [
                    a * ((a + 1.0) - (a - 1.0) * c + beta),
                    2.0 * a * ((a - 1.0) - (a + 1.0) * c),
                    a * ((a + 1.0) - (a - 1.0) * c - beta),
                ],
                [
                    (a + 1.0) + (a - 1.0) * c + beta,
                    -2.0 * ((a - 1.0) + (a + 1.0) * c),
                    (a + 1.0) + (a - 1.0) * c - beta,
                ],
            ),
            3 => (
                [
                    a * ((a + 1.0) + (a - 1.0) * c + beta),
                    -2.0 * a * ((a - 1.0) + (a + 1.0) * c),
                    a * ((a + 1.0) + (a - 1.0) * c - beta),
                ],
                [
                    (a + 1.0) - (a - 1.0) * c + beta,
                    2.0 * ((a - 1.0) - (a + 1.0) * c),
                    (a + 1.0) - (a - 1.0) * c - beta,
                ],
            ),
            4 => (
                [1.0 + alpha * a, -2.0 * c, 1.0 - alpha * a],
                [1.0 + alpha / a, -2.0 * c, 1.0 - alpha / a],
            ),
            _ => return None,
        };
        let inverse = 1.0 / denominator[0];
        let values = [
            b[0] * inverse,
            b[1] * inverse,
            b[2] * inverse,
            denominator[1] * inverse,
            denominator[2] * inverse,
        ];
        values.iter().all(|v| v.is_finite()).then_some(Self(values))
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
struct Band {
    revision: u64,
    enabled: bool,
    coefficients: Coefficients,
}
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub(crate) struct ChannelEq {
    bands: [[Band; 3]; 2],
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct EqTarget {
    pub epoch: u64,
    pub enabled: bool,
    pub fraction: f32,
    pub entity_bypass: [u64; 32],
    pub channels: [ChannelEq; 64],
}
impl Default for EqTarget {
    fn default() -> Self {
        Self {
            epoch: 0,
            enabled: true,
            fraction: 0.0,
            entity_bypass: [0; 32],
            channels: [ChannelEq::default(); 64],
        }
    }
}

impl EqTarget {
    fn set_entity_bypass(&mut self, entities: &[u16]) {
        self.entity_bypass = [0; 32];
        for &number in entities {
            if number < 2048 {
                self.entity_bypass[usize::from(number) / 64] |= 1 << (number % 64);
            }
        }
    }

    pub(crate) fn bypasses(&self, entity: Option<u32>) -> bool {
        entity
            .filter(|number| *number < 2048)
            .is_some_and(|number| {
                self.entity_bypass[number as usize / 64] & (1 << (number % 64)) != 0
            })
    }
}

pub(crate) struct EqControl {
    writer: Mutex<()>,
    sequence: AtomicU64,
    epoch: AtomicU64,
    header: [AtomicU64; 2],
    channels: [[AtomicU64; 25]; 64],
    entity_bypass: [AtomicU64; 32],
}
impl Default for EqControl {
    fn default() -> Self {
        let control = Self {
            writer: Mutex::new(()),
            sequence: AtomicU64::new(0),
            epoch: AtomicU64::new(0),
            header: std::array::from_fn(|_| AtomicU64::new(0)),
            channels: std::array::from_fn(|_| std::array::from_fn(|_| AtomicU64::new(0))),
            entity_bypass: std::array::from_fn(|_| AtomicU64::new(0)),
        };
        control.publish(&EqTarget::default());
        control
    }
}
impl EqControl {
    fn write(&self, target: &EqTarget) {
        self.sequence.fetch_add(1, Ordering::SeqCst);
        self.header[0].store(self.epoch.load(Ordering::Relaxed), Ordering::SeqCst);
        self.header[1].store(
            u64::from(target.fraction.to_bits()) | (u64::from(target.enabled) << 32),
            Ordering::SeqCst,
        );
        for (output, bypass) in self.entity_bypass.iter().zip(target.entity_bypass) {
            output.store(bypass, Ordering::SeqCst);
        }
        for (output, channel) in self.channels.iter().zip(target.channels) {
            let mut active = 0;
            for (i, band) in channel.bands.into_iter().flatten().enumerate() {
                active |= u64::from(band.enabled) << i;
                let c = band.coefficients.0;
                for (j, (low, high)) in [(c[0], c[1]), (c[2], c[3]), (c[4], 0.0)]
                    .into_iter()
                    .enumerate()
                {
                    output[1 + i * 4 + j].store(
                        u64::from(low.to_bits()) | (u64::from(high.to_bits()) << 32),
                        Ordering::SeqCst,
                    );
                }
                output[1 + i * 4 + 3].store(band.revision, Ordering::SeqCst);
            }
            output[0].store(active, Ordering::SeqCst);
        }
        self.sequence.fetch_add(1, Ordering::SeqCst);
    }
    pub(crate) fn publish(&self, target: &EqTarget) {
        let _writer = self.writer.lock().unwrap_or_else(|e| e.into_inner());
        self.write(target);
    }
    pub(crate) fn reset(&self) {
        let _writer = self.writer.lock().unwrap_or_else(|e| e.into_inner());
        self.epoch.fetch_add(1, Ordering::Relaxed);
        self.write(&EqTarget::default());
    }
    pub(crate) fn read(&self, revision: &mut u64, cached: &mut EqTarget) {
        let sequence = self.sequence.load(Ordering::SeqCst);
        if sequence == *revision || sequence & 1 != 0 {
            return;
        }
        let mut next = *cached;
        next.epoch = self.header[0].load(Ordering::SeqCst);
        let flags = self.header[1].load(Ordering::SeqCst);
        next.enabled = flags >> 32 != 0;
        next.fraction = f32::from_bits(flags as u32);
        for (output, input) in next.entity_bypass.iter_mut().zip(&self.entity_bypass) {
            *output = input.load(Ordering::SeqCst);
        }
        for (input, channel) in self.channels.iter().zip(&mut next.channels) {
            let active = input[0].load(Ordering::SeqCst);
            for (i, band) in channel.bands.iter_mut().flatten().enumerate() {
                band.enabled = active & (1 << i) != 0;
                band.revision = input[1 + i * 4 + 3].load(Ordering::SeqCst);
                let packed: [u64; 3] =
                    std::array::from_fn(|j| input[1 + i * 4 + j].load(Ordering::SeqCst));
                band.coefficients = Coefficients([
                    f32::from_bits(packed[0] as u32),
                    f32::from_bits((packed[0] >> 32) as u32),
                    f32::from_bits(packed[1] as u32),
                    f32::from_bits((packed[1] >> 32) as u32),
                    f32::from_bits(packed[2] as u32),
                ]);
            }
        }
        if self.sequence.load(Ordering::SeqCst) == sequence {
            *cached = next;
            *revision = sequence;
        }
    }
}

#[derive(Clone, Copy, Default)]
struct History {
    revision: u64,
    coefficients: Coefficients,
    samples: [[f32; 4]; 2],
}
#[derive(Clone, Copy, Default)]
pub(crate) struct VoiceEq {
    history: [[History; 3]; 2],
}
impl VoiceEq {
    pub(crate) fn process(
        &mut self,
        channel: ChannelEq,
        fraction: f32,
        input: [f32; 2],
    ) -> [f32; 2] {
        for track in 0..2 {
            for band in 0..3 {
                let coefficients = channel.bands[track][band].coefficients;
                if self.history[track][band].coefficients != coefficients
                    || self.history[track][band].revision != channel.bands[track][band].revision
                {
                    self.history[track][band] = History {
                        revision: channel.bands[track][band].revision,
                        coefficients,
                        ..Default::default()
                    };
                }
            }
        }
        if !channel.bands.iter().flatten().any(|band| band.enabled) {
            return input;
        }
        let mut output = [0.0; 2];
        for track in 0..2 {
            let weight = if track == 0 { fraction } else { 1.0 - fraction };
            if weight < 0.001 {
                continue;
            }
            let mut sample = input.map(|v| v * weight);
            for band in 0..3 {
                let filter = channel.bands[track][band];
                let history = &mut self.history[track][band];
                if !filter.enabled {
                    continue;
                }
                let [b0, b1, b2, a1, a2] = filter.coefficients.0;
                for side in 0..2 {
                    let [x1, x2, y1, y2] = history.samples[side];
                    let result = b0 * sample[side] + b1 * x1 + b2 * x2 - a1 * y1 - a2 * y2;
                    history.samples[side] = [sample[side], x1, result, y1];
                    sample[side] = result;
                }
            }
            for side in 0..2 {
                output[side] += sample[side];
            }
        }
        output
    }
}

#[derive(Resource, Default)]
struct ScriptEq {
    target: EqTarget,
    pending: std::collections::VecDeque<sim::ScriptAudioCommand>,
}
impl ScriptEq {
    fn apply(&mut self, command: sim::ScriptAudioCommand, channels: &[asset_audio::EntChannel]) {
        let find = |name: &str| {
            channels
                .iter()
                .position(|c| c.name.eq_ignore_ascii_case(name))
                .filter(|i| *i < 64)
        };
        match command {
            sim::ScriptAudioCommand::EqBands {
                channel,
                track,
                filters,
                ..
            } => {
                let Some(index) = find(&channel) else {
                    diag::warn!(Audio, "audio: unknown EQ channel `{channel}`");
                    return;
                };
                let Some(coefficients) = filters
                    .into_iter()
                    .map(Coefficients::compile)
                    .collect::<Option<Vec<_>>>()
                else {
                    diag::warn!(Audio, "audio: EQ parameters produce nonfinite coefficients");
                    return;
                };
                let bands = &mut self.target.channels[index].bands[usize::from(track)];
                for i in 0..3 {
                    if let Some(coefficients) = coefficients.get(i) {
                        bands[i] = Band {
                            revision: bands[i]
                                .revision
                                .wrapping_add(u64::from(bands[i].coefficients != *coefficients)),
                            coefficients: *coefficients,
                            enabled: true,
                        };
                    } else {
                        bands[i].enabled = false;
                    }
                }
            }
            sim::ScriptAudioCommand::Eq {
                channel,
                track,
                band,
                filter,
                ..
            } => {
                let Some(channel_index) = find(&channel) else {
                    diag::warn!(Audio, "audio: unknown EQ channel `{channel}`");
                    return;
                };
                let Some(coefficients) = Coefficients::compile(filter) else {
                    diag::warn!(Audio, "audio: EQ parameters produce nonfinite coefficients");
                    return;
                };
                let selected = &mut self.target.channels[channel_index].bands[usize::from(track)]
                    [usize::from(band)];
                *selected = Band {
                    revision: selected
                        .revision
                        .wrapping_add(u64::from(selected.coefficients != coefficients)),
                    coefficients,
                    enabled: true,
                };
            }
            sim::ScriptAudioCommand::DeactivateEq {
                track,
                channel,
                band,
                ..
            } => {
                let range = match channel {
                    Some(name) => match find(&name) {
                        Some(i) => i..i + 1,
                        None => {
                            diag::warn!(Audio, "audio: unknown EQ channel `{name}`");
                            return;
                        }
                    },
                    None => 0..64,
                };
                for channel in range {
                    for (i, filter) in self.target.channels[channel].bands[usize::from(track)]
                        .iter_mut()
                        .enumerate()
                    {
                        if band.is_none_or(|band| usize::from(band) == i) {
                            filter.enabled = false;
                        }
                    }
                }
            }
            sim::ScriptAudioCommand::EqLerp { fraction, .. } => self.target.fraction = fraction,
            _ => {}
        }
    }
}

fn update_eq(
    mut mix: ResMut<ScriptEq>,
    mut events: MessageReader<net::SvcScriptAudio>,
    local: Option<Res<net::LocalPresentClient>>,
    bank: Option<Res<crate::SoundBank>>,
    namespace: Option<Res<crate::ambient::SoundBankNamespace>>,
    runtime: Res<crate::AudioRuntime>,
    presented: Option<Res<net::PresentedSnapshot>>,
    dvars: Option<Res<frame::UiMenuDvars>>,
) {
    for event in events.read() {
        if matches!(
            event.0,
            sim::ScriptAudioCommand::EqBands { .. }
                | sim::ScriptAudioCommand::Eq { .. }
                | sim::ScriptAudioCommand::DeactivateEq { .. }
                | sim::ScriptAudioCommand::EqLerp { .. }
        ) {
            mix.pending.push_back(event.0.clone());
        }
    }
    let Some(local) = local else { return };
    let setting = presented
        .as_ref()
        .and_then(|view| view.snapshot())
        .and_then(|snapshot| snapshot.meta.script_dvars(local.0).string("snd_enableEq"))
        .or_else(|| dvars.as_ref().and_then(|dvars| dvars.get("snd_enableEq")));
    let old = mix.target;
    mix.target.set_entity_bypass(
        presented
            .as_ref()
            .and_then(|view| view.snapshot())
            .map_or(&[], |snapshot| {
                snapshot.meta.eq_disabled_entities.as_slice()
            }),
    );
    mix.target.enabled = setting
        .and_then(|v| v.parse::<f32>().ok())
        .filter(|v| v.is_finite())
        .is_none_or(|v| v != 0.0);
    while let Some(command) = mix.pending.front() {
        if command.target() != Some(local.0) || !command.valid() {
            mix.pending.pop_front();
            continue;
        }
        let Some((bank, namespace)) = bank.as_ref().zip(namespace.as_ref()) else {
            break;
        };
        let Some(channels) = bank.0.channels_in(namespace.namespace) else {
            break;
        };
        let command = mix.pending.pop_front().expect("front command");
        mix.apply(command, channels);
    }
    if mix.target != old {
        runtime.set_eq(&mix.target);
    }
}

pub(crate) fn register(app: &mut App) {
    app.scoped::<ScriptEq>(frame::MatchScope::Live).add_systems(
        Update,
        update_eq
            .in_set(frame::InMatch)
            .in_set(net::ClientSet::Effects),
    );
}
