use crate::smodel_lighting::{
    SMODEL_LIGHTING_FREEABLE_AGE_FRAMES, SMODEL_LIGHTING_RESERVED_ENTRY0_AFTER_WALK,
    draw_inst_defers_lighting, draw_inst_lighting_handle_entry, model_lighting_handle_from_entry,
    smodel_lighting_bits_mask, smodel_lighting_msb_local_bit,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SModelDirtyLightingAction {
    SampleImmediate { smodel_index: u32, entry: u16 },

    EnqueuePatch { smodel_index: u32, entry: u16 },
}

#[inline]
pub fn dirty_smodel_lighting_action(
    smodel_index: u32,
    handle: u16,
    flags: u8,
) -> Option<SModelDirtyLightingAction> {
    let entry = draw_inst_lighting_handle_entry(handle)?;
    if draw_inst_defers_lighting(flags) {
        Some(SModelDirtyLightingAction::EnqueuePatch {
            smodel_index,
            entry,
        })
    } else {
        Some(SModelDirtyLightingAction::SampleImmediate {
            smodel_index,
            entry,
        })
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SModelLightingAlloc {
    Reused {
        handle: u16,
    },

    Assigned {
        handle: u16,
    },

    Evicted {
        handle: u16,
        evicted_smodel_index: u16,
    },

    Failed,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SModelLightingGlobError {
    EntryLimitOutOfRange { entry_limit: u32 },

    EntryArrayTooShort { needed: usize, got: usize },

    LightingBitsTooShort { needed: usize, got: usize },
}

#[inline]
pub const fn smodel_lighting_bits_words(smodel_count: u32) -> usize {
    smodel_count.div_ceil(32) as usize
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SModelLightingCounters {
    pub assigned_count: u32,

    pub freeable_count: u32,

    pub frame_count: i32,

    pub any_new_lighting: bool,
}

impl SModelLightingCounters {
    pub const AFTER_LIGHT_WALK: Self = Self {
        assigned_count: SMODEL_LIGHTING_RESERVED_ENTRY0_AFTER_WALK,
        freeable_count: 0,
        frame_count: 0,
        any_new_lighting: false,
    };
}

impl Default for SModelLightingCounters {
    fn default() -> Self {
        Self::AFTER_LIGHT_WALK
    }
}

#[derive(Debug)]
pub struct SModelLightingGlob<'a> {
    entry_limit: u32,
    smodel_count: u32,

    counters: &'a mut SModelLightingCounters,

    freeable_handles: &'a mut [u16],

    smodel_index: &'a mut [u16],

    used_frame_count: &'a mut [i32],

    lighting_bits: &'a mut [u32],
}

impl<'a> SModelLightingGlob<'a> {
    pub fn new(
        entry_limit: u32,
        smodel_count: u32,
        counters: &'a mut SModelLightingCounters,
        freeable_handles: &'a mut [u16],
        smodel_index: &'a mut [u16],
        used_frame_count: &'a mut [i32],
        lighting_bits: &'a mut [u32],
    ) -> Result<Self, SModelLightingGlobError> {
        if entry_limit == 0 || entry_limit >= u32::from(u16::MAX) {
            return Err(SModelLightingGlobError::EntryLimitOutOfRange { entry_limit });
        }
        let needed = entry_limit as usize;
        for got in [
            freeable_handles.len(),
            smodel_index.len(),
            used_frame_count.len(),
        ] {
            if got < needed {
                return Err(SModelLightingGlobError::EntryArrayTooShort { needed, got });
            }
        }
        let bit_words = smodel_lighting_bits_words(smodel_count);
        if lighting_bits.len() < bit_words {
            return Err(SModelLightingGlobError::LightingBitsTooShort {
                needed: bit_words,
                got: lighting_bits.len(),
            });
        }
        Ok(Self {
            entry_limit,
            smodel_count,
            counters,
            freeable_handles,
            smodel_index,
            used_frame_count,
            lighting_bits,
        })
    }

    #[inline]
    pub const fn entry_limit(&self) -> u32 {
        self.entry_limit
    }

    #[inline]
    pub const fn assigned_count(&self) -> u32 {
        self.counters.assigned_count
    }

    #[inline]
    pub const fn freeable_count(&self) -> u32 {
        self.counters.freeable_count
    }

    #[inline]
    pub const fn frame_count(&self) -> i32 {
        self.counters.frame_count
    }

    #[inline]
    pub const fn any_new_lighting(&self) -> bool {
        self.counters.any_new_lighting
    }

    #[inline]
    pub fn owner_of_entry(&self, entry: u32) -> Option<u16> {
        if entry >= self.counters.assigned_count {
            return None;
        }
        self.smodel_index.get(entry as usize).copied()
    }

    #[inline]
    pub fn is_dirty(&self, smodel_index: u32) -> bool {
        let (word, mask) = smodel_lighting_bits_mask(smodel_index);
        self.lighting_bits
            .get(word)
            .is_some_and(|bits| bits & mask != 0)
    }

    pub fn clear_dirty_bits(&mut self) {
        let words = smodel_lighting_bits_words(self.smodel_count);
        for word in self.lighting_bits.iter_mut().take(words) {
            *word = 0;
        }
        self.counters.any_new_lighting = false;
    }

    pub fn toggle_frame(&mut self) {
        self.counters.frame_count = self.counters.frame_count.wrapping_add(1);
        self.counters.freeable_count = 0;
        let mut entry = SMODEL_LIGHTING_RESERVED_ENTRY0_AFTER_WALK;
        while entry < self.counters.assigned_count {
            let idle = self
                .counters
                .frame_count
                .wrapping_sub(self.used_frame_count[entry as usize]);
            if idle >= SMODEL_LIGHTING_FREEABLE_AGE_FRAMES as i32 {
                self.freeable_handles[self.counters.freeable_count as usize] =
                    model_lighting_handle_from_entry(entry as u16);
                self.counters.freeable_count += 1;
            }
            entry += 1;
        }
    }

    pub fn update_dirty_smodel_lighting(&mut self, mut visit: impl FnMut(u32)) -> u32 {
        if !self.counters.any_new_lighting {
            return 0;
        }
        self.counters.any_new_lighting = false;
        let word_count = smodel_lighting_bits_words(self.smodel_count);
        let mut visited = 0u32;
        for word_i in 0..word_count {
            let mut bits = self.lighting_bits[word_i];
            while let Some(local) = smodel_lighting_msb_local_bit(bits) {
                if local > 0x1f {
                    break;
                }
                let smodel_index = local + (word_i as u32) * 32;
                bits &= !(0x8000_0000u32 >> (local & 31));
                visit(smodel_index);
                visited = visited.saturating_add(1);
            }
        }
        visited
    }

    pub fn alloc_static_model_lighting(
        &mut self,
        smodel_index: u32,
        current_handle: u16,
    ) -> SModelLightingAlloc {
        if let Some(entry) = draw_inst_lighting_handle_entry(current_handle) {
            if let Some(stamp) = self.used_frame_count.get_mut(usize::from(entry)) {
                *stamp = self.counters.frame_count;
            }

            return SModelLightingAlloc::Reused {
                handle: current_handle,
            };
        }

        let mut evicted = None;
        let entry = if self.counters.assigned_count < self.entry_limit {
            let entry = self.counters.assigned_count;
            self.counters.assigned_count += 1;
            entry
        } else {
            loop {
                if self.counters.freeable_count == 0 {
                    return SModelLightingAlloc::Failed;
                }
                self.counters.freeable_count -= 1;
                let handle = self.freeable_handles[self.counters.freeable_count as usize];
                let Some(entry) = draw_inst_lighting_handle_entry(handle) else {
                    continue;
                };
                let entry = u32::from(entry);
                if self.used_frame_count[entry as usize] != self.counters.frame_count {
                    evicted = Some(self.smodel_index[entry as usize]);
                    break entry;
                }
            }
        };

        self.smodel_index[entry as usize] = smodel_index as u16;
        let (word, mask) = smodel_lighting_bits_mask(smodel_index);
        if let Some(bits) = self.lighting_bits.get_mut(word) {
            *bits |= mask;
        }
        self.counters.any_new_lighting = true;
        self.used_frame_count[entry as usize] = self.counters.frame_count;

        let handle = model_lighting_handle_from_entry(entry as u16);
        match evicted {
            Some(evicted_smodel_index) => SModelLightingAlloc::Evicted {
                handle,
                evicted_smodel_index,
            },
            None => SModelLightingAlloc::Assigned { handle },
        }
    }
}
