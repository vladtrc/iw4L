#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ScriptSunFog {
    pub color_rgb: [f32; 3],
    pub sun_dir: [f32; 3],
    pub begin_angle_deg: f32,
    pub end_angle_deg: f32,
    pub scale: f32,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ScriptFogParams {
    pub start_dist: f32,
    pub halfway_dist: f32,
    pub color_rgb: [f32; 3],
    pub max_opacity: f32,
    pub sun: Option<ScriptSunFog>,
}

impl ScriptFogParams {
    pub fn valid(self) -> bool {
        self.start_dist.is_finite()
            && self.start_dist >= 0.0
            && self.halfway_dist.is_finite()
            && self.halfway_dist > 0.0
            && self.density().is_finite()
            && self.density() > 0.0
            && self.color_rgb.iter().all(|v| (0.0..=1.0).contains(v))
            && (0.0..=1.0).contains(&self.max_opacity)
            && self.sun.is_none_or(|sun| {
                sun.color_rgb.iter().all(|v| (0.0..=1.0).contains(v))
                    && sun.sun_dir.iter().all(|v| v.is_finite())
                    && (0.0..=180.0).contains(&sun.begin_angle_deg)
                    && (sun.begin_angle_deg..=180.0).contains(&sun.end_angle_deg)
                    && sun.scale.is_finite()
                    && sun.scale >= 0.0
            })
    }

    pub fn density(self) -> f32 {
        if self.halfway_dist <= 0.0 {
            0.0
        } else {
            std::f32::consts::LN_2 / self.halfway_dist
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ScriptFog {
    pub from: ScriptFogParams,
    pub to: ScriptFogParams,
    pub start_ms: i32,
    pub duration_ms: i32,
}

impl ScriptFog {
    pub fn sample(self, now_ms: i32) -> ScriptFogParams {
        let elapsed = now_ms.wrapping_sub(self.start_ms).max(0);
        if self.duration_ms <= 0 || elapsed >= self.duration_ms {
            return self.to;
        }
        let t = elapsed as f32 / self.duration_ms as f32;
        let lerp = |a: f32, b: f32| a + t * (b - a);
        let color = |a: [f32; 3], b: [f32; 3]| {
            std::array::from_fn(|i| {
                let pack = |v: f32| (v.clamp(0.0, 1.0) * 255.0 + 0.5).floor();
                lerp(pack(a[i]), pack(b[i])).round() / 255.0
            })
        };
        let a = self.from;
        let b = self.to;
        let density = lerp(a.density(), b.density());
        ScriptFogParams {
            start_dist: lerp(a.start_dist, b.start_dist),
            halfway_dist: if density == 0.0 {
                0.0
            } else {
                std::f32::consts::LN_2 / density
            },
            color_rgb: color(a.color_rgb, b.color_rgb),
            max_opacity: lerp(a.max_opacity, b.max_opacity),
            sun: b.sun.map(|bs| {
                let as_ = a.sun.unwrap_or(ScriptSunFog {
                    color_rgb: a.color_rgb,
                    sun_dir: [0.0; 3],
                    begin_angle_deg: 0.0,
                    end_angle_deg: 0.0,
                    scale: 1.0,
                });
                ScriptSunFog {
                    color_rgb: color(as_.color_rgb, bs.color_rgb),
                    sun_dir: std::array::from_fn(|i| lerp(as_.sun_dir[i], bs.sun_dir[i])),
                    begin_angle_deg: lerp(as_.begin_angle_deg, bs.begin_angle_deg),
                    end_angle_deg: lerp(as_.end_angle_deg, bs.end_angle_deg),
                    scale: lerp(as_.scale, bs.scale),
                }
            }),
        }
    }
}

pub const MAX_SCRIPT_EARTHQUAKES: usize = 64;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ScriptEarthquake {
    pub id: u32,
    pub origin: [f32; 3],
    pub scale: f32,
    pub radius: f32,
    pub start_ms: i32,
    pub duration_ms: i32,
}

impl ScriptEarthquake {
    pub fn valid(self) -> bool {
        self.origin.iter().all(|v| v.is_finite())
            && self.scale.is_finite()
            && self.scale >= 0.0
            && self.radius.is_finite()
            && self.radius >= 0.0
            && self.duration_ms > 0
    }

    pub fn active(self, now_ms: i32) -> bool {
        (0..self.duration_ms).contains(&now_ms.wrapping_sub(self.start_ms))
    }

    pub fn angle_offset(self, eye: [f32; 3], now_ms: i32) -> [f32; 3] {
        if !self.active(now_ms) {
            return [0.0; 3];
        }
        let distance = eye
            .iter()
            .zip(self.origin)
            .map(|(a, b)| (a - b).powi(2))
            .sum::<f32>()
            .sqrt();
        let spatial = if self.radius == 0.0 {
            1.0
        } else {
            (1.0 - distance / self.radius).clamp(0.0, 1.0)
        };
        let elapsed = now_ms.wrapping_sub(self.start_ms) as f32;
        let temporal = 1.0 - elapsed / self.duration_ms as f32;
        let amplitude = self.scale.min(1.0) * spatial * temporal;
        let phase = (self.id % 1024) as f32 * 2.3999631;
        [17.0, 23.0, 11.0].map(|frequency| {
            (elapsed * 0.001 * frequency * std::f32::consts::TAU + phase).sin() * amplitude
        })
    }
}
