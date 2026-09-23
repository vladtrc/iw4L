//! Counters a patched `wgpu` writes into; the render frame drains them into
//! `frames.csv`.
//!
//! A delivery build has no patched `wgpu`, so nothing writes here and [`live`]
//! stays false.

use std::sync::Mutex;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Instant;

#[repr(usize)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Slot {
    Submits,
    BodyNs,
    ValidateNs,
    TransitionNs,
    PendingNs,
    BackendNs,
    CleanupNs,
    CallbackNs,
    CmdBufs,
    EncodersRetired,
    RetireInnerNs,
    RetireTrackersNs,
    RetireTempNs,
    ReleaseEncoderNs,
    ResetCalls,
    ResetListNs,
    FramebufferDestroyN,
    FramebufferDestroyNs,
    PoolResetNs,
    FramebufferCreateN,
    FramebufferCreateNs,
    EncodersAcquired,
    EncodersBuilt,
}

impl Slot {
    pub const COUNT: usize = Self::EncodersBuilt as usize + 1;

    /// Indexed by `slot as usize`; must match declaration order.
    pub const ALL: [Self; Self::COUNT] = [
        Self::Submits,
        Self::BodyNs,
        Self::ValidateNs,
        Self::TransitionNs,
        Self::PendingNs,
        Self::BackendNs,
        Self::CleanupNs,
        Self::CallbackNs,
        Self::CmdBufs,
        Self::EncodersRetired,
        Self::RetireInnerNs,
        Self::RetireTrackersNs,
        Self::RetireTempNs,
        Self::ReleaseEncoderNs,
        Self::ResetCalls,
        Self::ResetListNs,
        Self::FramebufferDestroyN,
        Self::FramebufferDestroyNs,
        Self::PoolResetNs,
        Self::FramebufferCreateN,
        Self::FramebufferCreateNs,
        Self::EncodersAcquired,
        Self::EncodersBuilt,
    ];
}

static SLOTS: [AtomicU64; Slot::COUNT] = [const { AtomicU64::new(0) }; Slot::COUNT];

static ACQUIRED_TOTAL: AtomicU64 = AtomicU64::new(0);
static RETIRED_TOTAL: AtomicU64 = AtomicU64::new(0);

static LIVE: AtomicU64 = AtomicU64::new(0);

#[inline]
pub fn live() -> bool {
    LIVE.load(Ordering::Relaxed) != 0
}

#[inline]
pub fn add(slot: Slot, by: u64) {
    LIVE.store(1, Ordering::Relaxed);
    SLOTS[slot as usize].fetch_add(by, Ordering::Relaxed);
    match slot {
        Slot::EncodersAcquired => {
            ACQUIRED_TOTAL.fetch_add(by, Ordering::Relaxed);
        }
        Slot::EncodersRetired => {
            RETIRED_TOTAL.fetch_add(by, Ordering::Relaxed);
        }
        _ => {}
    }
}

#[inline]
pub fn add_elapsed(slot: Slot, since: Instant) {
    add(slot, since.elapsed().as_nanos() as u64);
}

pub fn drain() -> Drained {
    let mut slots = [0u64; Slot::COUNT];
    for slot in Slot::ALL {
        slots[slot as usize] = SLOTS[slot as usize].swap(0, Ordering::Relaxed);
    }
    Drained {
        slots,
        outstanding: ACQUIRED_TOTAL
            .load(Ordering::Relaxed)
            .saturating_sub(RETIRED_TOTAL.load(Ordering::Relaxed)),
    }
}

#[derive(Clone, Copy, Debug, Default)]
pub struct Drained {
    pub slots: [u64; Slot::COUNT],
    pub outstanding: u64,
}

impl Drained {
    #[inline]
    pub fn get(&self, slot: Slot) -> u64 {
        self.slots[slot as usize]
    }

    pub fn is_empty(&self) -> bool {
        self.slots.iter().all(|&v| v == 0)
    }
}

const LABEL: usize = 32;

