use std::sync::OnceLock;

use crate::Error;
use crate::bits::Bits;
use crate::tables::{INTER_QUANT, INTRA_QUANT, PATTERNS, SCAN, TREE_BITS, TREE_LENS};

const SKIP: i32 = 0;
const SCALED: i32 = 1;
const MOTION: i32 = 2;
const RUN: i32 = 3;
const RESIDUE: i32 = 4;
const INTRA: i32 = 5;
const FILL: i32 = 6;
const INTER: i32 = 7;
const PATTERN: i32 = 8;
const RAW: i32 = 9;

const B_BLOCK_TYPES: usize = 0;
const B_SUB_BLOCK_TYPES: usize = 1;
const B_COLORS: usize = 2;
const B_PATTERN: usize = 3;
const B_X_OFF: usize = 4;
const B_Y_OFF: usize = 5;
const B_INTRA_DC: usize = 6;
const B_INTER_DC: usize = 7;
const B_RUN: usize = 8;
const BUNDLES: usize = 9;

const RLE_LENS: [usize; 4] = [4, 8, 12, 32];
const DC_START_BITS: u32 = 11;

struct Vlc {
    bits: u32,
    table: Vec<(u8, u8)>,
}

fn vlcs() -> &'static [Vlc; 16] {
    static VLCS: OnceLock<[Vlc; 16]> = OnceLock::new();
    VLCS.get_or_init(|| {
        std::array::from_fn(|i| {
            let bits = u32::from(TREE_LENS[i][15]);
            let mut table = vec![(0u8, 0u8); 1 << bits];
            for sym in 0..16 {
                let len = u32::from(TREE_LENS[i][sym]);
                let code = usize::from(TREE_BITS[i][sym]);
                for hi in 0..1usize << (bits - len) {
                    table[code | (hi << len)] = (sym as u8, len as u8);
                }
            }
            Vlc { bits, table }
        })
    })
}

#[derive(Clone, Copy)]
struct Tree {
    vlc: usize,
    syms: [u8; 16],
}

impl Default for Tree {
    fn default() -> Self {
        Self {
            vlc: 0,
            syms: std::array::from_fn(|i| i as u8),
        }
    }
}

impl Tree {
    fn read(b: &mut Bits) -> Self {
        let mut tree = Self {
            vlc: b.read(4) as usize,
            ..Self::default()
        };
        if tree.vlc == 0 {
            return tree;
        }
        if b.bit() {
            let mut len = b.read(3) as usize;
            let mut used = [false; 16];
            for i in 0..=len {
                let s = b.read(4) as u8;
                tree.syms[i] = s;
                used[usize::from(s)] = true;
            }
            for i in 0..16u8 {
                if len >= 15 {
                    break;
                }
                if !used[usize::from(i)] {
                    len += 1;
                    tree.syms[len] = i;
                }
            }
        } else {
            let depth = b.read(2);
            let mut input: [u8; 16] = std::array::from_fn(|i| i as u8);
            let mut output = [0u8; 16];
            for i in 0..=depth {
                let size = 1usize << i;
                for t in (0..16).step_by(size * 2) {
                    merge(
                        b,
                        &mut output[t..t + size * 2],
                        &input[t..t + size * 2],
                        size,
                    );
                }
                std::mem::swap(&mut input, &mut output);
            }
            tree.syms = input;
        }
        tree
    }

    fn get(&self, b: &mut Bits) -> u8 {
        let vlc = &vlcs()[self.vlc];
        let (sym, len) = vlc.table[b.peek(vlc.bits) as usize];
        b.skip(usize::from(len));
        self.syms[usize::from(sym)]
    }
}

fn merge(b: &mut Bits, dst: &mut [u8], src: &[u8], size: usize) {
    let (mut i1, mut i2, mut o) = (0, size, 0);
    while i1 < size && i2 < size * 2 {
        if b.bit() {
            dst[o] = src[i2];
            i2 += 1;
        } else {
            dst[o] = src[i1];
            i1 += 1;
        }
        o += 1;
    }
    for &v in src[i1..size].iter().chain(&src[i2..size * 2]) {
        dst[o] = v;
        o += 1;
    }
}

