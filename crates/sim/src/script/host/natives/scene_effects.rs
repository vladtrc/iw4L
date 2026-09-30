use bevy_ecs::prelude::World;

use crate::script::host::args::{float, vector};
use crate::script::{Runtime, Value};
use crate::{ScriptEarthquake, ScriptFog, ScriptFogParams, ScriptSunFog};

fn duration_ms(seconds: f32) -> Result<i32, String> {
    let ms = (f64::from(seconds) * 1000.0).round();
    if !seconds.is_finite() || seconds < 0.0 || ms > f64::from(i32::MAX) {
        return Err("duration must be finite, nonnegative and fit the level clock".into());
    }
    Ok(ms as i32)
}

pub(super) fn set_exp_fog(world: &mut World, _: &Value, args: &[Value]) -> Result<Value, String> {
    if !matches!(args.len(), 6 | 7 | 14) {
        return Err("setexpfog expects 6, 7 or 14 arguments".into());
    }
    let target = ScriptFogParams {
        start_dist: float(args, 0)?,
        halfway_dist: float(args, 1)?,
        color_rgb: [float(args, 2)?, float(args, 3)?, float(args, 4)?],
        max_opacity: if args.len() == 6 {
            1.0
        } else {
            float(args, 5)?
        },
        sun: if args.len() == 14 {
            Some(ScriptSunFog {
                color_rgb: [float(args, 7)?, float(args, 8)?, float(args, 9)?],
                sun_dir: vector(args, 10)?,
                begin_angle_deg: float(args, 11)?,
                end_angle_deg: float(args, 12)?,
                scale: float(args, 13)?,
            })
        } else {
            None
        },
    };
    if !target.valid() {
        return Err("invalid exponential fog parameters".into());
    }
    let duration_ms = duration_ms(float(args, if args.len() == 6 { 5 } else { 6 })?)?;
    let now = crate::level_time_ms(world.resource::<crate::step::StepRequest>().tick);
    let mut runtime = world.resource_mut::<Runtime>();
    let previous = runtime.engine.fog.map(|fog| fog.sample(now));
    runtime.engine.fog = Some(ScriptFog {
        from: previous.unwrap_or(target),
        to: target,
        start_ms: now,
        duration_ms: if previous.is_some() { duration_ms } else { 0 },
    });
    Ok(Value::Undefined)
}

pub(super) fn earthquake(world: &mut World, _: &Value, args: &[Value]) -> Result<Value, String> {
    if args.len() != 4 {
        return Err("earthquake expects scale, duration, origin and radius".into());
    }
    let now = crate::level_time_ms(world.resource::<crate::step::StepRequest>().tick);
    let mut quake = ScriptEarthquake {
        id: 0,
        scale: float(args, 0)?,
        duration_ms: duration_ms(float(args, 1)?)?,
        origin: vector(args, 2)?,
        radius: float(args, 3)?,
        start_ms: now,
    };
    if !quake.valid() {
        return Err("invalid earthquake parameters".into());
    }
    let mut runtime = world.resource_mut::<Runtime>();
    let engine = &mut runtime.engine;
    engine.earthquakes.retain(|quake| quake.active(now));
    if engine.earthquakes.len() >= crate::MAX_SCRIPT_EARTHQUAKES {
        return Err("too many active earthquakes".into());
    }
    quake.id = engine.next_earthquake;
    engine.next_earthquake = engine.next_earthquake.wrapping_add(1);
    engine.earthquakes.push(quake);
    Ok(Value::Undefined)
}
