use dpvs_iw4::{VisBits, words_for_bits};

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SurfaceCastsSunShadow {
    words: Vec<u32>,
    len: usize,
}

impl SurfaceCastsSunShadow {
    pub fn with_len(len: usize) -> Self {
        Self {
            words: vec![0; words_for_bits(len)],
            len,
        }
    }

    pub fn len(&self) -> usize {
        self.len
    }

    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    pub fn get(&self, index: usize) -> bool {
        if index >= self.len {
            return false;
        }
        let word = index / 32;
        let bit = index % 32;
        self.words
            .get(word)
            .is_some_and(|w| (*w & (1u32 << bit)) != 0)
    }

    pub fn set(&mut self, index: usize) {
        if index >= self.len {
            return;
        }
        VisBits::new(&mut self.words).set(index);
    }

    pub fn words(&self) -> &[u32] {
        &self.words
    }
}
