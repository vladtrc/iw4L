use asset_world::{ExpFog, SunFog};
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
        let sampled = sim::ScriptFog {
            from: fog_params(self.previous),
            to: fog_params(self.fog),
            start_ms: self.start_ms,
            duration_ms: self.duration_ms,
        }
        .sample(time_ms);
        let mut result = exp_fog(sampled);
        result.transition_time = self.fog.transition_time;
        result.volumetric = self.fog.volumetric;
        result
    }
}

fn fog_params(fog: ExpFog) -> sim::ScriptFogParams {
    sim::ScriptFogParams {
        start_dist: fog.start_dist,
        halfway_dist: fog.halfway_dist,
        color_rgb: fog.color_rgb,
        max_opacity: fog.max_opacity,
        sun: fog.sun.map(|sun| sim::ScriptSunFog {
            color_rgb: sun.color_rgb,
            sun_dir: sun.sun_dir,
            begin_angle_deg: sun.begin_angle_deg,
            end_angle_deg: sun.end_angle_deg,
            scale: sun.scale,
        }),
    }
}

fn exp_fog(fog: sim::ScriptFogParams) -> ExpFog {
    ExpFog {
        start_dist: fog.start_dist,
        halfway_dist: fog.halfway_dist,
        color_rgb: fog.color_rgb,
        max_opacity: fog.max_opacity,
        transition_time: 0.0,
        volumetric: None,
        sun: fog.sun.map(|sun| SunFog {
            color_rgb: sun.color_rgb,
            sun_dir: sun.sun_dir,
            begin_angle_deg: sun.begin_angle_deg,
            end_angle_deg: sun.end_angle_deg,
            scale: sun.scale,
        }),
    }
}

#[derive(Resource, Default)]
pub(crate) struct ScriptFogPresentation {
    applied: Option<sim::ScriptFog>,
    baseline: Option<MapFrameFog>,
}

pub(crate) fn sync_script_fog(
    scene: Res<crate::prepare::scene::world::WorldScene>,
    presented: Res<net::PresentedSnapshot>,
    mut state: ResMut<ScriptFogPresentation>,
    mut map_fog: Option<ResMut<MapFrameFog>>,
    mut commands: Commands,
) {
    if !scene.spawned {
        return;
    }
    let fog = presented
        .snapshot()
        .and_then(|snapshot| snapshot.meta.objectives.fog);
    if fog == state.applied {
        return;
    }
    if let Some(fog) = fog {
        if state.applied.is_none() {
            state.baseline = map_fog.as_deref().copied();
        }
        let mut target = MapFrameFog::new(exp_fog(fog.to));
        target.previous = exp_fog(fog.from);
        target.start_ms = fog.start_ms;
        target.duration_ms = fog.duration_ms;
        if let Some(map_fog) = map_fog.as_deref_mut() {
            *map_fog = target;
        } else {
            commands.insert_resource(target);
        }
    } else if let Some(baseline) = state.baseline.take() {
        if let Some(map_fog) = map_fog.as_deref_mut() {
            *map_fog = baseline;
        } else {
            commands.insert_resource(baseline);
        }
    } else {
        commands.remove_resource::<MapFrameFog>();
    }
    state.applied = fog;
}