#[derive(Default)]
struct Bundle {
    len_bits: u32,
    tree: Tree,
    data: Vec<i16>,
    dec: usize,
    ptr: usize,
    done: bool,
}

impl Bundle {
    fn reset(&mut self, len_bits: u32) {
        self.len_bits = len_bits;
        self.data.resize(1 << len_bits, 0);
        self.dec = 0;
        self.ptr = 0;
        self.done = false;
    }

    fn chunk(&mut self, b: &mut Bits) -> Option<usize> {
        if self.done || self.dec > self.ptr {
            return None;
        }
        let t = b.read(self.len_bits) as usize;
        if t == 0 {
            self.done = true;
            return None;
        }
        self.dec = 0;
        self.ptr = 0;
        Some(t)
    }

    fn push(&mut self, v: i32) -> Result<(), Error> {
        let slot = self
            .data
            .get_mut(self.dec)
            .ok_or(Error::Format("bundle overflow"))?;
        *slot = v as i16;
        self.dec += 1;
        Ok(())
    }

    fn fill(&mut self, v: i32, n: usize) -> Result<(), Error> {
        let end = self.dec + n;
        self.data
            .get_mut(self.dec..end)
            .ok_or(Error::Format("bundle overflow"))?
            .fill(v as i16);
        self.dec = end;
        Ok(())
    }

    fn next(&mut self) -> i32 {
        let v = self.data.get(self.ptr).copied().unwrap_or(0);
        self.ptr += 1;
        i32::from(v)
    }
}

fn signed(b: &mut Bits, v: i32) -> i32 {
    if v != 0 && b.bit() { -v } else { v }
}

#[derive(Clone, Default)]
pub(crate) struct Plane {
    pub(crate) stride: usize,
    pub(crate) data: Vec<u8>,
}

impl Plane {
    fn new(blocks_w: usize, blocks_h: usize) -> Self {
        let stride = (blocks_w * 8).next_multiple_of(16);
        let rows = (blocks_h * 8).next_multiple_of(16);
        Self {
            stride,
            data: vec![0; stride * rows],
        }
    }
}

pub(crate) struct Video {
    width: usize,
    height: usize,
    swap_planes: bool,
    rev_i: bool,
    has_alpha: bool,
    bundles: [Bundle; BUNDLES],
    col_high: [Tree; 16],
    col_last: usize,
    pub(crate) cur: Vec<Plane>,
    prev: Vec<Plane>,
}

impl Video {
    pub(crate) fn new(width: usize, height: usize, revision: u8, has_alpha: bool) -> Self {
        let planes = |_| -> Vec<Plane> {
            let luma = Plane::new(width.div_ceil(8), height.div_ceil(8));
            let chroma = Plane::new(width.div_ceil(16), height.div_ceil(16));
            vec![luma.clone(), chroma.clone(), chroma, luma]
        };
        Self {
            width,
            height,
            swap_planes: revision >= b'h',
            rev_i: revision >= b'i',
            has_alpha,
            bundles: Default::default(),
            col_high: [Tree::default(); 16],
            col_last: 0,
            cur: planes(()),
            prev: planes(()),
        }
    }

    pub(crate) fn decode(&mut self, packet: &[u8]) -> Result<(), Error> {
        std::mem::swap(&mut self.cur, &mut self.prev);
        let mut b = Bits::new(packet);
        if self.has_alpha {
            if self.rev_i {
                b.skip(32);
            }
            self.plane(&mut b, 3, false)?;
        }
        if self.rev_i {
            b.skip(32);
        }
        for plane in 0..3 {
            let idx = if plane == 0 || !self.swap_planes {
                plane
            } else {
                plane ^ 3
            };
            self.plane(&mut b, idx, plane != 0)?;
            if b.pos() >= b.len() {
                break;
            }
        }
        Ok(())
    }

