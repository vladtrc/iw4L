pub(crate) struct Bits<'a> {
    data: &'a [u8],
    pos: usize,
}

impl<'a> Bits<'a> {
    pub(crate) fn new(data: &'a [u8]) -> Self {
        Self { data, pos: 0 }
    }

    pub(crate) fn pos(&self) -> usize {
        self.pos
    }

    pub(crate) fn len(&self) -> usize {
        self.data.len() * 8
    }

    pub(crate) fn left(&self) -> isize {
        self.len() as isize - self.pos as isize
    }

    pub(crate) fn overrun(&self) -> bool {
        self.pos > self.len()
    }

    pub(crate) fn peek(&self, n: u32) -> u32 {
        debug_assert!(n <= 32);
        if n == 0 {
            return 0;
        }
        let byte = self.pos >> 3;
        let mut word = [0u8; 8];
        if byte < self.data.len() {
            let end = (byte + 8).min(self.data.len());
            word[..end - byte].copy_from_slice(&self.data[byte..end]);
        }
        let v = u64::from_le_bytes(word) >> (self.pos & 7);
        (v & ((1u64 << n) - 1)) as u32
    }

    pub(crate) fn skip(&mut self, n: usize) {
        self.pos += n;
    }

    pub(crate) fn read(&mut self, n: u32) -> u32 {
        let v = self.peek(n);
        self.pos += n as usize;
        v
    }

    pub(crate) fn bit(&mut self) -> bool {
        self.read(1) != 0
    }

    pub(crate) fn align32(&mut self) {
        self.pos = (self.pos + 31) & !31;
    }
}
