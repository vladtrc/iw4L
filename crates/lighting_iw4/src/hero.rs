use crate::LIGHT_GRID_COLORS_BYTE_COUNT;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct HeroOnlyLight {
    pub kind: u8,
    pub color: [f32; 3],
    pub direction: [f32; 3],
    pub origin: [f32; 3],
    pub radius: f32,
    pub outer_cos: f32,
    pub inner_cos: f32,
    pub exponent: i32,
}

impl HeroOnlyLight {
    pub fn from_le_bytes(bytes: &[u8]) -> Option<Self> {
        let scalar = |offset: usize| {
            Some(f32::from_le_bytes(
                bytes.get(offset..offset + 4)?.try_into().ok()?,
            ))
        };
        let vector = |offset| Some([scalar(offset)?, scalar(offset + 4)?, scalar(offset + 8)?]);
        let light = Self {
            kind: *bytes.first()?,
            color: vector(4)?,
            direction: vector(16)?,
            origin: vector(28)?,
            radius: scalar(40)?,
            outer_cos: scalar(44)?,
            inner_cos: scalar(48)?,
            exponent: i32::from_le_bytes(bytes.get(52..56)?.try_into().ok()?),
        };
        light.valid().then_some(light)
    }

    pub fn valid(&self) -> bool {
        self.color
            .iter()
            .chain(&self.direction)
            .chain(&self.origin)
            .all(|v| v.is_finite())
            && self.radius.is_finite()
            && self.radius >= 0.0
            && self.outer_cos.is_finite()
            && self.inner_cos.is_finite()
    }

    pub fn evaluate(&self, point: [f32; 3]) -> Option<([f32; 3], [f32; 3])> {
        let delta = core::array::from_fn::<_, 3, _>(|i| self.origin[i] - point[i]);
        let squared = dot(delta, delta);
        if squared > self.radius * self.radius {
            return None;
        }
        let distance = libm::sqrtf(squared);
        if distance < 0.001 || !distance.is_finite() {
            return None;
        }
        let inverse_distance = 1.0 / distance;
        let direction = delta.map(|v| v * inverse_distance);
        let mut angular = 1.0;
        if self.kind == 2 {
            let cosine = dot(self.direction, direction);
            if cosine <= self.outer_cos {
                return None;
            }
            if cosine < self.inner_cos {
                let fraction = (cosine - self.outer_cos) / (self.inner_cos - self.outer_cos);
                if self.exponent <= 256 {
                    for _ in 0..self.exponent.max(0) {
                        angular *= fraction;
                    }
                } else {
                    angular = libm::powf(fraction, self.exponent as f32);
                }
            }
        }
        let strength = (1.0 - (distance / self.radius).clamp(0.0, 1.0)) * angular;
        Some((direction, self.color.map(|v| v * strength)))
    }
}

fn dot(a: [f32; 3], b: [f32; 3]) -> f32 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

pub fn add_hero_only_lights(
    colors: &mut [u8; LIGHT_GRID_COLORS_BYTE_COUNT],
    point: [f32; 3],
    lights: &[HeroOnlyLight],
) {
    for light in lights {
        let Some((direction, intensity)) = light.evaluate(point) else {
            continue;
        };
        let mut sample = 0;
        for z in 0..4 {
            for y in 0..4 {
                for x in 0..4 {
                    if (1..=2).contains(&x) && (1..=2).contains(&y) && (1..=2).contains(&z) {
                        continue;
                    }
                    let basis = [x, y, z].map(|v| {
                        (f64::from(v) * f64::from(f32::from_bits(0x3f2aaaab)) - 1.0) as f32
                    });
                    let inverse = 1.0 / libm::sqrtf(dot(basis, basis));
                    let incident = dot(basis.map(|v| v * inverse), direction).max(0.0);
                    for channel in 0..3 {
                        let energy = (incident * intensity[channel]).max(0.0);
                        let old = f32::from(colors[sample * 3 + channel]) / 255.0;
                        let value = (old + libm::sqrtf(energy) * 0.5).clamp(0.0, 1.0);
                        colors[sample * 3 + channel] = libm::floorf(value * 255.0 + 0.5) as u8;
                    }
                    sample += 1;
                }
            }
        }
    }
}