    fn read_bundle_headers(&mut self, b: &mut Bits, width: usize, bw: usize) {
        let log = |v: usize| v.ilog2() + 1;
        let width = width.next_multiple_of(8);
        let lens = [
            log((width >> 3) + 511),
            log((width >> 4) + 511),
            log(bw * 64 + 511),
            log((bw << 3) + 511),
            log((width >> 3) + 511),
            log((width >> 3) + 511),
            log((width >> 3) + 511),
            log((width >> 3) + 511),
            log(bw * 48 + 511),
        ];
        for (i, len) in lens.into_iter().enumerate() {
            if i == B_COLORS {
                for t in &mut self.col_high {
                    *t = Tree::read(b);
                }
                self.col_last = 0;
            }
            let bundle = &mut self.bundles[i];
            if i != B_INTRA_DC && i != B_INTER_DC {
                bundle.tree = Tree::read(b);
            }
            bundle.reset(len);
        }
    }

    fn read_block_types(&mut self, b: &mut Bits, which: usize) -> Result<(), Error> {
        let bundle = &mut self.bundles[which];
        let Some(t) = bundle.chunk(b) else {
            return Ok(());
        };
        if b.bit() {
            return bundle.fill(b.read(4) as i32, t);
        }
        let mut last = 0;
        while bundle.dec < t {
            let v = i32::from(bundle.tree.get(b));
            if v < 12 {
                last = v;
                bundle.push(v)?;
            } else {
                let run = RLE_LENS[(v - 12) as usize];
                if t - bundle.dec < run {
                    return Err(Error::Format("block type run overflow"));
                }
                bundle.fill(last, run)?;
            }
        }
        Ok(())
    }

    fn color(&mut self, b: &mut Bits) -> i32 {
        self.col_last = usize::from(self.col_high[self.col_last].get(b));
        let v = (self.col_last << 4) as i32 | i32::from(self.bundles[B_COLORS].tree.get(b));
        if self.rev_i {
            return v;
        }
        let m = v & 0x7f;
        if v & 0x80 != 0 { 0x80 - m } else { 0x80 + m }
    }

    fn read_colors(&mut self, b: &mut Bits) -> Result<(), Error> {
        let Some(t) = self.bundles[B_COLORS].chunk(b) else {
            return Ok(());
        };
        if b.bit() {
            let v = self.color(b);
            return self.bundles[B_COLORS].fill(v, t);
        }
        while self.bundles[B_COLORS].dec < t {
            if b.overrun() {
                return Err(Error::Format("color bundle truncated"));
            }
            let v = self.color(b);
            self.bundles[B_COLORS].push(v)?;
        }
        Ok(())
    }

    fn read_patterns(&mut self, b: &mut Bits) -> Result<(), Error> {
        let bundle = &mut self.bundles[B_PATTERN];
        let Some(t) = bundle.chunk(b) else {
            return Ok(());
        };
        while bundle.dec < t {
            if b.overrun() {
                return Err(Error::Format("pattern bundle truncated"));
            }
            let lo = i32::from(bundle.tree.get(b));
            let hi = i32::from(bundle.tree.get(b));
            bundle.push(lo | (hi << 4))?;
        }
        Ok(())
    }

    fn read_motion(&mut self, b: &mut Bits, which: usize) -> Result<(), Error> {
        let bundle = &mut self.bundles[which];
        let Some(t) = bundle.chunk(b) else {
            return Ok(());
        };
        if b.bit() {
            let v = b.read(4) as i32;
            let v = signed(b, v);
            return bundle.fill(v, t);
        }
        while bundle.dec < t {
            let v = i32::from(bundle.tree.get(b));
            let v = signed(b, v);
            bundle.push(v)?;
        }
        Ok(())
    }

