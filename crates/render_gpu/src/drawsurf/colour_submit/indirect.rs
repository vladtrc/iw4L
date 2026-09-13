use bevy::render::render_resource::{BufferUsages, DownlevelFlags};
use bevy::render::renderer::{RenderAdapter, RenderDevice, RenderQueue};
use bevy::render::settings::WgpuFeatures;

use super::residency::GpuStream;

const ARG_WORDS: usize = 5;

pub(super) fn multi_draw_requested() -> bool {
    static REQUESTED: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *REQUESTED.get_or_init(|| std::env::var_os("IW4L_MULTI_DRAW").is_some())
}

#[derive(bevy::prelude::Resource, Default)]
pub(super) struct ExactIndirectDraws {
    stream: GpuStream,

    words: Vec<u32>,

    live: u32,

    resident_allocation: u64,

    pub(super) uploaded_words: u32,

    pub(super) folded: u32,
    pub(super) batches: u32,
}

impl ExactIndirectDraws {
    pub(super) fn capable(device: &RenderDevice, adapter: &RenderAdapter) -> bool {
        adapter
            .get_downlevel_capabilities()
            .flags
            .contains(DownlevelFlags::INDIRECT_EXECUTION)
            && device
                .features()
                .contains(WgpuFeatures::INDIRECT_FIRST_INSTANCE)
    }

    pub(super) fn publish(
        &mut self,
        draws: &mut [super::PreparedExactDraw],
        device: &RenderDevice,
        adapter: &RenderAdapter,
        queue: &RenderQueue,
    ) {
        self.folded = 0;
        self.batches = 0;
        self.live = 0;
        self.uploaded_words = 0;
        if !multi_draw_requested() || !Self::capable(device, adapter) {
            for draw in draws.iter_mut() {
                draw.indirect_arg = None;
            }
            return;
        }
        let needed = draws.len() * ARG_WORDS;

        let mut dirty = if self.words.len() == needed {
            None
        } else {
            self.words.clear();
            self.words.resize(needed, 0);
            (needed > 0).then_some((0usize, needed))
        };
        for (slot, draw) in draws.iter_mut().enumerate() {
            let args = match draw.constant_base {
                Some(constant_base) => {
                    draw.indirect_arg = Some(slot as u32);
                    [draw.count, 1, draw.start, 0, constant_base]
                }
                None => {
                    draw.indirect_arg = None;
                    [0; ARG_WORDS]
                }
            };
            let at = slot * ARG_WORDS;
            if self.words[at..at + ARG_WORDS] != args {
                self.words[at..at + ARG_WORDS].copy_from_slice(&args);
                dirty = Some(match dirty {
                    Some((first, end)) => (first.min(at), end.max(at + ARG_WORDS)),
                    None => (at, at + ARG_WORDS),
                });
            }
        }
        if self.words.is_empty() {
            return;
        }
        self.stream.reserve(
            device,
            "iw4_exact_colour_indirect_args",
            BufferUsages::INDIRECT,
            self.words.len(),
            std::mem::size_of::<u32>(),
        );
        self.stream.set_len(self.words.len());

        if self.resident_allocation != self.stream.allocation() {
            self.resident_allocation = self.stream.allocation();
            dirty = Some((0, self.words.len()));
        }
        if let Some((first, end)) = dirty {
            let word = std::mem::size_of::<u32>();
            let Some((start_byte, end_byte)) = super::residency::aligned_stream_span(
                first * word,
                end * word,
                self.words.len() * word,
            ) else {
                return;
            };
            let bytes: &[u8] = bytemuck::cast_slice(self.words.as_slice());
            if !self
                .stream
                .write_at(queue, start_byte as u64, &bytes[start_byte..end_byte])
            {
                self.resident_allocation = self.resident_allocation.wrapping_sub(1);
                for draw in draws.iter_mut() {
                    draw.indirect_arg = None;
                }
                return;
            }
            self.uploaded_words = ((end_byte - start_byte) / word) as u32;
        }
        self.live = (self.words.len() / ARG_WORDS) as u32;
    }

    pub(super) fn buffer(&self) -> Option<&bevy::render::render_resource::Buffer> {
        if self.live == 0 {
            return None;
        }
        self.stream.buffer()
    }

    pub(super) fn byte_offset(slot: u32) -> u64 {
        u64::from(slot) * (ARG_WORDS * std::mem::size_of::<u32>()) as u64
    }
}

#[derive(Clone, Copy, Default)]
pub(super) struct IndirectBatch {
    first: u32,
    count: u32,
}

impl IndirectBatch {
    pub(super) fn push(&mut self, slot: u32) -> Option<IndirectBatch> {
        if self.count > 0 && self.first.saturating_add(self.count) == slot {
            self.count = self.count.saturating_add(1);
            return None;
        }
        let flushed = self.take();
        self.first = slot;
        self.count = 1;
        flushed
    }

    pub(super) fn take(&mut self) -> Option<IndirectBatch> {
        let done = *self;
        self.count = 0;
        (done.count > 0).then_some(done)
    }

    pub(super) fn first(&self) -> u32 {
        self.first
    }

    pub(super) fn count(&self) -> u32 {
        self.count
    }
}
