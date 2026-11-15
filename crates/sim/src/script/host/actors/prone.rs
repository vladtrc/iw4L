use bevy_ecs::prelude::World;

use super::super::args::{arg, float};
use super::super::script_anim::set_weight;
use super::{actor, brain, store};
use crate::script::{Namespace, NativeRegistry, Value};

const SLOPE_WEIGHT_MAX: f32 = 0.99;
const SLOPE_BLEND: f32 = 0.05;
const SLOPE_REACH: f32 = 15.0;
const SLOPE_DROP: f32 = 48.0;
const SLOPE_MAX_DEG: f32 = 45.0;

fn slope_weights(rise: f32, (inv_down, inv_up): (f32, f32)) -> [f32; 3] {
    let (down, up) = if rise >= 0.0 {
        (0.0, (rise * inv_up).min(SLOPE_WEIGHT_MAX))
    } else {
        ((rise * inv_down).min(SLOPE_WEIGHT_MAX), 0.0)
    };
    [down, 1.0 - down.max(up), up]
}

fn ground_rise(world: &mut World, id: u64) -> f32 {
    let mut runtime = world.resource_mut::<crate::script::RoundScript>();
    let origin = super::campaign::vector_field(&mut runtime, id, "origin").unwrap_or([0.0; 3]);
    let yaw = super::campaign::vector_field(&mut runtime, id, "angles").map_or(0.0, |a| a[1]);
    drop(runtime);
    let (sin, cos) = yaw.to_radians().sin_cos();
    let collision = super::super::presence::settled(world);
    let ground = |side: f32| {
        let at = [
            origin[0] + cos * SLOPE_REACH * side,
            origin[1] + sin * SLOPE_REACH * side,
            origin[2] + SLOPE_DROP,
        ];
        let below = [at[0], at[1], origin[2] - SLOPE_DROP];
        let trace = collision.trace_world(
            at,
            below,
            [0.0; 3],
            [0.0; 3],
            crate::bullet_collision::MASK_PLAYER_SOLID,
        );
        (trace.startsolid == 0 && trace.fraction < 1.0).then_some(trace.endpos[2])
    };
    match (ground(-1.0), ground(1.0)) {
        (Some(back), Some(ahead)) => (ahead - back)
            .atan2(2.0 * SLOPE_REACH)
            .to_degrees()
            .clamp(-SLOPE_MAX_DEG, SLOPE_MAX_DEG),
        _ => 0.0,
    }
}

fn blend_nodes(world: &mut World, receiver: &Value, weight: f32, blend: f32) -> Result<(), String> {
    let id = actor(world, receiver)?;
    let nodes = brain(world, id)
        .prone_nodes
        .ok_or("Must call SetProneAnimNodes before calling EnterProne")?;
    for node in &nodes {
        set_weight(world, receiver, node, weight, blend, 1.0)?;
    }
    Ok(())
}

fn set_prone(world: &mut World, receiver: &Value, prone: bool) -> Result<(), String> {
    let id = actor(world, receiver)?;
    let mut state = brain(world, id);
    state.prone = prone;
    store(world, id, state);
    Ok(())
}

pub(super) fn register(registry: &mut NativeRegistry) {
    use Namespace::Method;
    registry.register(Method, "setproneanimnodes", |world, receiver, args| {
        let id = actor(world, receiver)?;
        let (down, up) = (float(args, 0)?, float(args, 1)?);
        if down >= 0.0 {
            return Err("Down angle (parameter 1) must be set to be less than 0.".into());
        }
        if up <= 0.0 {
            return Err("Up angle (parameter 2) must be set to be greater than 0.".into());
        }
        let nodes = [
            arg(args, 2)?.clone(),
            arg(args, 3)?.clone(),
            arg(args, 4)?.clone(),
        ];
        let mut state = brain(world, id);
        state.prone_nodes = Some(nodes.clone());
        state.prone_scale = (down.recip(), up.recip());
        store(world, id, state);
        for node in &nodes {
            set_weight(world, receiver, node, 1.0, 0.0, 1.0)?;
        }
        Ok(Value::Undefined)
    });
    registry.register(Method, "enterprone", |world, receiver, args| {
        blend_nodes(world, receiver, 1.0, float(args, 0)?)?;
        set_prone(world, receiver, true)?;
        Ok(Value::Undefined)
    });
    registry.register(Method, "exitprone", |world, receiver, args| {
        set_prone(world, receiver, false)?;
        if brain(world, actor(world, receiver)?).prone_nodes.is_some() {
            blend_nodes(world, receiver, 0.0, float(args, 0)?)?;
        }
        Ok(Value::Undefined)
    });
    registry.register(Method, "updateprone", |world, receiver, args| {
        let id = actor(world, receiver)?;
        let state = brain(world, id);
        if !state.prone {
            return Ok(Value::Undefined);
        }
        let (weight, blend, rate) = (float(args, 2)?, float(args, 3)?, float(args, 4)?);
        for anim in [arg(args, 0)?.clone(), arg(args, 1)?.clone()] {
            set_weight(world, receiver, &anim, weight, blend, rate)?;
        }
        if let Some(nodes) = state.prone_nodes {
            let weights = slope_weights(ground_rise(world, id), state.prone_scale);
            for (node, weight) in nodes.iter().zip(weights) {
                set_weight(world, receiver, node, weight, SLOPE_BLEND, 1.0)?;
            }
        }
        Ok(Value::Undefined)
    });
}