    fn read_dcs(&mut self, b: &mut Bits, which: usize, has_sign: bool) -> Result<(), Error> {
        let bundle = &mut self.bundles[which];
        let Some(len) = bundle.chunk(b) else {
            return Ok(());
        };
        let mut v = b.read(DC_START_BITS - u32::from(has_sign)) as i32;
        if has_sign {
            v = signed(b, v);
        }
        bundle.push(v)?;
        let rest = len - 1;
        for i in (0..rest).step_by(8) {
            let n = (rest - i).min(8);
            let size = b.read(4);
            for _ in 0..n {
                if size != 0 {
                    let d = b.read(size) as i32;
                    v += signed(b, d);
                    if !(-32768..=32767).contains(&v) {
                        return Err(Error::Format("DC out of range"));
                    }
                }
                bundle.push(v)?;
            }
        }
        Ok(())
    }

    fn read_runs(&mut self, b: &mut Bits) -> Result<(), Error> {
        let bundle = &mut self.bundles[B_RUN];
        let Some(t) = bundle.chunk(b) else {
            return Ok(());
        };
        if b.bit() {
            return bundle.fill(b.read(4) as i32, t);
        }
        while bundle.dec < t {
            let v = i32::from(bundle.tree.get(b));
            bundle.push(v)?;
        }
        Ok(())
    }

    fn plane(&mut self, b: &mut Bits, idx: usize, chroma: bool) -> Result<(), Error> {
        let shift = usize::from(chroma);
        let (bw, bh) = if chroma {
            (self.width.div_ceil(16), self.height.div_ceil(16))
        } else {
            (self.width.div_ceil(8), self.height.div_ceil(8))
        };
        self.read_bundle_headers(b, (self.width >> shift).max(8), bw);

        let mut dst = std::mem::take(&mut self.cur[idx]);
        let prev = std::mem::take(&mut self.prev[idx]);
        let result = self.blocks(b, &mut dst, &prev, bw, bh);
        self.cur[idx] = dst;
        self.prev[idx] = prev;
        result?;
        b.align32();
        Ok(())
    }

