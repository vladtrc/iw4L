use super::args::{string, vector};
use super::natives::engine::entity_id;
use crate::frame::FrameWorld;
use crate::script::{Namespace, NativeRegistry, RoundScript, Value};
use bevy_ecs::prelude::World;
use entity_iw4::EntityEventKind;

fn alias_index(world: &World, args: &[Value]) -> Result<i32, String> {
    let name = string(args, 0)?;
    world
        .resource::<crate::script::MatchScript>()
        .precached
        .get(&("rumble", name.clone()))
        .copied()
        .ok_or_else(|| format!("rumble {name} was not precached"))
}

fn emit(world: &mut World, kind: EntityEventKind, number: i32, origin: [f32; 3], index: i32) {
    let tick = world.resource::<crate::step::StepRequest>().tick;
    FrameWorld::from_world(world).push_entity_event(
        tick,
        crate::EventAudience::All,
        kind,
        crate::EntityEventPayload {
            number,
            origin,
            event_parm: index,
            ..Default::default()
        },
    );
}

fn entity_change(
    world: &mut World,
    receiver: &Value,
    args: &[Value],
    kind: EntityEventKind,
) -> Result<Value, String> {
    let id = entity_id(world, receiver)?;
    let index = alias_index(world, args)?;
    if args.len() != 1 {
        return Err("entity rumble requires one rumble name".into());
    }
    let number = world.resource::<RoundScript>().entities[&id].number;
    let origin = match super::players::entity_field(world, id, "origin") {
        Value::Vector(origin) => origin,
        _ => [0.0; 3],
    };
    emit(world, kind, number, origin, index);
    Ok(Value::Undefined)
}

pub(crate) fn register(registry: &mut NativeRegistry) {
    registry.register(
        Namespace::Method,
        "playrumbleonentity",
        |world, receiver, args| {
            entity_change(world, receiver, args, EntityEventKind::PLAY_RUMBLE_ON_ENT)
        },
    );
    registry.register(
        Namespace::Method,
        "playrumblelooponentity",
        |world, receiver, args| {
            entity_change(world, receiver, args, EntityEventKind::PLAY_RUMBLE_ON_ENT)
        },
    );
    registry.register(Namespace::Method, "stoprumble", |world, receiver, args| {
        entity_change(world, receiver, args, EntityEventKind::STOP_RUMBLE)
    });
    registry.register(Namespace::Method, "vibrate", |_world, _receiver, _args| {
        Ok(Value::Undefined)
    });
    registry.register(
        Namespace::Function,
        "playrumbleonposition",
        |world, _, args| {
            if args.len() != 2 {
                return Err("playRumbleOnPosition requires a name and position".into());
            }
            let index = alias_index(world, args)?;
            let origin = vector(args, 1)?;
            if origin.iter().any(|v| !v.is_finite()) {
                return Err("rumble position must be finite".into());
            }
            emit(
                world,
                EntityEventKind::PLAY_RUMBLE_ON_POS,
                i32::from(trace_iw4::ENTITYNUM_WORLD),
                origin,
                index,
            );
            Ok(Value::Undefined)
        },
    );
    registry.register(
        Namespace::Function,
        "playrumblelooponposition",
        |world, _, args| {
            if args.len() != 2 {
                return Err("playRumbleLoopOnPosition requires a name and position".into());
            }
            let index = alias_index(world, args)?;
            let origin = vector(args, 1)?;
            if origin.iter().any(|v| !v.is_finite()) {
                return Err("rumble position must be finite".into());
            }
            emit(
                world,
                EntityEventKind::PLAY_RUMBLE_ON_POS,
                i32::from(trace_iw4::ENTITYNUM_WORLD),
                origin,
                index,
            );
            Ok(Value::Undefined)
        },
    );
    registry.register(Namespace::Function, "stopallrumbles", |world, _, _| {
        emit(
            world,
            EntityEventKind::STOP_RUMBLE,
            i32::from(trace_iw4::ENTITYNUM_WORLD),
            [0.0; 3],
            0,
        );
        Ok(Value::Undefined)
    });
}
