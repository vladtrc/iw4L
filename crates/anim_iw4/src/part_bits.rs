#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct PartBits([u32; Self::WORDS]);

impl PartBits {
    pub const WORDS: usize = 6;

    pub const CAPACITY: usize = Self::WORDS * 32;

    pub const fn from_words(words: [u32; Self::WORDS]) -> Self {
        Self(words)
    }

    pub const fn words(&self) -> &[u32; Self::WORDS] {
        &self.0
    }

    pub fn set(&mut self, bone: usize) {
        if bone < Self::CAPACITY {
            self.0[bone / 32] |= 1 << (bone % 32);
        }
    }

    pub fn clear(&mut self, bone: usize) {
        if bone < Self::CAPACITY {
            self.0[bone / 32] &= !(1 << (bone % 32));
        }
    }

    pub fn get(&self, bone: usize) -> bool {
        bone < Self::CAPACITY && self.0[bone / 32] & (1 << (bone % 32)) != 0
    }

    pub fn count(&self) -> u32 {
        let mut n = 0u32;
        let mut i = 0;
        while i < Self::WORDS {
            n += self.0[i].count_ones();
            i += 1;
        }
        n
    }

    pub fn is_empty(&self) -> bool {
        let mut i = 0;
        while i < Self::WORDS {
            if self.0[i] != 0 {
                return false;
            }
            i += 1;
        }
        true
    }

    pub fn without(self, other: &PartBits) -> PartBits {
        let mut out = self;
        let mut i = 0;
        while i < Self::WORDS {
            out.0[i] &= !other.0[i];
            i += 1;
        }
        out
    }

    pub fn intersect(self, other: &PartBits) -> PartBits {
        let mut out = self;
        let mut i = 0;
        while i < Self::WORDS {
            out.0[i] &= other.0[i];
            i += 1;
        }
        out
    }

    pub fn union(self, other: &PartBits) -> PartBits {
        let mut out = self;
        let mut i = 0;
        while i < Self::WORDS {
            out.0[i] |= other.0[i];
            i += 1;
        }
        out
    }
}

pub fn xmodel_no_scale_bit(bits: &[u32; 6], index: usize) -> bool {
    index < PartBits::CAPACITY && bits[index / 32] & (0x8000_0000 >> (index % 32)) != 0
}