    fn blocks(
        &mut self,
        b: &mut Bits,
        dst: &mut Plane,
        prev: &Plane,
        bw: usize,
        bh: usize,
    ) -> Result<(), Error> {
        let stride = dst.stride;
        let ref_end = ((bw - 1) + stride * (bh - 1)) * 8;
        let mut ublock = [0u8; 64];
        for by in 0..bh {
            self.read_block_types(b, B_BLOCK_TYPES)?;
            self.read_block_types(b, B_SUB_BLOCK_TYPES)?;
            self.read_colors(b)?;
            self.read_patterns(b)?;
            self.read_motion(b, B_X_OFF)?;
            self.read_motion(b, B_Y_OFF)?;
            self.read_dcs(b, B_INTRA_DC, false)?;
            self.read_dcs(b, B_INTER_DC, true)?;
            self.read_runs(b)?;
            if b.overrun() {
                return Err(Error::Format("video packet truncated"));
            }

            let mut bx = 0;
            while bx < bw {
                let off = by * 8 * stride + bx * 8;
                let blk = self.bundles[B_BLOCK_TYPES].next();
                if (by & 1 != 0 || bx & 1 != 0) && blk == SCALED {
                    bx += 2;
                    continue;
                }
                match blk {
                    SKIP => copy8(&mut dst.data, off, &prev.data, off, stride),
                    SCALED => {
                        let sub = self.bundles[B_SUB_BLOCK_TYPES].next();
                        match sub {
                            RUN => {
                                let scan = &PATTERNS[b.read(4) as usize];
                                self.runs(b, scan, |i, v| ublock[usize::from(i)] = v)?;
                            }
                            INTRA => {
                                let mut coeffs = self.dct(b, B_INTRA_DC, &INTRA_QUANT)?;
                                idct(&mut coeffs);
                                for (u, &c) in ublock.iter_mut().zip(&coeffs) {
                                    *u = c as u8;
                                }
                            }
                            FILL => {
                                let v = self.bundles[B_COLORS].next() as u8;
                                for row in 0..16 {
                                    dst.data[off + row * stride..][..16].fill(v);
                                }
                            }
                            PATTERN => self.pattern(|i, v| ublock[i] = v),
                            RAW => {
                                for u in &mut ublock {
                                    *u = self.bundles[B_COLORS].next() as u8;
                                }
                            }
                            _ => return Err(Error::Format("bad 16x16 block type")),
                        }
                        if sub != FILL {
                            for (row, src) in ublock.as_chunks::<8>().0.iter().enumerate() {
                                for line in 0..2 {
                                    let d = &mut dst.data[off + (row * 2 + line) * stride..][..16];
                                    for (pair, &v) in d.as_chunks_mut::<2>().0.iter_mut().zip(src) {
                                        pair.fill(v);
                                    }
                                }
                            }
                        }
                        bx += 1;
                    }
                    MOTION => self.motion(dst, prev, off, ref_end)?,
                    RUN => {
                        let scan = &PATTERNS[b.read(4) as usize];
                        let data = &mut dst.data;
                        self.runs(b, scan, |i, v| {
                            data[off + usize::from(i >> 3) * stride + usize::from(i & 7)] = v;
                        })?;
                    }
                    RESIDUE => {
                        self.motion(dst, prev, off, ref_end)?;
                        let masks = b.read(7) as i32;
                        let mut block = [0i16; 64];
                        residue(b, &mut block, masks)?;
                        for (row, r) in block.as_chunks::<8>().0.iter().enumerate() {
                            let d = &mut dst.data[off + row * stride..][..8];
                            for (p, &v) in d.iter_mut().zip(r) {
                                *p = p.wrapping_add(v as u8);
                            }
                        }
                    }
                    INTRA => {
                        let mut coeffs = self.dct(b, B_INTRA_DC, &INTRA_QUANT)?;
                        idct(&mut coeffs);
                        for (row, r) in coeffs.as_chunks::<8>().0.iter().enumerate() {
                            for (p, &v) in dst.data[off + row * stride..][..8].iter_mut().zip(r) {
                                *p = v as u8;
                            }
                        }
                    }
                    FILL => {
                        let v = self.bundles[B_COLORS].next() as u8;
                        for row in 0..8 {
                            dst.data[off + row * stride..][..8].fill(v);
                        }
                    }
                    INTER => {
                        self.motion(dst, prev, off, ref_end)?;
                        let mut coeffs = self.dct(b, B_INTER_DC, &INTER_QUANT)?;
                        idct(&mut coeffs);
                        for (row, r) in coeffs.as_chunks::<8>().0.iter().enumerate() {
                            for (p, &v) in dst.data[off + row * stride..][..8].iter_mut().zip(r) {
                                *p = p.wrapping_add(v as u8);
                            }
                        }
                    }
                    PATTERN => {
                        let data = &mut dst.data;
                        self.pattern(|i, v| data[off + (i >> 3) * stride + (i & 7)] = v);
                    }
                    RAW => {
                        for row in 0..8 {
                            for p in &mut dst.data[off + row * stride..][..8] {
                                *p = self.bundles[B_COLORS].next() as u8;
                            }
                        }
                    }
                    _ => return Err(Error::Format("bad block type")),
                }
                bx += 1;
            }
        }
        Ok(())
    }

    fn motion(
        &mut self,
        dst: &mut Plane,
        prev: &Plane,
        off: usize,
        ref_end: usize,
    ) -> Result<(), Error> {
        let x = self.bundles[B_X_OFF].next() as isize;
        let y = self.bundles[B_Y_OFF].next() as isize;
        let src = off as isize + x + y * dst.stride as isize;
        if src < 0 || src as usize > ref_end {
            return Err(Error::Format("motion vector out of bounds"));
        }
        copy8(&mut dst.data, off, &prev.data, src as usize, dst.stride);
        Ok(())
    }

