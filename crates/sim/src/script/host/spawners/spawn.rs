use super::super::{
    args::{int, optional},
    entities::EntityKind,
};
use super::*;

fn time(world: &World) -> i32 {
    world
        .get_resource::<crate::step::StepRequest>()
        .map_or(0, |request| crate::level_time_ms(request.tick))
}

fn number(runtime: &mut RoundScript, id: u64, field: &str) -> i32 {
    match runtime.object_field(id, field) {
        Value::Int(value) => value,
        _ => 0,
    }
}

pub(super) fn forced(world: &mut World, receiver: &Value, args: &[Value]) -> Result<Value, String> {
    let parent = entity_id(world, receiver)?;
    let now = time(world);
    let (classname, fields, count, flags, last_spawn) = {
        let mut runtime = world.resource_mut::<RoundScript>();
        let spawner = runtime
            .spawners
            .get(&parent)
            .ok_or("StalingradSpawn requires an AI spawner")?;
        let last_spawn = spawner.last_spawn_time;
        (
            runtime.entities[&parent].classname.clone(),
            runtime.objects[&parent].clone(),
            number(&mut runtime, parent, "count"),
            number(&mut runtime, parent, "spawnflags"),
            last_spawn,
        )
    };
    if last_spawn >= now {
        return Ok(Value::Undefined);
    }
    let dont_share = optional(args, 0, int)?.unwrap_or(0) != 0;
    let targetname = if args.len() >= 2 {
        Some(string(args, 1)?)
    } else {
        None
    };
    let dvars = &world.resource::<crate::script::MatchScript>().dvars;
    let disabled = dvars
        .get("ai_disablespawn")
        .is_some_and(|value| super::super::natives::iw4::atoi(value) != 0);
    let limit = dvars.get("ai_count").map_or(32, |value| {
        super::super::natives::iw4::atoi(value).clamp(0, 32)
    }) as usize;
    if disabled || count == 0 {
        return Ok(Value::Undefined);
    }
    if world.resource::<RoundScript>().actors.len() >= limit {
        if flags & 2 != 0 {
            return Err("forced actor eviction is not implemented".into());
        }
        return Ok(Value::Undefined);
    }
    let mut copied = std::collections::BTreeMap::new();
    for (field, value) in fields {
        copied.insert(field, crate::script::runtime::copy_value(world, value)?);
    }
    let child = world
        .resource_mut::<RoundScript>()
        .create_entity(EntityKind::Actor, &classname)?;
    {
        let mut runtime = world.resource_mut::<RoundScript>();
        runtime.objects.insert(child, copied);
        runtime.set_object_field(child, "activator", Value::Undefined);
        runtime.set_object_field(
            child,
            "targetname",
            targetname.map_or(Value::Undefined, |name| Value::string(&name)),
        );
        runtime.set_object_field(child, "spawnflags", Value::Int(flags & !1));
    }
    let result = (|| -> Result<(), String> {
        super::super::actors::spatial::initialize_physics(world, child)?;
        if flags & 8 != 0 {
            super::super::actors::perception::perfect_spawn_info(world, child, now)?;
        }
        super::super::actors::startup::start_type(world, child, now)?;
        if !dont_share && flags & 16 == 0 {
            super::super::actors::perception::share_spawn_info(world, child);
        }
        super::super::actors::startup::start_animation(world, child, now)?;
        Ok(())
    })();
    if let Err(error) = result {
        let number = world
            .resource::<RoundScript>()
            .entities
            .get(&child)
            .filter(|entity| entity.presence.is_some())
            .map(|entity| entity.number);
        crate::script::runtime::raise(world, Value::Object(child), "death", vec![]);
        world.resource_mut::<RoundScript>().delete_entity(child);
        if let Some(number) = number {
            crate::frame::FrameWorld::from_world(world).remove_script_mover_by_number(number);
        }
        return Err(error);
    }
    super::super::presence::settle_collision(world);
    crate::script::runtime::raise(
        world,
        Value::Object(parent),
        "spawned",
        vec![Value::Object(child)],
    );
    let mut runtime = world.resource_mut::<RoundScript>();
    let remaining = number(&mut runtime, parent, "count");
    if remaining > 0 {
        runtime.set_object_field(parent, "count", Value::Int(remaining - 1));
    }
    if let Some(spawner) = runtime.spawners.get_mut(&parent) {
        spawner.last_spawn_time = now;
    }
    Ok(Value::Object(child))
}
