#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WireError {
    Truncated { needed: usize, available: usize },

    UnknownField(usize),

    Malformed(&'static str),
}

impl core::fmt::Display for WireError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            WireError::Truncated { needed, available } => write!(
                f,
                "stream ended inside a value: needed {needed} bytes, {available} available"
            ),
            WireError::UnknownField(index) => write!(
                f,
                "field index {index} is not in this build's playerState registry"
            ),
            WireError::Malformed(what) => write!(f, "malformed frame: {what}"),
        }
    }
}

impl std::error::Error for WireError {}

#[derive(Debug, Default)]
pub struct WireWriter {
    bytes: Vec<u8>,
}

impl WireWriter {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_capacity(capacity: usize) -> Self {
        Self {
            bytes: Vec::with_capacity(capacity),
        }
    }

    pub fn put_u8(&mut self, value: u8) {
        self.bytes.push(value);
    }

    pub fn put_u16(&mut self, value: u16) {
        self.bytes.extend_from_slice(&value.to_le_bytes());
    }

    pub fn put_u32(&mut self, value: u32) {
        self.bytes.extend_from_slice(&value.to_le_bytes());
    }

    pub fn put_u64(&mut self, value: u64) {
        self.bytes.extend_from_slice(&value.to_le_bytes());
    }

    pub fn put_i32(&mut self, value: i32) {
        self.bytes.extend_from_slice(&value.to_le_bytes());
    }

    pub fn put_i8(&mut self, value: i8) {
        self.bytes.push(value as u8);
    }

    pub fn put_f32(&mut self, value: f32) {
        self.put_u32(value.to_bits());
    }

    pub fn put_bytes(&mut self, value: &[u8]) {
        self.bytes.extend_from_slice(value);
    }

    pub fn len(&self) -> usize {
        self.bytes.len()
    }

    pub fn is_empty(&self) -> bool {
        self.bytes.is_empty()
    }

    pub fn finish(self) -> Vec<u8> {
        self.bytes
    }

    pub fn as_slice(&self) -> &[u8] {
        &self.bytes
    }
}

#[derive(Debug)]
pub struct WireReader<'a> {
    bytes: &'a [u8],
    cursor: usize,
}

impl<'a> WireReader<'a> {
    pub fn new(bytes: &'a [u8]) -> Self {
        Self { bytes, cursor: 0 }
    }

    pub fn remaining(&self) -> usize {
        self.bytes.len() - self.cursor
    }

    pub fn is_empty(&self) -> bool {
        self.remaining() == 0
    }

    fn take(&mut self, count: usize) -> Result<&'a [u8], WireError> {
        if self.remaining() < count {
            return Err(WireError::Truncated {
                needed: count,
                available: self.remaining(),
            });
        }
        let slice = &self.bytes[self.cursor..self.cursor + count];
        self.cursor += count;
        Ok(slice)
    }

    pub fn get_u8(&mut self) -> Result<u8, WireError> {
        Ok(self.take(1)?[0])
    }

    pub fn get_i8(&mut self) -> Result<i8, WireError> {
        Ok(self.take(1)?[0] as i8)
    }

    pub fn get_u16(&mut self) -> Result<u16, WireError> {
        let bytes = self.take(2)?;
        Ok(u16::from_le_bytes([bytes[0], bytes[1]]))
    }

    pub fn get_u32(&mut self) -> Result<u32, WireError> {
        let bytes = self.take(4)?;
        Ok(u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]))
    }

    pub fn get_u64(&mut self) -> Result<u64, WireError> {
        let bytes = self.take(8)?;
        Ok(u64::from_le_bytes([
            bytes[0], bytes[1], bytes[2], bytes[3], bytes[4], bytes[5], bytes[6], bytes[7],
        ]))
    }

    pub fn get_i32(&mut self) -> Result<i32, WireError> {
        Ok(self.get_u32()? as i32)
    }

    pub fn get_f32(&mut self) -> Result<f32, WireError> {
        Ok(f32::from_bits(self.get_u32()?))
    }

    pub fn get_bytes(&mut self, out: &mut [u8]) -> Result<(), WireError> {
        let slice = self.take(out.len())?;
        out.copy_from_slice(slice);
        Ok(())
    }
}