    fn runs(
        &mut self,
        b: &mut Bits,
        scan: &[u8; 64],
        mut put: impl FnMut(u8, u8),
    ) -> Result<(), Error> {
        let mut i = 0;
        let mut s = 0;
        loop {
            let run = (self.bundles[B_RUN].next() + 1) as usize;
            i += run;
            if i > 64 {
                return Err(Error::Format("run out of bounds"));
            }
            if b.bit() {
                let v = self.bundles[B_COLORS].next() as u8;
                for _ in 0..run {
                    put(scan[s], v);
                    s += 1;
                }
            } else {
                for _ in 0..run {
                    put(scan[s], self.bundles[B_COLORS].next() as u8);
                    s += 1;
                }
            }
            if i >= 63 {
                break;
            }
        }
        if i == 63 {
            put(scan[s], self.bundles[B_COLORS].next() as u8);
        }
        Ok(())
    }

    fn pattern(&mut self, mut put: impl FnMut(usize, u8)) {
        let col = [
            self.bundles[B_COLORS].next() as u8,
            self.bundles[B_COLORS].next() as u8,
        ];
        for row in 0..8 {
            let mut v = self.bundles[B_PATTERN].next();
            for x in 0..8 {
                put(row * 8 + x, col[(v & 1) as usize]);
                v >>= 1;
            }
        }
    }

    fn dct(
        &mut self,
        b: &mut Bits,
        dc: usize,
        quant: &[[i32; 64]; 16],
    ) -> Result<[i32; 64], Error> {
        let mut block = [0i32; 64];
        block[0] = self.bundles[dc].next();
        let mut idx = [0u8; 64];
        let count = dct_coeffs(b, &mut block, &mut idx)?;
        let q = &quant[b.read(4) as usize];
        block[0] = block[0].wrapping_mul(q[0]) >> 11;
        for &i in &idx[..count] {
            let at = usize::from(SCAN[usize::from(i)]);
            block[at] = block[at].wrapping_mul(q[usize::from(i)]) >> 11;
        }
        Ok(block)
    }
}

fn dct_coeffs(b: &mut Bits, block: &mut [i32; 64], idx: &mut [u8; 64]) -> Result<usize, Error> {
    let mut coef = [0i32; 128];
    let mut mode = [0u8; 128];
    let (mut start, mut end) = (64usize, 64usize);
    for (c, m) in [(4, 0), (24, 0), (44, 0), (1, 3), (2, 3), (3, 3)] {
        coef[end] = c;
        mode[end] = m;
        end += 1;
    }
    let mut count = 0;
    let value = |b: &mut Bits, bits: i32| -> i32 {
        if bits == 0 {
            1 - ((b.read(1) as i32) << 1)
        } else {
            let t = b.read(bits as u32) as i32 | (1 << bits);
            signed(b, t)
        }
    };
    let mut bits = b.read(4) as i32 - 1;
    while bits >= 0 {
        let mut pos = start;
        while pos < end {
            if (mode[pos] == 0 && coef[pos] == 0) || !b.bit() {
                pos += 1;
                continue;
            }
            let mut c = coef[pos];
            match mode[pos] {
                0 | 2 => {
                    if mode[pos] == 0 {
                        coef[pos] = c + 4;
                        mode[pos] = 1;
                    } else {
                        coef[pos] = 0;
                        mode[pos] = 0;
                        pos += 1;
                    }
                    for _ in 0..4 {
                        if b.bit() {
                            if start == 0 {
                                return Err(Error::Format("coefficient list overflow"));
                            }
                            start -= 1;
                            coef[start] = c;
                            mode[start] = 3;
                        } else {
                            if count == 64 || c > 63 {
                                return Err(Error::Format("coefficient overflow"));
                            }
                            block[usize::from(SCAN[c as usize])] = value(b, bits);
                            idx[count] = c as u8;
                            count += 1;
                        }
                        c += 1;
                    }
                }
                1 => {
                    mode[pos] = 2;
                    for _ in 0..3 {
                        if end == 128 {
                            return Err(Error::Format("coefficient list overflow"));
                        }
                        c += 4;
                        coef[end] = c;
                        mode[end] = 2;
                        end += 1;
                    }
                }
                _ => {
                    if count == 64 || c > 63 {
                        return Err(Error::Format("coefficient overflow"));
                    }
                    block[usize::from(SCAN[c as usize])] = value(b, bits);
                    idx[count] = c as u8;
                    count += 1;
                    coef[pos] = 0;
                    mode[pos] = 0;
                    pos += 1;
                }
            }
        }
        bits -= 1;
    }
    Ok(count)
}

