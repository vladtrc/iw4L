use crate::iwi::{IWI_FLAG_NO_MIPMAPS, IwiHeader};

mod tables;

pub const WAVELET_ESCAPE: i16 = i16::MIN;

pub const WAVELET_BOOK_DETAIL_HEAD: [(i16, i16); 8] = [
    (12, 6),
    (0, 3),
    (10, 6),
    (7, 6),
    (4, 5),
    (2, 5),
    (9, 6),
    (1, 5),
];

pub const WAVELET_BOOK_CROSS_HEAD: [(i16, i16); 4] = [(30, 10), (2, 4), (1, 3), (0, 2)];

pub const WAVELET_BOOK_PLAIN_HEAD: [(i16, i16); 4] = [(WAVELET_ESCAPE, 4), (0, 1), (1, 4), (0, 1)];

pub const WAVELET_BOOK_DETAIL_TAIL: (i16, i16) = (-5, 6);
pub const WAVELET_BOOK_CROSS_TAIL: (i16, i16) = (0, 2);
pub const WAVELET_BOOK_PLAIN_TAIL: (i16, i16) = (0, 1);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WaveletError {
    Truncated { needed: usize, have: usize },
    Dimensions,
    UnsupportedFormat(u8),
    CubemapOrVolume,

    BadLanding { pos: usize, end: usize },
}

pub fn wavelet_top_level(header: &IwiHeader) -> u32 {
    if header.flags & IWI_FLAG_NO_MIPMAPS != 0 {
        return 0;
    }
    let largest = u32::from(header.width)
        .max(u32::from(header.height))
        .max(u32::from(header.depth));
    let mut level = 0u32;
    while (1u32 << level) < largest {
        level += 1;
    }
    level
}

#[inline]
pub fn wavelet_level_size(size: u32, level: u32) -> u32 {
    (size >> level).max(1)
}

pub fn wavelet_lift_block(
    out: &mut [u8],
    at: usize,
    parent: u8,
    detail: [i32; 3],
    carry: u8,
    stride: usize,
    row: usize,
) {
    let a = i32::from(parent) * 2;
    let d0 = detail[0];
    let d1 = detail[1];
    let d2 = detail[2];
    out[at] = (((a + d2 + d1 + d0) >> 1) as u8).wrapping_add(carry);
    out[at + stride] = ((a + d0 - d1 - d2) >> 1) as u8;
    out[at + row] = ((a + d1 - d0 - d2) >> 1) as u8;
    out[at + row + stride] = ((a + d2 - d0 - d1) >> 1) as u8;
}

#[derive(Clone, Copy)]
struct Code {
    value: i16,
    bits: u8,
}

const fn lookup(codes: &[(u8, u16, i16)]) -> [Code; 4096] {
    let mut table = [Code { value: 0, bits: 0 }; 4096];
    let mut i = 0;
    while i < codes.len() {
        let (bits, code, value) = codes[i];
        let mut index = code as usize;
        while index < 4096 {
            table[index] = Code { value, bits };
            index += 1 << bits;
        }
        i += 1;
    }
    table
}

static DETAIL: [Code; 4096] = lookup(&tables::DETAIL);
static CROSS: [Code; 4096] = lookup(&tables::CROSS);
static PLAIN: [Code; 4096] = lookup(&tables::PLAIN);

pub struct WaveletBits<'a> {
    data: &'a [u8],
    pos: usize,
    register: u16,
    index: u32,
    primed: bool,
}

impl<'a> WaveletBits<'a> {
    pub fn new(data: &'a [u8]) -> Self {
        Self {
            data,
            pos: 0,
            register: 0,
            index: 0,
            primed: false,
        }
    }

    pub fn pos(&self) -> usize {
        self.pos
    }

    fn byte(&mut self) -> Result<u8, WaveletError> {
        let byte = *self.data.get(self.pos).ok_or(WaveletError::Truncated {
            needed: self.pos + 1,
            have: self.data.len(),
        })?;
        self.pos += 1;
        Ok(byte)
    }

    fn dword(&self) -> u32 {
        let mut word = [0u8; 4];
        let end = (self.pos + 4).min(self.data.len());
        if self.pos < end {
            word[..end - self.pos].copy_from_slice(&self.data[self.pos..end]);
        }
        u32::from_le_bytes(word)
    }

    fn advance(&mut self, n: u32) {
        debug_assert!((1..=12).contains(&n));
        let fresh = self.dword() >> self.index;
        self.register = ((fresh << (16 - n)) as u16) | (self.register >> n);
        let index = self.index + n;
        self.pos += (index >> 3) as usize;
        self.index = index & 7;
    }

    fn prime(&mut self) -> Result<(), WaveletError> {
        if !self.primed {
            let lo = self.byte()?;
            let hi = self.byte()?;
            self.register = u16::from_le_bytes([lo, hi]);
            self.index = 0;
            self.primed = true;
        }
        Ok(())
    }

