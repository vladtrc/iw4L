#[derive(Clone, Copy, Default)]
pub(super) struct Complex {
    pub re: f32,
    pub im: f32,
}

impl Complex {
    pub fn product(self, other: Self) -> Self {
        Self {
            re: self.re * other.re - self.im * other.im,
            im: self.re * other.im + self.im * other.re,
        }
    }
}

pub(super) struct Fft<const N: usize> {
    roots: Box<[Complex]>,
    reversed: Box<[usize]>,
}

impl<const N: usize> Default for Fft<N> {
    fn default() -> Self {
        assert!(N.is_power_of_two());
        Self {
            roots: (0..N / 2)
                .map(|i| {
                    let angle = -std::f32::consts::TAU * i as f32 / N as f32;
                    Complex {
                        re: angle.cos(),
                        im: angle.sin(),
                    }
                })
                .collect(),
            reversed: (0..N)
                .map(|i| i.reverse_bits() >> (usize::BITS - N.ilog2()))
                .collect(),
        }
    }
}

impl<const N: usize> Fft<N> {
    pub fn transform(&self, data: &mut [Complex; N], inverse: bool) {
        for (i, &j) in self.reversed.iter().enumerate() {
            if i < j {
                data.swap(i, j);
            }
        }
        let mut width = 2;
        while width <= N {
            for start in (0..N).step_by(width) {
                for k in 0..width / 2 {
                    let mut root = self.roots[k * (N / width)];
                    if inverse {
                        root.im = -root.im;
                    }
                    let a = data[start + k];
                    let b = data[start + k + width / 2].product(root);
                    data[start + k] = Complex {
                        re: a.re + b.re,
                        im: a.im + b.im,
                    };
                    data[start + k + width / 2] = Complex {
                        re: a.re - b.re,
                        im: a.im - b.im,
                    };
                }
            }
            width *= 2;
        }
        if inverse {
            for value in data {
                value.re /= N as f32;
                value.im /= N as f32;
            }
        }
    }
}