fn residue(b: &mut Bits, block: &mut [i16; 64], mut masks: i32) -> Result<(), Error> {
    let mut coef = [0i32; 128];
    let mut mode = [0u8; 128];
    let (mut start, mut end) = (64usize, 64usize);
    for (c, m) in [(4, 0), (24, 0), (44, 0), (0, 2)] {
        coef[end] = c;
        mode[end] = m;
        end += 1;
    }
    let mut nz = [0usize; 64];
    let mut nz_count = 0;
    let mut mask = 1i16 << b.read(3);
    while mask != 0 {
        for &at in &nz[..nz_count] {
            if !b.bit() {
                continue;
            }
            block[at] = if block[at] < 0 {
                block[at].wrapping_sub(mask)
            } else {
                block[at].wrapping_add(mask)
            };
            masks -= 1;
            if masks < 0 {
                return Ok(());
            }
        }
        let mut pos = start;
        while pos < end {
            if (coef[pos] == 0 && mode[pos] == 0) || !b.bit() {
                pos += 1;
                continue;
            }
            let mut c = coef[pos];
            match mode[pos] {
                0 | 2 => {
                    if mode[pos] == 0 {
                        coef[pos] = c + 4;
                        mode[pos] = 1;
                    } else {
                        coef[pos] = 0;
                        mode[pos] = 0;
                        pos += 1;
                    }
                    for _ in 0..4 {
                        if b.bit() {
                            if start == 0 {
                                return Err(Error::Format("residue list overflow"));
                            }
                            start -= 1;
                            coef[start] = c;
                            mode[start] = 3;
                        } else {
                            if nz_count == 64 || c > 63 {
                                return Err(Error::Format("residue overflow"));
                            }
                            let at = usize::from(SCAN[c as usize]);
                            nz[nz_count] = at;
                            nz_count += 1;
                            block[at] = if b.bit() { -mask } else { mask };
                            masks -= 1;
                            if masks < 0 {
                                return Ok(());
                            }
                        }
                        c += 1;
                    }
                }
                1 => {
                    mode[pos] = 2;
                    for _ in 0..3 {
                        if end == 128 {
                            return Err(Error::Format("residue list overflow"));
                        }
                        c += 4;
                        coef[end] = c;
                        mode[end] = 2;
                        end += 1;
                    }
                }
                _ => {
                    if nz_count == 64 || c > 63 {
                        return Err(Error::Format("residue overflow"));
                    }
                    let at = usize::from(SCAN[c as usize]);
                    nz[nz_count] = at;
                    nz_count += 1;
                    block[at] = if b.bit() { -mask } else { mask };
                    coef[pos] = 0;
                    mode[pos] = 0;
                    pos += 1;
                    masks -= 1;
                    if masks < 0 {
                        return Ok(());
                    }
                }
            }
        }
        mask >>= 1;
    }
    Ok(())
}

fn copy8(dst: &mut [u8], at: usize, src: &[u8], from: usize, stride: usize) {
    for row in 0..8 {
        dst[at + row * stride..][..8].copy_from_slice(&src[from + row * stride..][..8]);
    }
}

