use crate::smodel_lighting::MODEL_LIGHTING_WARN_CACHE_ALLOC_FAILED;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ModelLightingCacheAlloc {
    Reused { handle: u16, info: u16 },

    Assigned { handle: u16, slot: u32 },

    Failed,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ModelLightingCacheGlobError {
    LimitsOutOfRange {
        base_index: u32,
        xmodel_entry_limit: u32,
    },

    FreeBitsWordCountMismatch {
        needed: usize,
        got: usize,
    },

    EntryArrayTooShort {
        needed: usize,
        got: usize,
    },
}

pub const MODEL_LIGHTING_PIXEL_FREE_BITS_BUFFERS: usize = 4;

#[inline]
pub const fn model_lighting_cache_free_bits_words(xmodel_entry_limit: u32) -> usize {
    xmodel_entry_limit.div_ceil(32) as usize
}

#[inline]
pub const fn model_lighting_pixel_free_bits_size_bytes(xmodel_entry_limit: u32) -> u32 {
    xmodel_entry_limit >> 3
}

#[inline]
pub const fn model_lighting_pixel_free_bits_word_count(xmodel_entry_limit: u32) -> u32 {
    xmodel_entry_limit >> 5
}

#[inline]
pub const fn dyn_pixel_free_bits_index(mod_frame_count: u32, age: u32) -> usize {
    ((mod_frame_count.wrapping_sub(age)) & 3) as usize
}

pub fn toggle_dyn_model_lighting_frame(
    pixel_free_bits: &mut [&mut [u32]; MODEL_LIGHTING_PIXEL_FREE_BITS_BUFFERS],
    mod_frame_count: &mut u32,
    alloc_fail: &mut bool,
) {
    *mod_frame_count = (*mod_frame_count + 1) & 3;
    *alloc_fail = false;
    for word in pixel_free_bits[*mod_frame_count as usize].iter_mut() {
        *word = !0;
    }
}

pub fn model_lighting_cache_free_slot_count(
    prev_prev: &[u32],
    prev: &[u32],
    curr: &[u32],
    xmodel_entry_limit: u32,
) -> u32 {
    let words = model_lighting_cache_free_bits_words(xmodel_entry_limit);
    let mut n = 0u32;
    for wi in 0..words {
        if wi >= prev_prev.len() || wi >= prev.len() || wi >= curr.len() {
            break;
        }
        let free = prev_prev[wi] & prev[wi] & curr[wi];
        let base = (wi as u32) * 32;
        let remain = xmodel_entry_limit.saturating_sub(base).min(32);
        for bit in 0..remain {
            if free & model_lighting_cache_bit_mask(base + bit) != 0 {
                n += 1;
            }
        }
    }
    n
}

#[inline]
pub fn model_lighting_cache_handle(base_index: u32, slot: u32) -> Option<u16> {
    let handle = base_index.checked_add(slot)?.checked_add(1)?;
    u16::try_from(handle).ok()
}

#[inline]
pub const fn lighting_info_scene_light_index(info: u16) -> u8 {
    (info & 0xff) as u8
}

#[inline]
pub const fn lighting_info_reflection_probe_index(info: u16) -> u8 {
    (info >> 8) as u8
}

#[inline]
pub const fn lighting_info_from_bytes(primary_light_index: u8, reflection_probe_index: u8) -> u16 {
    (primary_light_index as u16) | ((reflection_probe_index as u16) << 8)
}

#[inline]
pub fn model_lighting_cache_slot(base_index: u32, handle: u16) -> Option<u32> {
    if handle == 0 {
        return None;
    }
    let h = handle as u32;
    if h <= base_index {
        return None;
    }
    Some(h - base_index - 1)
}

#[inline]
pub const fn model_lighting_cache_bit_mask(slot: u32) -> u32 {
    0x8000_0000u32 >> (slot & 31)
}

#[derive(Debug)]
pub struct ModelLightingCacheGlob<'a> {
    base_index: u32,
    xmodel_entry_limit: u32,
    rover: &'a mut u32,
    alloc_fail: &'a mut bool,
    origins: &'a mut [[f32; 3]],
    lighting_info: &'a mut [u16],
    prev_prev: &'a [u32],
    prev: &'a [u32],
    curr: &'a mut [u32],
}

