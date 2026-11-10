use std::sync::{
    Mutex,
    atomic::{AtomicU64, Ordering},
};

use crate::render_core::{QUANTUM, SAMPLE_RATE};
use bevy::prelude::*;
use frame::ScopeApp;

#[derive(Clone, Copy)]
pub(crate) struct RoomVoice {
    full_dry: bool,
    send: bool,
}

impl RoomVoice {
    pub(crate) fn from_flags(flags: u32) -> Self {
        Self {
            full_dry: flags & 8 != 0,
            send: flags & 16 == 0,
        }
    }

    pub(crate) fn gains(self, levels: [f32; 2], enabled: bool) -> [f32; 2] {
        [
            if self.full_dry { 1.0 } else { levels[0] },
            if self.send && enabled { levels[1] } else { 0.0 },
        ]
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct RoomTarget {
    pub epoch: u64,
    pub room: u8,
    pub enabled: bool,
    pub from: [f32; 2],
    pub to: [f32; 2],
    pub start: u64,
    pub end: u64,
}

impl Default for RoomTarget {
    fn default() -> Self {
        Self {
            epoch: 0,
            room: 0,
            enabled: true,
            from: [1.0, 0.0],
            to: [1.0, 0.0],
            start: 0,
            end: 0,
        }
    }
}

impl RoomTarget {
    pub(crate) fn levels(self, frame: u64) -> [f32; 2] {
        let t = if frame >= self.end || self.end <= self.start {
            1.0
        } else {
            frame.saturating_sub(self.start) as f64 / (self.end - self.start) as f64
        };
        std::array::from_fn(|i| {
            (f64::from(self.from[i]) + (f64::from(self.to[i]) - f64::from(self.from[i])) * t) as f32
        })
    }
}

/// Control writes can lock; the render callback makes only bounded atomic reads.
pub(crate) struct RoomControl {
    writer: Mutex<()>,
    sequence: AtomicU64,
    epoch: AtomicU64,
    words: [AtomicU64; 6],
}

impl Default for RoomControl {
    fn default() -> Self {
        let control = Self {
            writer: Mutex::new(()),
            sequence: AtomicU64::new(0),
            epoch: AtomicU64::new(0),
            words: std::array::from_fn(|_| AtomicU64::new(0)),
        };
        control.publish(RoomTarget::default());
        control
    }
}

fn pair(values: [f32; 2]) -> u64 {
    u64::from(values[0].to_bits()) | (u64::from(values[1].to_bits()) << 32)
}
fn unpair(bits: u64) -> [f32; 2] {
    [
        f32::from_bits(bits as u32),
        f32::from_bits((bits >> 32) as u32),
    ]
}

impl RoomControl {
    fn write(&self, mut target: RoomTarget) {
        target.epoch = self.epoch.load(Ordering::Relaxed);
        self.sequence.fetch_add(1, Ordering::SeqCst);
        let words = [
            target.epoch,
            u64::from(target.room) | (u64::from(target.enabled) << 8),
            pair(target.from),
            pair(target.to),
            target.start,
            target.end,
        ];
        for (word, value) in self.words.iter().zip(words) {
            word.store(value, Ordering::SeqCst);
        }
        self.sequence.fetch_add(1, Ordering::SeqCst);
    }
    pub(crate) fn publish(&self, target: RoomTarget) {
        let _writer = self.writer.lock().unwrap_or_else(|e| e.into_inner());
        self.write(target);
    }
    pub(crate) fn reset(&self) {
        let _writer = self.writer.lock().unwrap_or_else(|e| e.into_inner());
        self.epoch.fetch_add(1, Ordering::Relaxed);
        self.write(RoomTarget::default());
    }
    pub(crate) fn read(&self, cached: &mut RoomTarget) {
        for _ in 0..2 {
            let sequence = self.sequence.load(Ordering::SeqCst);
            if sequence & 1 != 0 {
                continue;
            }
            let words = self
                .words
                .each_ref()
                .map(|word| word.load(Ordering::SeqCst));
            if self.sequence.load(Ordering::SeqCst) == sequence {
                *cached = RoomTarget {
                    epoch: words[0],
                    room: words[1] as u8,
                    enabled: words[1] & 256 != 0,
                    from: unpair(words[2]),
                    to: unpair(words[3]),
                    start: words[4],
                    end: words[5],
                };
                return;
            }
        }
    }
}

pub(crate) struct RoomBlock {
    pub enabled: bool,
    pub levels: [[f32; 2]; QUANTUM],
}

impl RoomBlock {
    pub(crate) fn new(target: RoomTarget, frame: u64) -> Self {
        Self {
            enabled: target.enabled,
            levels: std::array::from_fn(|i| target.levels(frame + i as u64)),
        }
    }
}

#[derive(Clone, Copy)]
struct Group {
    active: bool,
    room: u8,
    goal: [f32; 2],
}

#[derive(Resource)]
pub(crate) struct RoomAudioMix {
    groups: [Group; 4],
    selected: usize,
    target: RoomTarget,
    preview: Option<u8>,
    pending: std::collections::VecDeque<sim::ScriptAudioCommand>,
}

impl Default for RoomAudioMix {
    fn default() -> Self {
        let mut groups = [Group {
            active: false,
            room: 0,
            goal: [1.0, 0.0],
        }; 4];
        groups[0].active = true;
        Self {
            groups,
            selected: 0,
            target: RoomTarget::default(),
            preview: None,
            pending: Default::default(),
        }
    }
}

impl RoomAudioMix {
    fn select(&mut self, frame: u64, fade_ms: i32) {
        let group = self.groups[self.selected];
        let current = self.target.levels(frame);
        self.target = RoomTarget {
            from: current,
            to: group.goal,
            room: group.room,
            start: frame,
            end: frame.saturating_add(fade_ms.max(1) as u64 * u64::from(SAMPLE_RATE) / 1000),
            ..self.target
        };
    }
    fn apply(&mut self, frame: u64, command: sim::ScriptAudioCommand) {
        match command {
            sim::ScriptAudioCommand::Reverb {
                priority,
                room,
                dry,
                wet,
                fade_ms,
                ..
            } => {
                let room = crate::room_dsp::RoomProcessor::room_index(&room).unwrap_or_else(|| {
                    diag::warn!(Audio, "audio: unknown room `{room}`, using generic");
                    0
                });
                self.groups[usize::from(priority)] = Group {
                    active: true,
                    room,
                    goal: [dry, wet],
                };
                if usize::from(priority) >= self.selected {
                    self.selected = usize::from(priority);
                    self.select(frame, fade_ms);
                }
            }
            sim::ScriptAudioCommand::DeactivateReverb {
                priority, fade_ms, ..
            } => {
                self.groups[usize::from(priority)].active = false;
                if self.selected == usize::from(priority) {
                    self.selected = (0..self.selected)
                        .rev()
                        .find(|i| self.groups[*i].active)
                        .unwrap_or(0);
                    self.select(frame, fade_ms);
                }
            }
            _ => {}
        }
    }
}

fn update_room_mix(
    mut mix: ResMut<RoomAudioMix>,
    mut events: MessageReader<net::SvcScriptAudio>,
    local: Option<Res<net::LocalPresentClient>>,
    presented: Option<Res<net::PresentedSnapshot>>,
    dvars: Option<Res<frame::UiMenuDvars>>,
    runtime: Res<crate::AudioRuntime>,
) {
    for event in events.read() {
        if matches!(
            event.0,
            sim::ScriptAudioCommand::Reverb { .. }
                | sim::ScriptAudioCommand::DeactivateReverb { .. }
        ) {
            mix.pending.push_back(event.0.clone());
        }
    }
    let Some(local) = local else { return };
    let setting = presented
        .as_ref()
        .and_then(|view| view.snapshot())
        .and_then(|snapshot| {
            snapshot
                .meta
                .script_dvars(local.0)
                .string("snd_enableReverb")
        })
        .or_else(|| {
            dvars
                .as_ref()
                .and_then(|dvars| dvars.get("snd_enableReverb"))
        });
    mix.target.enabled = setting
        .and_then(|v| v.parse::<f32>().ok())
        .filter(|v| v.is_finite())
        .is_none_or(|v| v != 0.0);
    let frame = runtime.audio_frame();
    while let Some(command) = mix.pending.pop_front() {
        if command.target() == Some(local.0) && command.valid() {
            mix.apply(frame, command);
        }
    }
    let preview_name = dvars
        .as_ref()
        .and_then(|dvars| dvars.get("snd_roomPreview"));
    let preview = preview_name.and_then(crate::room_dsp::RoomProcessor::room_index);
    if preview != mix.preview {
        diag::info!(
            Audio,
            "audio: room preview={}",
            preview_name.filter(|_| preview.is_some()).unwrap_or("off")
        );
        mix.preview = preview;
    }
    let mut target = mix.target;
    if let Some(room) = preview {
        let wet = dvars
            .as_ref()
            .and_then(|dvars| dvars.get("snd_roomPreviewWet"))
            .and_then(|value| value.parse::<f32>().ok())
            .filter(|value| value.is_finite())
            .unwrap_or(0.5)
            .clamp(0.0, 1.0);
        target.room = room;
        target.from = [1.0, wet];
        target.to = target.from;
        target.start = frame;
        target.end = frame;
    }
    runtime.set_room_mix(target);
}

pub(crate) fn register(app: &mut App) {
    app.scoped::<RoomAudioMix>(frame::MatchScope::Live)
        .add_systems(
            Update,
            update_room_mix
                .in_set(frame::InMatch)
                .in_set(net::ClientSet::Effects),
        );
}
