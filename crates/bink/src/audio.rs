use crate::Error;
use crate::bits::Bits;

const CRITICAL_FREQS: [u32; 25] = [
    100, 200, 300, 400, 510, 630, 770, 920, 1080, 1270, 1480, 1720, 2000, 2320, 2700, 3150, 3700,
    4400, 5300, 6400, 7700, 9500, 12000, 15500, 24500,
];
const RLE_LENS: [usize; 16] = [2, 3, 4, 5, 6, 8, 9, 10, 11, 12, 13, 14, 15, 16, 32, 64];

pub(crate) struct Audio {
    dct: bool,
    transforms: usize,
    frame_len: usize,
    overlap: usize,
    bands: Vec<usize>,
    quant: [f32; 96],
    root: f32,
    first: bool,
    fft: Fft,
    coeffs: Vec<f32>,
    spectrum: Vec<(f32, f32)>,
    out: Vec<Vec<f32>>,
    previous: Vec<Vec<f32>>,
}

impl Audio {
    pub(crate) fn new(rate: u32, channels: u32, dct: bool) -> Self {
        let mut bits = if rate < 22050 {
            9
        } else if rate < 44100 {
            10
        } else {
            11
        };
        let (rate, transforms) = if dct {
            (rate, channels as usize)
        } else {
            bits += channels.ilog2();
            (rate * channels, 1)
        };
        let frame_len = 1usize << bits;
        let half = rate.div_ceil(2) as usize;
        let num_bands = (1..25)
            .find(|&n| half <= CRITICAL_FREQS[n - 1] as usize)
            .unwrap_or(25);
        let mut bands = vec![2usize];
        for &f in &CRITICAL_FREQS[..num_bands - 1] {
            bands.push((f as usize * frame_len / half) & !1);
        }
        bands.push(frame_len);
        let sqrt = (frame_len as f32).sqrt() * 32768.0;
        let root = if dct {
            frame_len as f32 / sqrt
        } else {
            2.0 / sqrt
        };
        let overlap = frame_len / 16;
        Self {
            dct,
            transforms,
            frame_len,
            overlap,
            bands,
            quant: std::array::from_fn(|i| (i as f32 * 0.152_891_65).exp() * root),
            root,
            first: true,
            fft: Fft::new(frame_len),
            coeffs: vec![0.0; frame_len + 2],
            spectrum: vec![(0.0, 0.0); frame_len],
            out: vec![vec![0.0; frame_len]; transforms],
            previous: vec![vec![0.0; overlap]; transforms],
        }
    }

    pub(crate) fn decode(&mut self, packet: &[u8], out: &mut Vec<f32>) -> Result<(), Error> {
        let mut b = Bits::new(packet);
        b.skip(32);
        let keep = self.frame_len - self.overlap;
        while b.left() > 0 {
            self.block(&mut b);
            if b.overrun() {
                return Err(Error::Format("audio packet truncated"));
            }
            if self.transforms == 1 {
                out.extend_from_slice(&self.out[0][..keep]);
            } else {
                out.extend((0..keep).flat_map(|i| self.out.iter().map(move |ch| ch[i])));
            }
            b.align32();
        }
        Ok(())
    }

    fn block(&mut self, b: &mut Bits) {
        if self.dct {
            b.skip(2);
        }
        for ch in 0..self.transforms {
            self.coefficients(b);
            let n = self.frame_len;
            let mut out = std::mem::take(&mut self.out[ch]);
            let c = &mut self.coeffs;
            if self.dct {
                c[0] *= 2.0;
                dct3(c, &mut out, &self.fft, &mut self.spectrum);
            } else {
                for v in c[3..n].iter_mut().step_by(2) {
                    *v = -*v;
                }
                c[n] = c[1];
                c[n + 1] = 0.0;
                c[1] = 0.0;
                c2r(c, &mut out, 0.5, &self.fft, &mut self.spectrum);
            }
            self.out[ch] = out;
        }

        let ov = self.overlap;
        let count = (ov * self.transforms) as f32;
        for ch in 0..self.transforms {
            let (out, prev) = (&mut self.out[ch], &mut self.previous[ch]);
            if !self.first {
                for (i, (x, &p)) in out[..ov].iter_mut().zip(prev.iter()).enumerate() {
                    let j = (ch + i * self.transforms) as f32;
                    *x = (p * (count - j) + *x * j) / count;
                }
            }
            prev.copy_from_slice(&out[self.frame_len - ov..]);
        }
        self.first = false;
    }

