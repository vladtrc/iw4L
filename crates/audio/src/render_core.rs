use std::cell::UnsafeCell;
use std::sync::atomic::{AtomicBool, AtomicU8, AtomicU32, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use crate::admission::AdmissionFailure;
use crate::media::RenderMedia;

pub const SAMPLE_RATE: u32 = 48_000;
pub const QUANTUM: usize = 128;
pub const PHYSICAL_VOICES: usize = 128;
pub(crate) const MIN_ATTACK_FRAMES: u64 = SAMPLE_RATE as u64 / 50;

const FREE: u8 = 0;
const PUBLISHED: u8 = 1;
const RETIRED: u8 = 2;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum AudioScope {
    Menu,
    Match,
    MatchMusic,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum InstanceStatus {
    Requested,
    Accepted,
    Scheduled,
    Started,
    Virtual,
    SourceEnded,
    Finished,
    Retired,
}

pub(crate) struct InstanceState {
    pub id: u64,
    pub scope: AudioScope,
    pub stopped: AtomicBool,
    pub finish_attack: AtomicBool,
    pub paused: AtomicBool,
    pub audible: AtomicBool,
    pub gain: AtomicU32,
    pub rate: AtomicU32,
    pub priority: AtomicU32,
    pub rejection: AtomicU8,
    pub cursor: AtomicU64,
    pub rendered_at: AtomicU64,
    pub requested_at: std::time::Instant,
    pub first_device_us: AtomicU64,
    pub first_device_frame: AtomicU64,
    pub device_frames: AtomicU64,
    pub null_frames: AtomicU64,
    pub status: AtomicU8,
    pub transitions: AtomicU8,
}

impl InstanceState {
    pub fn status(&self) -> InstanceStatus {
        match self.status.load(Ordering::Acquire) {
            0 => InstanceStatus::Requested,
            1 => InstanceStatus::Accepted,
            2 => InstanceStatus::Scheduled,
            3 => InstanceStatus::Started,
            4 => InstanceStatus::Virtual,
            5 => InstanceStatus::SourceEnded,
            6 => InstanceStatus::Finished,
            _ => InstanceStatus::Retired,
        }
    }

    pub fn set_status(&self, status: InstanceStatus) {
        self.transitions
            .fetch_or(1 << status as u8, Ordering::Release);
        self.status.store(status as u8, Ordering::Release);
    }

    pub fn has_reached(&self, status: InstanceStatus) -> bool {
        self.transitions.load(Ordering::Acquire) & (1 << status as u8) != 0
    }

    pub fn retire(&self) {
        self.set_status(InstanceStatus::Finished);
        self.set_status(InstanceStatus::Retired);
    }

    pub fn reject(&self, reason: AdmissionFailure) {
        self.stopped.store(true, Ordering::Release);
        self.rejection.store(reason as u8, Ordering::Release);
        self.retire();
    }

    pub fn rejection(&self) -> Option<AdmissionFailure> {
        match self.rejection.load(Ordering::Acquire) {
            1 => Some(AdmissionFailure::QueueFull),
            2 => Some(AdmissionFailure::LogicalBudget),
            3 => Some(AdmissionFailure::PhysicalBudget),
            4 => Some(AdmissionFailure::Concurrency),
            5 => Some(AdmissionFailure::Cancelled),
            6 => Some(AdmissionFailure::StaleScope),
            7 => Some(AdmissionFailure::OutputUnavailable),
            _ => None,
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) struct RenderVoiceId(pub u64);

pub(crate) struct Assignment {
    pub voice: RenderVoiceId,
    pub media: RenderMedia,
    pub instance: Arc<InstanceState>,
    pub start_frame: u64,
    pub start_cursor: f64,
    pub looping: bool,
}

struct Slot {
    phase: AtomicU8,
    assignment: UnsafeCell<Option<Assignment>>,
}

// Only the control owner writes FREE slots or takes RETIRED assignments. Release/
// acquire phase changes transfer ownership. Rendering never destroys assignments.
unsafe impl Sync for Slot {}

impl Slot {
    fn new() -> Self {
        Self {
            phase: AtomicU8::new(FREE),
            assignment: UnsafeCell::new(None),
        }
    }
}

#[derive(Clone, Copy)]
struct Cursor {
    voice: RenderVoiceId,
    position: f64,
    gains: [crate::media::GainEnvelope; 3],
    eq: crate::eq::VoiceEq,
}

struct RenderState {
    slaves: crate::master::SlaveMixer,
    cursors: [Cursor; PHYSICAL_VOICES],
    buses: crate::mix_bus::MixBuses,
    limiter: crate::mix_bus::MasterLimiter,
    room: crate::room_dsp::RoomProcessor,
    room_target: crate::room_mix::RoomTarget,
    room_active: bool,
    eq_target: crate::eq::EqTarget,
    eq_revision: u64,
}

/// The mix taken off the device for an offline capture. While armed the device
/// plays silence, and the control thread renders up to `target` and keeps the
/// blocks here, so the mix advances by the capturer's clock, not the wall's.
#[derive(Default)]
pub(crate) struct CaptureTap {
    pub armed: AtomicBool,
    pub target: AtomicU64,
    pub samples: Mutex<Vec<[f32; 2]>>,
}

pub(crate) struct RenderShared {
    slots: [Slot; PHYSICAL_VOICES],
    rendering: AtomicBool,
    state: UnsafeCell<RenderState>,
    pub room: crate::room_mix::RoomControl,
    pub eq: crate::eq::EqControl,
    pub slaves: crate::master::SlaveControl,
    pub frame: AtomicU64,
    pub master: AtomicU32,
    pub busy_blocks: AtomicU64,
    pub device_active: AtomicBool,
    pub device_blocks: AtomicU64,
    pub device_callbacks: AtomicU64,
    pub device_required: AtomicBool,
    pub null_blocks: AtomicU64,
    pub device_underruns: AtomicU64,
    pub cancelled: AtomicBool,
    pub peak: AtomicU32,
    pub capture: CaptureTap,
}

// The bounded rendering lease exclusively owns RenderState. Slot payload access
// additionally follows Slot's phase protocol; control never touches RenderState.
unsafe impl Sync for RenderShared {}

struct RenderLease<'a>(&'a AtomicBool);

impl Drop for RenderLease<'_> {
    fn drop(&mut self) {
        self.0.store(false, Ordering::Release);
    }
}

impl RenderShared {
    pub fn synchronize(&self) {
        while self.rendering.load(Ordering::Acquire) {
            std::thread::yield_now();
        }
    }

    pub fn new() -> Self {
        Self {
            slots: std::array::from_fn(|_| Slot::new()),
            rendering: AtomicBool::new(false),
            state: UnsafeCell::new(RenderState {
                slaves: Default::default(),
                cursors: [Cursor {
                    voice: RenderVoiceId(0),
                    position: 0.0,
                    gains: [crate::media::GainEnvelope::default(); 3],
                    eq: Default::default(),
                }; PHYSICAL_VOICES],
                buses: Default::default(),
                limiter: Default::default(),
                room: Default::default(),
                room_target: Default::default(),
                room_active: false,
                eq_target: Default::default(),
                eq_revision: 0,
            }),
            room: Default::default(),
            eq: Default::default(),
            slaves: Default::default(),
            frame: AtomicU64::new(0),
            master: AtomicU32::new(1.0f32.to_bits()),
            busy_blocks: AtomicU64::new(0),
            device_active: AtomicBool::new(false),
            device_blocks: AtomicU64::new(0),
            device_callbacks: AtomicU64::new(0),
            device_required: AtomicBool::new(false),
            null_blocks: AtomicU64::new(0),
            device_underruns: AtomicU64::new(0),
            cancelled: AtomicBool::new(false),
            peak: AtomicU32::new(0),
            capture: CaptureTap::default(),
        }
    }

    pub fn render(&self, output: &mut [[f32; 2]; QUANTUM]) -> bool {
        self.render_for(output, None)
    }

    pub fn render_for(&self, output: &mut [[f32; 2]; QUANTUM], device: Option<bool>) -> bool {
        output.fill([0.0; 2]);
        if self
            .rendering
            .compare_exchange(false, true, Ordering::Acquire, Ordering::Relaxed)
            .is_err()
        {
            self.busy_blocks.fetch_add(1, Ordering::Relaxed);
            return false;
        }
        let _lease = RenderLease(&self.rendering);
        if device.is_some_and(|device| device != self.device_active.load(Ordering::Acquire)) {
            return false;
        }
        // The lease is held until every reference to this state has expired.
        let state = unsafe { &mut *self.state.get() };
        let frame = self.frame.load(Ordering::Relaxed);
        let old = state.room_target;
        self.room.read(&mut state.room_target);
        if old.epoch != state.room_target.epoch {
            state.room.clear();
            state.room_active = false;
        }
        if old.epoch != state.room_target.epoch || old.room != state.room_target.room {
            state.room.set_room(state.room_target.room);
        }
        let room_block = crate::room_mix::RoomBlock::new(state.room_target, frame);
        let eq_epoch = state.eq_target.epoch;
        self.eq.read(&mut state.eq_revision, &mut state.eq_target);
        if state.eq_target.epoch != eq_epoch {
            for cursor in &mut state.cursors {
                cursor.eq = Default::default();
            }
        }
        let cancelled = self.cancelled.load(Ordering::Acquire);
        let hold_one_shots = device == Some(false) && self.device_required.load(Ordering::Relaxed);
        let mut master_active = false;
        let mut master_percentage = 0.0f32;
        for slot in &self.slots {
            if slot.phase.load(Ordering::Acquire) != PUBLISHED {
                continue;
            }
            let assignment = unsafe { (&*slot.assignment.get()).as_ref().unwrap() };
            let instance = &assignment.instance;
            let Some(voice) = assignment.media.master_slave.filter(|voice| voice.master) else {
                continue;
            };
            if cancelled
                || instance.scope == AudioScope::Menu
                || instance.stopped.load(Ordering::Acquire)
                || !instance.audible.load(Ordering::Acquire)
                || frame < assignment.start_frame
                || (hold_one_shots && !assignment.looping)
                || assignment.media.release.as_ref().is_some_and(|release| {
                    release.expired(frame)
                        || (release.requested() && !instance.has_reached(InstanceStatus::Started))
                })
            {
                continue;
            }
            master_active = true;
            master_percentage = master_percentage.max(voice.master_percentage);
        }
        state.slaves.advance(
            master_active,
            master_percentage,
            QUANTUM as u64,
            &self.slaves,
        );
        for (index, slot) in self.slots.iter().enumerate() {
            if slot.phase.load(Ordering::Acquire) != PUBLISHED {
                continue;
            }
            let ended = {
                // Control cannot replace a PUBLISHED payload until we retire it.
                let assignment = unsafe { (&*slot.assignment.get()).as_ref().unwrap() };
                let (ended, rendered) = render_assignment(
                    assignment,
                    &mut state.cursors[index],
                    frame,
                    cancelled,
                    hold_one_shots,
                    state.buses.bus(assignment.media.bus),
                    &RenderMix {
                        room: &room_block,
                        eq: &state.eq_target,
                        slaves: &state.slaves,
                    },
                );
                match device {
                    Some(true) => {
                        if rendered != 0
                            && assignment.instance.first_device_us.load(Ordering::Relaxed)
                                == u64::MAX
                        {
                            assignment
                                .instance
                                .first_device_frame
                                .store(frame.max(assignment.start_frame), Ordering::Relaxed);
                            assignment.instance.first_device_us.store(
                                assignment.instance.requested_at.elapsed().as_micros() as u64,
                                Ordering::Release,
                            );
                        }
                        assignment
                            .instance
                            .device_frames
                            .fetch_add(rendered as u64, Ordering::Relaxed);
                    }
                    Some(false) => {
                        assignment
                            .instance
                            .null_frames
                            .fetch_add(rendered as u64, Ordering::Relaxed);
                    }
                    None => {}
                }
                ended
            };
            if ended {
                slot.phase.store(RETIRED, Ordering::Release);
            }
        }
        let mut sends = [[0.0; 2]; QUANTUM];
        state.buses.mix_into(output, &mut sends);
        state.room_active |= sends.iter().flatten().any(|v| *v != 0.0);
        if state.room_active {
            for (target, wet) in output.iter_mut().zip(state.room.process_block(&sends)) {
                target[0] += wet[0];
                target[1] += wet[1];
            }
        }
        let master = f32::from_bits(self.master.load(Ordering::Relaxed));
        for sample in output.iter_mut().flatten() {
            let value = *sample * master;
            *sample = if value.is_finite() { value } else { 0.0 };
        }
        state.limiter.apply(output);
        let mut peak = f32::from_bits(self.peak.load(Ordering::Relaxed));
        for sample in output.iter_mut().flatten() {
            *sample = sample.clamp(-1.0, 1.0);
            peak = peak.max(sample.abs());
        }
        self.peak.store(peak.to_bits(), Ordering::Relaxed);
        self.frame.store(frame + QUANTUM as u64, Ordering::Release);
        match device {
            Some(true) => {
                self.device_blocks.fetch_add(1, Ordering::Relaxed);
            }
            Some(false) => {
                self.null_blocks.fetch_add(1, Ordering::Relaxed);
            }
            None => {}
        }
        true
    }

    /// # Safety
    /// The caller must be the sole control owner of this `RenderShared`.
    /// Calls to `publish` and `reclaim` must not overlap, including across threads.
    pub unsafe fn publish(&self, assignment: Assignment) -> Result<usize, Assignment> {
        let Some(index) = self
            .slots
            .iter()
            .position(|slot| slot.phase.load(Ordering::Acquire) == FREE)
        else {
            return Err(assignment);
        };
        let slot = &self.slots[index];
        unsafe { *slot.assignment.get() = Some(assignment) };
        slot.phase.store(PUBLISHED, Ordering::Release);
        Ok(index)
    }

    /// # Safety
    /// The caller must be the sole control owner of this `RenderShared`.
    /// Calls to `publish` and `reclaim` must not overlap, including across threads.
    pub unsafe fn reclaim(&self, index: usize) -> Option<Assignment> {
        let slot = self.slots.get(index)?;
        if slot.phase.load(Ordering::Acquire) != RETIRED {
            return None;
        }
        let assignment = unsafe { (&mut *slot.assignment.get()).take() };
        slot.phase.store(FREE, Ordering::Release);
        assignment
    }
}

struct RenderMix<'a> {
    room: &'a crate::room_mix::RoomBlock,
    eq: &'a crate::eq::EqTarget,
    slaves: &'a crate::master::SlaveMixer,
}

fn render_assignment(
    assignment: &Assignment,
    cursor: &mut Cursor,
    frame: u64,
    cancelled: bool,
    hold_one_shots: bool,
    (output, sends): (&mut [[f32; 2]; QUANTUM], &mut [[f32; 2]; QUANTUM]),
    mix: &RenderMix<'_>,
) -> (bool, usize) {
    let room = mix.room;
    let eq = mix.eq;
    let instance = &assignment.instance;
    if cancelled
        || instance.stopped.load(Ordering::Acquire)
        || assignment.media.release.as_ref().is_some_and(|release| {
            release.expired(frame)
                || (release.requested() && !instance.has_reached(InstanceStatus::Started))
        })
    {
        instance.set_status(InstanceStatus::Finished);
        return (true, 0);
    }
    if hold_one_shots && !assignment.looping {
        return (false, 0);
    }
    let rate = f32::from_bits(instance.rate.load(Ordering::Relaxed));
    let step = f64::from(assignment.media.rate()) / f64::from(SAMPLE_RATE) * f64::from(rate);
    if cursor.voice != assignment.voice {
        cursor.voice = assignment.voice;
        cursor.position = assignment.start_cursor;
        cursor.gains = [crate::media::GainEnvelope::default(); 3];
        cursor.eq = Default::default();
        if assignment.looping && !instance.paused.load(Ordering::Relaxed) {
            cursor.position += frame.saturating_sub(assignment.start_frame) as f64 * step;
        }
    }
    if !instance.audible.load(Ordering::Acquire) {
        instance
            .cursor
            .store(cursor.position.to_bits(), Ordering::Release);
        instance.rendered_at.store(frame, Ordering::Release);
        instance.set_status(InstanceStatus::Virtual);
        return (true, 0);
    }
    if instance.paused.load(Ordering::Relaxed) {
        return (false, 0);
    }
    let first = assignment
        .start_frame
        .saturating_sub(frame)
        .min(QUANTUM as u64) as usize;
    if first == QUANTUM {
        return (false, 0);
    }
    instance.set_status(InstanceStatus::Started);
    let slave_gain = if instance.scope == AudioScope::Menu {
        1.0
    } else {
        assignment
            .media
            .master_slave
            .map_or(1.0, |voice| voice.gain(mix.slaves))
    };
    let gain = f32::from_bits(instance.gain.load(Ordering::Relaxed)) * slave_gain;
    let frames = assignment.media.frames();
    let gains = assignment.media.gains(&mut cursor.gains);
    let use_eq = eq.enabled
        && instance.scope != AudioScope::Menu
        && !eq.bypasses(assignment.media.eq_entity);
    let mut rendered = 0;
    for (offset, target) in output[first..].iter_mut().enumerate() {
        let audio_frame = frame + first as u64 + offset as u64;
        if assignment
            .media
            .release
            .as_ref()
            .is_some_and(|release| release.expired(audio_frame))
        {
            instance
                .cursor
                .store(cursor.position.to_bits(), Ordering::Release);
            instance.rendered_at.store(audio_frame, Ordering::Release);
            instance.set_status(InstanceStatus::Finished);
            return (true, rendered);
        }
        if cursor.position >= frames as f64 {
            if assignment.looping {
                cursor.position %= frames as f64;
            } else {
                instance
                    .cursor
                    .store(cursor.position.to_bits(), Ordering::Release);
                instance.set_status(InstanceStatus::SourceEnded);
                return (true, rendered);
            }
        }
        let mut sample = assignment
            .media
            .interpolate(cursor.position, assignment.looping);
        if use_eq && let Some(channel) = assignment.media.eq_channel {
            sample = cursor
                .eq
                .process(eq.channels[usize::from(channel)], eq.fraction, sample);
        }
        let gains = gains.sample(audio_frame);
        let routing = (instance.scope != AudioScope::Menu)
            .then_some(assignment.media.room)
            .flatten();
        let routed = routing.map_or([1.0, 0.0], |voice| {
            voice.gains(room.levels[first + offset], room.enabled)
        });
        for side in 0..2 {
            let sample = sample[side] * gain * gains[side];
            target[side] += sample * routed[0];
            sends[first + offset][side] += sample * routed[1];
        }
        cursor.position += step;
        rendered += 1;
    }
    instance
        .cursor
        .store(cursor.position.to_bits(), Ordering::Release);
    instance
        .rendered_at
        .store(frame + QUANTUM as u64, Ordering::Release);
    if !assignment.looping && cursor.position >= frames as f64 {
        instance.set_status(InstanceStatus::SourceEnded);
        return (true, rendered);
    }
    if instance.finish_attack.load(Ordering::Acquire)
        && instance.device_frames.load(Ordering::Relaxed)
            + instance.null_frames.load(Ordering::Relaxed)
            + rendered as u64
            >= MIN_ATTACK_FRAMES
    {
        instance.set_status(InstanceStatus::Finished);
        return (true, rendered);
    }
    (false, rendered)
}
