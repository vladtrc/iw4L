use assets::{ExpFog, SunFog};
use bevy::prelude::*;

#[derive(Resource)]
pub struct FogDvars {
    pub enabled: bool,
    pub zfar: f32,
}
impl Default for FogDvars {
    fn default() -> Self {
        Self {
            enabled: true,
            zfar: 0.0,
        }
    }
}

#[derive(Resource, Clone, Copy, Debug)]
pub struct MapFrameFog {
    pub fog: ExpFog,
    pub sliders: ExpFog,
    pub slider_sun: SunFog,
    pub readonly_sun: SunFog,
    pub art_tweak: bool,
    pub script_disabled: bool,
    previous: ExpFog,
    start_ms: i32,
    duration_ms: i32,
}
impl MapFrameFog {
    pub fn new(fog: ExpFog) -> Self {
        let sun = fog.sun.unwrap_or(SunFog {
            color_rgb: [1.0, 0.0, 0.0],
            sun_dir: [1.0, 0.0, 0.0],
            begin_angle_deg: 0.0,
            end_angle_deg: 180.0,
            scale: 1.0,
        });
        Self {
            fog,
            sliders: fog,
            slider_sun: sun,
            readonly_sun: sun,
            art_tweak: false,
            script_disabled: false,
            previous: fog,
            start_ms: 0,
            duration_ms: 0,
        }
    }

    pub fn set(&mut self, fog: ExpFog, time_ms: i32) {
        let current = self.sample(time_ms);
        self.previous = current;
        self.start_ms = time_ms;

        self.duration_ms = if current.density() == 0.0 {
            0
        } else {
            (fog.transition_time * 1000.0).round() as i32
        };
        if let Some(sun) = fog.sun {
            self.readonly_sun = sun;
        }
        self.fog = fog;
    }

    pub fn apply_sliders(&mut self, time_ms: i32) {
        let mut fog = self.sliders;
        fog.transition_time = 0.0;
        if fog.sun.is_some() {
            fog.sun = Some(self.slider_sun);
        }
        if self.script_disabled {
            fog = ExpFog {
                start_dist: 100_000_000_000.0,
                halfway_dist: 100_000_000_001.0,
                color_rgb: [0.0; 3],
                max_opacity: 0.0,
                transition_time: 0.0,
                sun: None,
                volumetric: None,
            };
        }
        self.set(fog, time_ms);
    }

    pub fn sample(&self, time_ms: i32) -> ExpFog {
        let elapsed = time_ms.saturating_sub(self.start_ms);
        if self.duration_ms <= 0 || elapsed >= self.duration_ms {
            return self.fog;
        }
        let t = elapsed as f32 / self.duration_ms as f32;
        let lerp = |a: f32, b: f32| a + t * (b - a);
        let color = |a: [f32; 3], b: [f32; 3]| {
            std::array::from_fn(|i| {
                let pack = |v: f32| (v.clamp(0.0, 1.0) * 255.0 + 0.5).floor();
                lerp(pack(a[i]), pack(b[i])).round() / 255.0
            })
        };
        let a = self.previous;
        let b = self.fog;
        let mut result = b;
        result.start_dist = lerp(a.start_dist, b.start_dist);
        let density = lerp(a.density(), b.density());
        result.halfway_dist = if density == 0.0 {
            0.0
        } else {
            std::f32::consts::LN_2 / density
        };
        result.max_opacity = lerp(a.max_opacity, b.max_opacity);
        result.color_rgb = color(a.color_rgb, b.color_rgb);

        if let Some(bs) = b.sun {
            let as_ = a.sun.unwrap_or(SunFog {
                color_rgb: a.color_rgb,
                sun_dir: [0.0; 3],
                begin_angle_deg: 0.0,
                end_angle_deg: 0.0,
                scale: 1.0,
            });
            result.sun = Some(SunFog {
                color_rgb: color(as_.color_rgb, bs.color_rgb),
                sun_dir: std::array::from_fn(|i| lerp(as_.sun_dir[i], bs.sun_dir[i])),
                begin_angle_deg: lerp(as_.begin_angle_deg, bs.begin_angle_deg),
                end_angle_deg: lerp(as_.end_angle_deg, bs.end_angle_deg),
                scale: lerp(as_.scale, bs.scale),
            });
        }
        result
    }
}