impl<'a> ModelLightingCacheGlob<'a> {
    pub fn new(
        base_index: u32,
        xmodel_entry_limit: u32,
        rover: &'a mut u32,
        alloc_fail: &'a mut bool,
        origins: &'a mut [[f32; 3]],
        lighting_info: &'a mut [u16],
        prev_prev: &'a [u32],
        prev: &'a [u32],
        curr: &'a mut [u32],
    ) -> Result<Self, ModelLightingCacheGlobError> {
        if xmodel_entry_limit == 0
            || model_lighting_cache_handle(base_index, xmodel_entry_limit.saturating_sub(1))
                .is_none()
        {
            return Err(ModelLightingCacheGlobError::LimitsOutOfRange {
                base_index,
                xmodel_entry_limit,
            });
        }
        let needed = xmodel_entry_limit as usize;
        let words = model_lighting_cache_free_bits_words(xmodel_entry_limit);
        if origins.len() < needed || lighting_info.len() < needed {
            return Err(ModelLightingCacheGlobError::EntryArrayTooShort {
                needed,
                got: origins.len().min(lighting_info.len()),
            });
        }
        if prev_prev.len() < words || prev.len() < words || curr.len() < words {
            return Err(ModelLightingCacheGlobError::FreeBitsWordCountMismatch {
                needed: words,
                got: curr.len().min(prev.len()).min(prev_prev.len()),
            });
        }
        Ok(Self {
            base_index,
            xmodel_entry_limit,
            rover,
            alloc_fail,
            origins,
            lighting_info,
            prev_prev,
            prev,
            curr,
        })
    }

    #[inline]
    pub const fn warn_index_on_fail() -> u32 {
        MODEL_LIGHTING_WARN_CACHE_ALLOC_FAILED
    }

    pub fn alloc(
        &mut self,
        current_handle: u16,
        origin: [f32; 3],
        allow_moved_reuse: bool,
    ) -> ModelLightingCacheAlloc {
        if let Some(slot) = model_lighting_cache_slot(self.base_index, current_handle) {
            if slot < self.xmodel_entry_limit {
                let same = self.origins[slot as usize] == origin;

                let _ = allow_moved_reuse;
                if same {
                    let word = (slot >> 5) as usize;
                    let mask = model_lighting_cache_bit_mask(slot);
                    self.curr[word] &= !mask;
                    return ModelLightingCacheAlloc::Reused {
                        handle: current_handle,
                        info: self.lighting_info[slot as usize],
                    };
                }
            }
        }

        if *self.alloc_fail {
            return ModelLightingCacheAlloc::Failed;
        }

        let word_count = model_lighting_cache_free_bits_words(self.xmodel_entry_limit) as u32;
        if word_count == 0 {
            *self.alloc_fail = true;
            return ModelLightingCacheAlloc::Failed;
        }

        let start = *self.rover % word_count;
        let mut word_i = start;
        loop {
            let free = self.prev_prev[word_i as usize]
                & self.prev[word_i as usize]
                & self.curr[word_i as usize];
            if free != 0 {
                let high = 31u32 - free.leading_zeros();
                let bit = high ^ 0x1f;
                let slot = word_i * 32 + bit;
                if slot < self.xmodel_entry_limit {
                    let mask = model_lighting_cache_bit_mask(slot);
                    self.curr[word_i as usize] &= !mask;
                    *self.rover = word_i;
                    self.origins[slot as usize] = origin;
                    let Some(handle) = model_lighting_cache_handle(self.base_index, slot) else {
                        *self.alloc_fail = true;
                        return ModelLightingCacheAlloc::Failed;
                    };
                    return ModelLightingCacheAlloc::Assigned { handle, slot };
                }
            }
            word_i = (word_i + 1) % word_count;
            if word_i == start {
                break;
            }
        }

        *self.alloc_fail = true;
        ModelLightingCacheAlloc::Failed
    }
}
