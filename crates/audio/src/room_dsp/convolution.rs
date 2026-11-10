use super::fft::{Complex, Fft};
use super::{Block, QUANTUM};

pub(super) struct Kernel {
    channels: [Box<[Complex]>; 4],
    parts: usize,
}

impl Kernel {
    pub fn new<const H: usize, const N: usize>(
        ir: &[Vec<f32>; 4],
        start: usize,
        end: usize,
        fft: &Fft<N>,
    ) -> Self {
        let end = end.min(ir[0].len());
        let parts = end.saturating_sub(start).div_ceil(H);
        let bins = N / 2 + 1;
        Self {
            parts,
            channels: std::array::from_fn(|channel| {
                let mut spectra = vec![Complex::default(); parts * bins];
                for part in 0..parts {
                    let first = start + part * H;
                    let last = (first + H).min(end);
                    let mut values = [Complex::default(); N];
                    for (v, &sample) in values.iter_mut().zip(&ir[channel][first..last]) {
                        v.re = sample;
                    }
                    fft.transform(&mut values, false);
                    spectra[part * bins..(part + 1) * bins].copy_from_slice(&values[..bins]);
                }
                spectra.into_boxed_slice()
            }),
        }
    }

    pub fn parts(&self) -> usize {
        self.parts
    }
}

pub(super) struct Stage<const H: usize, const N: usize> {
    history: [Box<[Complex]>; 2],
    overlap: [[f32; H]; 2],
    cursor: usize,
    parts: usize,
}

impl<const H: usize, const N: usize> Stage<H, N> {
    pub fn new(parts: usize) -> Self {
        assert_eq!(N, 2 * H);
        let parts = parts.max(1);
        Self {
            history: std::array::from_fn(|_| {
                vec![Complex::default(); parts * (N / 2 + 1)].into_boxed_slice()
            }),
            overlap: [[0.0; H]; 2],
            cursor: 0,
            parts,
        }
    }

    pub fn clear(&mut self) {
        for channel in &mut self.history {
            channel.fill(Complex::default());
        }
        self.overlap = [[0.0; H]; 2];
        self.cursor = 0;
    }

    pub fn process(
        &mut self,
        input: &[[f32; 2]; H],
        kernel: &Kernel,
        fft: &Fft<N>,
    ) -> [[f32; 2]; H] {
        if kernel.parts == 0 {
            return [[0.0; 2]; H];
        }
        let bins = N / 2 + 1;
        for channel in 0..2 {
            let mut current = [Complex::default(); N];
            for (value, sample) in current.iter_mut().zip(input) {
                value.re = sample[channel];
            }
            fft.transform(&mut current, false);
            self.history[channel][self.cursor * bins..(self.cursor + 1) * bins]
                .copy_from_slice(&current[..bins]);
        }
        let mut output = [[0.0; 2]; H];
        for channel in 0..2 {
            let mut result = [Complex::default(); N];
            for source in 0..2 {
                for part in 0..kernel.parts {
                    let position = (self.cursor + self.parts - part) % self.parts;
                    let input = &self.history[source][position * bins..(position + 1) * bins];
                    let weights =
                        &kernel.channels[channel * 2 + source][part * bins..(part + 1) * bins];
                    for ((value, &x), &weight) in result[..bins].iter_mut().zip(input).zip(weights)
                    {
                        let product = x.product(weight);
                        value.re += product.re;
                        value.im += product.im;
                    }
                }
            }
            result[0].im = 0.0;
            result[N / 2].im = 0.0;
            for bin in 1..N / 2 {
                result[N - bin] = Complex {
                    re: result[bin].re,
                    im: -result[bin].im,
                };
            }
            fft.transform(&mut result, true);
            for i in 0..H {
                output[i][channel] = result[i].re + self.overlap[channel][i];
                self.overlap[channel][i] = result[i + H].re;
            }
        }
        self.cursor = (self.cursor + 1) % self.parts;
        output
    }
}

pub(super) struct DelayedStage<const H: usize, const N: usize> {
    stage: Stage<H, N>,
    input: Box<[[f32; 2]; H]>,
    output: Box<[[f32; 2]]>,
    input_cursor: usize,
    output_cursor: usize,
}

impl<const H: usize, const N: usize> DelayedStage<H, N> {
    pub fn new(parts: usize, offset: usize) -> Self {
        assert!(offset >= H && offset.is_multiple_of(H) && H.is_multiple_of(QUANTUM));
        Self {
            stage: Stage::new(parts),
            input: Box::new([[0.0; 2]; H]),
            output: vec![[0.0; 2]; offset].into_boxed_slice(),
            input_cursor: 0,
            output_cursor: 0,
        }
    }

    pub fn clear(&mut self) {
        self.stage.clear();
        self.input.fill([0.0; 2]);
        self.output.fill([0.0; 2]);
        self.input_cursor = 0;
        self.output_cursor = 0;
    }

    pub fn process(&mut self, input: &Block, kernel: &Kernel, fft: &Fft<N>) -> Block {
        let mut output = [[0.0; 2]; QUANTUM];
        output.copy_from_slice(&self.output[self.output_cursor..self.output_cursor + QUANTUM]);
        self.input[self.input_cursor..self.input_cursor + QUANTUM].copy_from_slice(input);
        self.input_cursor += QUANTUM;
        self.output_cursor = (self.output_cursor + QUANTUM) % self.output.len();
        if self.input_cursor == H {
            let result = self.stage.process(&self.input, kernel, fft);
            // Offset buys the larger FFT its input block before any corresponding output is due.
            let position = (self.output_cursor + self.output.len() - H) % self.output.len();
            self.output[position..position + H].copy_from_slice(&result);
            self.input_cursor = 0;
        }
        output
    }
}