    fn bit(&mut self) -> u8 {
        let bit = (self.register & 1) as u8;
        self.advance(1);
        bit
    }

    fn symbol(&mut self, table: &[Code; 4096], raw: u32, bias: i32) -> i32 {
        let code = table[(self.register & 0xfff) as usize];
        self.advance(u32::from(code.bits));
        if code.value == WAVELET_ESCAPE {
            let literal = i32::from(self.register) & ((1 << raw) - 1);
            self.advance(raw);
            literal - bias
        } else {
            i32::from(code.value)
        }
    }

    pub fn check_landing(&self) -> Result<(), WaveletError> {
        let end = self.data.len();
        if self.pos > end + 2 || self.pos + 16 < end {
            return Err(WaveletError::BadLanding { pos: self.pos, end });
        }
        Ok(())
    }
}

pub fn wavelet_decompress_level(
    bits: &mut WaveletBits,
    parent: &mut [u8],
    out: &mut [u8],
    width: u32,
    height: u32,
    channels: u8,
    stride: usize,
) -> Result<(), WaveletError> {
    let channels_n = channels as usize;
    if !(1..=4).contains(&channels_n) || stride < channels_n {
        return Err(WaveletError::Dimensions);
    }
    if width < 2 || height < 2 {
        let w = width.max(1) as usize;
        let h = height.max(1) as usize;
        if out.len() < w * h * stride {
            return Err(WaveletError::Truncated {
                needed: w * h * stride,
                have: out.len(),
            });
        }
        for pixel in out.chunks_exact_mut(stride).take(w * h) {
            for plane in &mut pixel[..channels_n] {
                *plane = bits.byte()?;
            }
            if stride != channels_n {
                pixel[3] = 0xff;
            }
        }
        return Ok(());
    }
    if width % 2 != 0 || height % 2 != 0 {
        return Err(WaveletError::Dimensions);
    }
    let (blocks_x, blocks_y) = (width as usize / 2, height as usize / 2);
    let needed = blocks_x * blocks_y * stride;
    if parent.len() < needed {
        return Err(WaveletError::Truncated {
            needed,
            have: parent.len(),
        });
    }
    if out.len() < width as usize * height as usize * stride {
        return Err(WaveletError::Truncated {
            needed: width as usize * height as usize * stride,
            have: out.len(),
        });
    }

    bits.prime()?;

    if bits.bit() != 0 {
        for pixel in parent.chunks_exact_mut(stride).take(blocks_x * blocks_y) {
            for plane in &mut pixel[..channels_n] {
                let residual = bits.symbol(&PLAIN, 9, 0xff);
                *plane = plane.wrapping_add(residual as u8);
            }
        }
    }

    let row = width as usize * stride;
    let (mut src, mut dst) = (0usize, 0usize);
    for _ in 0..blocks_y {
        for _ in 0..blocks_x {
            if channels != 1 {
                let carry = bits.bit();
                let detail = [
                    bits.symbol(&DETAIL, 9, 0xff),
                    bits.symbol(&DETAIL, 9, 0xff),
                    bits.symbol(&DETAIL, 9, 0xff),
                ];
                wavelet_lift_block(out, dst, parent[src], detail, carry, stride, row);
                if channels > 2 {
                    for plane in 1..3 {
                        let carry = bits.bit();
                        let delta = [
                            detail[0] + bits.symbol(&CROSS, 10, 0x1fe),
                            detail[1] + bits.symbol(&CROSS, 10, 0x1fe),
                            detail[2] + bits.symbol(&CROSS, 10, 0x1fe),
                        ];
                        wavelet_lift_block(
                            out,
                            dst + plane,
                            parent[src + plane],
                            delta,
                            carry,
                            stride,
                            row,
                        );
                    }
                }
            }
            if channels == 3 {
                if stride != 3 {
                    for offset in [0, stride, row, row + stride] {
                        out[dst + 3 + offset] = 0xff;
                    }
                }
            } else {
                let carry = bits.bit();
                let last = [
                    bits.symbol(&PLAIN, 9, 0xff),
                    bits.symbol(&PLAIN, 9, 0xff),
                    bits.symbol(&PLAIN, 9, 0xff),
                ];
                let at = dst + channels_n - 1;
                wavelet_lift_block(
                    out,
                    at,
                    parent[src + channels_n - 1],
                    last,
                    carry,
                    stride,
                    row,
                );
            }
            src += stride;
            dst += stride * 2;
        }
        dst += row;
    }
    Ok(())
}

pub fn wavelet_check_header(header: &IwiHeader) -> Result<u8, WaveletError> {
    if header.is_skybox() || header.depth > 1 {
        return Err(WaveletError::CubemapOrVolume);
    }
    match header.format {
        6..=10 => Ok(header.format),
        other => Err(WaveletError::UnsupportedFormat(other)),
    }
}