    fn coefficients(&mut self, b: &mut Bits) {
        let n = self.frame_len;
        let c = &mut self.coeffs;
        c[0] = float(b) * self.root;
        c[1] = float(b) * self.root;
        let mut quant = [0f32; 25];
        for q in &mut quant[..self.bands.len() - 1] {
            *q = self.quant[(b.read(8) as usize).min(95)];
        }
        let (mut k, mut q) = (0, quant[0]);
        let mut i = 2;
        while i < n {
            let j = if b.bit() {
                i + RLE_LENS[b.read(4) as usize] * 8
            } else {
                i + 8
            }
            .min(n);
            let width = b.read(4);
            if width == 0 {
                c[i..j].fill(0.0);
                i = j;
                while self.bands[k] < i {
                    q = quant[k];
                    k += 1;
                }
            } else {
                while i < j {
                    if self.bands[k] == i {
                        q = quant[k];
                        k += 1;
                    }
                    let v = b.read(width) as f32;
                    c[i] = if v != 0.0 && b.bit() { -q * v } else { q * v };
                    i += 1;
                }
            }
        }
    }
}

fn float(b: &mut Bits) -> f32 {
    let power = b.read(5) as i32;
    let f = b.read(23) as f32 * 2f32.powi(power - 23);
    if b.bit() { -f } else { f }
}

fn c2r(packed: &[f32], out: &mut [f32], scale: f32, fft: &Fft, s: &mut [(f32, f32)]) {
    let n = out.len();
    s[0] = (packed[0], 0.0);
    s[n / 2] = (packed[n], 0.0);
    for k in 1..n / 2 {
        s[k] = (packed[2 * k], packed[2 * k + 1]);
        s[n - k] = (packed[2 * k], -packed[2 * k + 1]);
    }
    fft.inverse(s);
    for (o, &(re, _)) in out.iter_mut().zip(s.iter()) {
        *o = re * scale;
    }
}

fn dct3(src: &mut [f32], out: &mut [f32], fft: &Fft, s: &mut [(f32, f32)]) {
    let n = out.len();
    let freq = std::f64::consts::PI / (2 * n) as f64;
    let cos = |i: usize| (i as f64 * freq).cos() as f32;
    src[n] = 2.0 * src[n - 1];
    for i in (2..=n - 2).rev().step_by(2) {
        let (v1, v2) = (src[i], src[i - 1] - src[i + 1]);
        let (a, b) = (cos(n - i), cos(i));
        src[i + 1] = a * v1 - b * v2;
        src[i] = a * v2 + b * v1;
    }
    c2r(src, out, 0.5 / n as f32, fft, s);
    for i in 0..n / 2 {
        let (a, b) = (out[i], out[n - 1 - i]);
        let c = (0.5 / ((2 * i + 1) as f64 * freq).sin()) as f32;
        let (t1, t2) = (a + b, (a - b) * c);
        out[i] = t1 + t2;
        out[n - 1 - i] = t1 - t2;
    }
}

struct Fft {
    twiddle: Vec<(f32, f32)>,
    rev: Vec<u32>,
}

impl Fft {
    fn new(n: usize) -> Self {
        let bits = n.ilog2();
        Self {
            twiddle: (0..n / 2)
                .map(|k| {
                    let a = 2.0 * std::f64::consts::PI * k as f64 / n as f64;
                    (a.cos() as f32, a.sin() as f32)
                })
                .collect(),
            rev: (0..n as u32)
                .map(|i| i.reverse_bits() >> (32 - bits))
                .collect(),
        }
    }

    fn inverse(&self, x: &mut [(f32, f32)]) {
        let n = x.len();
        for i in 0..n {
            let r = self.rev[i] as usize;
            if i < r {
                x.swap(i, r);
            }
        }
        let mut len = 2;
        while len <= n {
            let step = n / len;
            for start in (0..n).step_by(len) {
                for k in 0..len / 2 {
                    let (wr, wi) = self.twiddle[k * step];
                    let (br, bi) = x[start + k + len / 2];
                    let t = (br * wr - bi * wi, br * wi + bi * wr);
                    let a = x[start + k];
                    x[start + k] = (a.0 + t.0, a.1 + t.1);
                    x[start + k + len / 2] = (a.0 - t.0, a.1 - t.1);
                }
            }
            len *= 2;
        }
    }
}
