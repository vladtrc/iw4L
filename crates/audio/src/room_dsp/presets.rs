use super::RATE;

pub(super) struct Preset {
    pub name: &'static str,
    decay_seconds: f32,
    brightness: f32,
    span_ms: f32,
}

const fn room(name: &'static str, decay_seconds: f32, brightness: f32, span_ms: f32) -> Preset {
    Preset {
        name,
        decay_seconds,
        brightness,
        span_ms,
    }
}

pub(super) const PRESETS: [Preset; 26] = [
    room("generic", 1.35, 0.65, 14.0),
    room("paddedcell", 0.24, 0.12, 3.0),
    room("room", 0.65, 0.6, 7.0),
    room("bathroom", 1.05, 0.95, 5.0),
    room("livingroom", 0.48, 0.25, 8.0),
    room("stoneroom", 1.8, 0.85, 12.0),
    room("auditorium", 3.4, 0.55, 26.0),
    room("concerthall", 4.4, 0.7, 32.0),
    room("cave", 3.8, 0.45, 42.0),
    room("arena", 4.9, 0.65, 55.0),
    room("hangar", 5.4, 0.8, 64.0),
    room("carpetedhallway", 0.38, 0.15, 11.0),
    room("hallway", 1.15, 0.55, 15.0),
    room("stonecorridor", 2.4, 0.8, 21.0),
    room("alley", 1.3, 0.6, 37.0),
    room("forest", 0.9, 0.18, 48.0),
    room("city", 1.65, 0.55, 58.0),
    room("mountains", 2.7, 0.35, 95.0),
    room("quarry", 2.1, 0.7, 72.0),
    room("plain", 0.32, 0.2, 28.0),
    room("parkinglot", 1.55, 0.8, 31.0),
    room("sewerpipe", 2.65, 0.5, 19.0),
    room("underwater", 1.1, 0.025, 6.0),
    room("drugged", 4.7, 0.25, 43.0),
    room("dizzy", 3.25, 0.45, 68.0),
    room("psychotic", 5.1, 0.95, 9.0),
];

fn random(state: &mut u32) -> f32 {
    *state ^= *state << 13;
    *state ^= *state >> 17;
    *state ^= *state << 5;
    (*state >> 8) as f32 / (1u32 << 24) as f32
}

impl Preset {
    pub fn impulse(&self, room: usize, channel: usize) -> Vec<f32> {
        let mut seed = 1789 + room as u32 * 257 + channel as u32 * 65537;
        let predelay = (self.span_ms * 0.001 * RATE as f32) as usize;
        let length = (RATE as f32 * self.decay_seconds * 1.25) as usize;
        let mut response = vec![0.0; length + predelay];
        let mut filtered = 0.0;
        for n in 0..length {
            let time = n as f32 / RATE as f32;
            let cutoff =
                300.0 + 16_000.0 * self.brightness * (-2.4 * time / self.decay_seconds).exp();
            let pole = (-std::f32::consts::TAU * cutoff / RATE as f32).exp();
            filtered = pole * filtered + (1.0 - pole) * (2.0 * random(&mut seed) - 1.0);
            let attack = (time / (0.012 + self.span_ms * 0.0003)).min(1.0);
            response[n + predelay] =
                filtered * attack * 10.0_f32.powf(-3.0 * time / self.decay_seconds);
        }
        let tail_energy: f64 = response.iter().map(|&v| f64::from(v).powi(2)).sum();
        let early_gain = (tail_energy as f32 * 0.04).sqrt();
        for tap in 0..6 {
            let delay = predelay
                + ((tap as f32 + 0.5 + random(&mut seed) * 0.3)
                    * self.span_ms
                    * 0.001
                    * RATE as f32) as usize;
            if let Some(value) = response.get_mut(delay) {
                *value += early_gain * (2.0 * random(&mut seed) - 1.0) / (tap + 1) as f32;
            }
        }
        let energy: f64 = response.iter().map(|&v| f64::from(v).powi(2)).sum();
        let gain = (0.5 / energy).sqrt() as f32 * 0.085;
        for sample in &mut response {
            *sample *= gain;
        }
        response
    }
}