fn idct8(s: [i32; 8]) -> [i32; 8] {
    const A1: i32 = 2896;
    const A2: i32 = 2217;
    const A3: i32 = 3784;
    const A4: i32 = -5352;
    let mul = |x: i32, y: i32| (x.wrapping_mul(y)) >> 11;
    let a0 = s[0].wrapping_add(s[4]);
    let a1 = s[0].wrapping_sub(s[4]);
    let a2 = s[2].wrapping_add(s[6]);
    let a3 = mul(A1, s[2].wrapping_sub(s[6]));
    let a4 = s[5].wrapping_add(s[3]);
    let a5 = s[5].wrapping_sub(s[3]);
    let a6 = s[1].wrapping_add(s[7]);
    let a7 = s[1].wrapping_sub(s[7]);
    let b0 = a4.wrapping_add(a6);
    let b1 = mul(A3, a5.wrapping_add(a7));
    let b2 = mul(A4, a5).wrapping_sub(b0).wrapping_add(b1);
    let b3 = mul(A1, a6.wrapping_sub(a4)).wrapping_sub(b2);
    let b4 = mul(A2, a7).wrapping_add(b3).wrapping_sub(b1);
    [
        a0.wrapping_add(a2).wrapping_add(b0),
        a1.wrapping_add(a3).wrapping_sub(a2).wrapping_add(b2),
        a1.wrapping_sub(a3).wrapping_add(a2).wrapping_add(b3),
        a0.wrapping_sub(a2).wrapping_sub(b4),
        a0.wrapping_sub(a2).wrapping_add(b4),
        a1.wrapping_sub(a3).wrapping_add(a2).wrapping_sub(b3),
        a1.wrapping_add(a3).wrapping_sub(a2).wrapping_sub(b2),
        a0.wrapping_add(a2).wrapping_sub(b0),
    ]
}

fn idct(block: &mut [i32; 64]) {
    let mut tmp = [0i32; 64];
    for x in 0..8 {
        let col: [i32; 8] = std::array::from_fn(|k| block[x + k * 8]);
        let out = if col[1..].iter().all(|&v| v == 0) {
            [col[0]; 8]
        } else {
            idct8(col)
        };
        for (k, v) in out.into_iter().enumerate() {
            tmp[x + k * 8] = v;
        }
    }
    for y in 0..8 {
        let row: [i32; 8] = std::array::from_fn(|k| tmp[y * 8 + k]);
        for (k, v) in idct8(row).into_iter().enumerate() {
            block[y * 8 + k] = v.wrapping_add(0x7f) >> 8;
        }
    }
}

pub(crate) fn to_rgba(planes: &[Plane], width: usize, height: usize, out: &mut [u8]) {
    let (y_p, u_p, v_p) = (&planes[0], &planes[1], &planes[2]);
    for (row, line) in out.chunks_exact_mut(width * 4).take(height).enumerate() {
        let ys = &y_p.data[row * y_p.stride..][..width];
        let us = &u_p.data[(row >> 1) * u_p.stride..];
        let vs = &v_p.data[(row >> 1) * v_p.stride..];
        let y_pairs = ys.as_chunks::<2>().0;
        let px_pairs = line.as_chunks_mut::<8>().0;
        for (i, ([y0, y1], px8)) in y_pairs.iter().zip(px_pairs).enumerate() {
            let d = i32::from(us[i]) - 128;
            let e = i32::from(vs[i]) - 128;
            let r_add = 409 * e + 128;
            let g_sub = 100 * d + 208 * e - 128;
            let b_add = 516 * d + 128;

            let c0 = (i32::from(*y0) - 16) * 298;
            px8[0] = ((c0 + r_add) >> 8).clamp(0, 255) as u8;
            px8[1] = ((c0 - g_sub) >> 8).clamp(0, 255) as u8;
            px8[2] = ((c0 + b_add) >> 8).clamp(0, 255) as u8;
            px8[3] = 255;

            let c1 = (i32::from(*y1) - 16) * 298;
            px8[4] = ((c1 + r_add) >> 8).clamp(0, 255) as u8;
            px8[5] = ((c1 - g_sub) >> 8).clamp(0, 255) as u8;
            px8[6] = ((c1 + b_add) >> 8).clamp(0, 255) as u8;
            px8[7] = 255;
        }
    }
}