fn short(label: &str) -> String {
    let end = (0..=LABEL.min(label.len()))
        .rev()
        .find(|at| label.is_char_boundary(*at))
        .unwrap_or(0);
    label[..end].to_owned()
}

const LABELS: usize = 32;
const KEYS: usize = 16;

static FRAMEBUFFER_KEYS: Mutex<Vec<FramebufferPass>> = Mutex::new(Vec::new());

#[derive(Clone, Debug)]
pub struct FramebufferPass {
    pub label: String,
    pub keys: Vec<FramebufferKeyRow>,
    pub overflow_keys: u64,
    pub overflow_creations: u64,
}

#[derive(Clone, Copy, Debug)]
pub struct FramebufferKeyRow {
    pub key: u64,
    pub extent: (u32, u32),
    pub attachments: u32,
    pub creations: u64,
}

pub fn note_framebuffer(label: &str, key: u64, extent: (u32, u32), attachments: u32) {
    let label = short(label);
    let Ok(mut passes) = FRAMEBUFFER_KEYS.lock() else {
        return;
    };
    let pass = match passes.iter_mut().position(|pass| pass.label == label) {
        Some(at) => &mut passes[at],
        None if passes.len() < LABELS => {
            passes.push(FramebufferPass {
                label,
                keys: Vec::new(),
                overflow_keys: 0,
                overflow_creations: 0,
            });
            passes.last_mut().expect("just pushed")
        }
        None => return,
    };
    if let Some(row) = pass.keys.iter_mut().find(|row| row.key == key) {
        row.creations += 1;
    } else if pass.keys.len() < KEYS {
        pass.keys.push(FramebufferKeyRow {
            key,
            extent,
            attachments,
            creations: 1,
        });
    } else {
        pass.overflow_keys += 1;
        pass.overflow_creations += 1;
    }
}

pub fn framebuffers() -> Vec<FramebufferPass> {
    match FRAMEBUFFER_KEYS.lock() {
        Ok(passes) => passes.clone(),
        Err(poison) => poison.into_inner().clone(),
    }
}

const SIGNATURES: usize = 48;
const PASSES_PER_ENCODER: usize = 8;

static OPEN_ENCODERS: Mutex<Vec<(u64, Vec<String>)>> = Mutex::new(Vec::new());
static ENCODER_SHAPES: Mutex<Vec<EncoderShape>> = Mutex::new(Vec::new());

#[derive(Clone, Debug)]
pub struct EncoderShape {
    pub signature: String,
    pub retires: u64,
}

pub fn note_pass(encoder: u64, label: &str) {
    let Ok(mut open) = OPEN_ENCODERS.lock() else {
        return;
    };
    let labels = match open.iter_mut().find(|(id, _)| *id == encoder) {
        Some((_, labels)) => labels,
        None => {
            open.push((encoder, Vec::new()));
            &mut open.last_mut().expect("just pushed").1
        }
    };
    if labels.len() < PASSES_PER_ENCODER {
        labels.push(short(label));
    } else if labels.len() == PASSES_PER_ENCODER {
        labels.push("…".to_owned());
    }
}

pub fn note_reset(encoder: u64) {
    let signature = {
        let Ok(mut open) = OPEN_ENCODERS.lock() else {
            return;
        };
        match open.iter().position(|(id, _)| *id == encoder) {
            Some(at) => open.swap_remove(at).1.join(" "),
            None => String::new(),
        }
    };
    let Ok(mut shapes) = ENCODER_SHAPES.lock() else {
        return;
    };
    if let Some(shape) = shapes.iter_mut().find(|shape| shape.signature == signature) {
        shape.retires += 1;
    } else if shapes.len() < SIGNATURES {
        shapes.push(EncoderShape {
            signature,
            retires: 1,
        });
    }
}

pub fn encoders() -> Vec<EncoderShape> {
    match ENCODER_SHAPES.lock() {
        Ok(shapes) => shapes.clone(),
        Err(poison) => poison.into_inner().clone(),
    }
}
