#[derive(Debug)]
pub struct VisBits<'a> {
    words: &'a mut [u32],
}

impl<'a> VisBits<'a> {
    pub fn new(words: &'a mut [u32]) -> Self {
        Self { words }
    }

    pub fn clear(&mut self) {
        for w in self.words.iter_mut() {
            *w = 0;
        }
    }

    pub fn set(&mut self, index: usize) {
        let word = index / 32;
        let bit = index % 32;
        if let Some(w) = self.words.get_mut(word) {
            *w |= 1u32 << bit;
        }
    }

    pub fn get(&self, index: usize) -> bool {
        let word = index / 32;
        let bit = index % 32;
        self.words
            .get(word)
            .is_some_and(|w| (*w & (1u32 << bit)) != 0)
    }

    pub fn count_ones(&self) -> u32 {
        self.words.iter().map(|w| w.count_ones()).sum()
    }
}

pub fn words_for_bits(count: usize) -> usize {
    count.div_ceil(32)
}

#[derive(Debug)]
pub struct MsbBits<'a> {
    words: &'a mut [u32],
}

impl<'a> MsbBits<'a> {
    pub fn new(words: &'a mut [u32]) -> Self {
        Self { words }
    }

    pub fn clear(&mut self) {
        for w in self.words.iter_mut() {
            *w = 0;
        }
    }

    pub fn set(&mut self, index: usize) {
        msb_set(self.words, index);
    }

    pub fn get(&self, index: usize) -> bool {
        msb_get(self.words, index)
    }

    pub fn count_ones(&self) -> u32 {
        self.words.iter().map(|w| w.count_ones()).sum()
    }

    pub fn iter_set(&self, bit_count: usize) -> MsbBitIter<'_> {
        msb_iter(self.words, bit_count)
    }
}

#[inline]
pub fn msb_set(words: &mut [u32], index: usize) {
    let word = index >> 5;
    let shift = (index & 31) as u32;
    if let Some(w) = words.get_mut(word) {
        *w |= 0x8000_0000u32 >> shift;
    }
}

#[inline]
pub fn msb_get(words: &[u32], index: usize) -> bool {
    let word = index >> 5;
    let shift = (index & 31) as u32;
    words
        .get(word)
        .is_some_and(|w| (*w & (0x8000_0000u32 >> shift)) != 0)
}

pub fn msb_iter(words: &[u32], bit_count: usize) -> MsbBitIter<'_> {
    let remaining = words.first().copied().unwrap_or(0);
    MsbBitIter {
        words,
        bit_count,
        word: 0,
        remaining,
        started: !words.is_empty(),
    }
}

#[derive(Clone, Copy, Debug)]
pub struct MsbBitIter<'a> {
    words: &'a [u32],
    bit_count: usize,
    word: usize,
    remaining: u32,
    started: bool,
}

impl Iterator for MsbBitIter<'_> {
    type Item = usize;

    fn next(&mut self) -> Option<usize> {
        if !self.started {
            return None;
        }
        loop {
            if self.remaining != 0 {
                let bit = self.remaining.leading_zeros() as usize;
                self.remaining &= !(0x8000_0000u32 >> bit);
                let index = self.word * 32 + bit;
                if index < self.bit_count {
                    return Some(index);
                }
                continue;
            }
            self.word = self.word.saturating_add(1);
            if self.word >= self.words.len() || self.word * 32 >= self.bit_count {
                return None;
            }
            self.remaining = self.words[self.word];
        }
    }
}
