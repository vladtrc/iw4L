use bevy_ecs::prelude::World;

use super::{args::string, entities::EntityKind, natives::engine::entity_id};
use crate::script::{Namespace, NativeRegistry, RoundScript, Value};

fn initialize(runtime: &mut RoundScript, id: u64) {
    let entity = runtime.entities.get_mut(&id).unwrap();
    entity.solid = true;
    entity.contents |= 0xa080;
}

fn spawn(world: &mut World, receiver: &Value, args: &[Value]) -> Result<Value, String> {
    let parent = entity_id(world, receiver)?;
    let (module, classname, fields, origin) = {
        let mut runtime = world.resource_mut::<RoundScript>();
        let module = runtime
            .spawners
            .get(&parent)
            .ok_or("spawndrone can only be applied to AI spawners")?
            .module
            .clone();
        let classname = runtime.entities[&parent].classname.clone();
        let fields = runtime.objects[&parent].clone();
        let origin = match runtime.object_field(parent, "origin") {
            Value::Vector(origin) => origin,
            _ => [0.0; 3],
        };
        (module, classname, fields, origin)
    };
    let targetname = if args.len() >= 2 {
        Value::string(&string(args, 1)?)
    } else {
        Value::Undefined
    };
    let mut copied = Vec::with_capacity(fields.len());
    for (field, value) in fields {
        copied.push((field, crate::script::runtime::copy_value(world, value)?));
    }
    let child = world
        .resource_mut::<RoundScript>()
        .create_entity(EntityKind::Drone, &classname)?;
    let result = (|| -> Result<(), String> {
        let presence = super::presence::spawn_presence(world, origin)?;
        world
            .resource_mut::<RoundScript>()
            .entities
            .get_mut(&child)
            .unwrap()
            .presence = Some(presence);
        let number = crate::frame::FrameWorld::from_world(world)
            .gentity_number(presence)
            .ok_or("new drone has no entity number")?;
        let mut runtime = world.resource_mut::<RoundScript>();
        runtime.objects.insert(child, copied.into_iter().collect());
        runtime.set_object_field(child, "activator", Value::Undefined);
        runtime.set_object_field(child, "targetname", targetname);
        let flags = match runtime.object_field(child, "spawnflags") {
            Value::Int(flags) => flags,
            _ => 0,
        };
        runtime.set_object_field(child, "spawnflags", Value::Int(flags & !1));
        let entity = runtime.entities.get_mut(&child).unwrap();
        entity.presence = Some(presence);
        entity.number = number;
        initialize(&mut runtime, child);
        let errors = runtime.errors.clone();
        drop(runtime);
        let now = world
            .get_resource::<crate::step::StepRequest>()
            .map_or(0, |request| i64::from(crate::level_time_ms(request.tick)));
        crate::script::runtime::run_now(
            world,
            &format!("{module}::main"),
            Value::Object(child),
            vec![],
            now,
        )
        .map_err(|error| error.to_string())?;
        let mut runtime = world.resource_mut::<RoundScript>();
        if let Some(((site, message), _)) = runtime
            .errors
            .iter()
            .find(|(key, hits)| errors.get(*key) != Some(*hits))
        {
            return Err(format!(
                "drone type initialization failed at {site}: {message}"
            ));
        }
        runtime
            .entities
            .get_mut(&child)
            .ok_or("drone was deleted during initialization")?
            .classname = "script_model".into();
        runtime.set_object_field(child, "classname", Value::string("script_model"));
        runtime.set_object_field(child, "code_classname", Value::string("script_model"));
        let origin = match runtime.object_field(child, "origin") {
            Value::Vector(origin) => origin,
            _ => origin,
        };
        drop(runtime);
        if let Some(at) = super::actors::spatial::floor_position(world, origin, None)? {
            world.resource_mut::<RoundScript>().set_object_field(
                child,
                "origin",
                Value::Vector(at),
            );
        }
        Ok(())
    })();
    if let Err(error) = result {
        world.resource_mut::<RoundScript>().delete_entity(child);
        return Err(error);
    }
    let now = world
        .get_resource::<crate::step::StepRequest>()
        .map_or(0, |request| crate::level_time_ms(request.tick));
    if let Some(spawner) = world
        .resource_mut::<RoundScript>()
        .spawners
        .get_mut(&parent)
    {
        spawner.last_spawn_time = now;
    }
    Ok(Value::Object(child))
}

pub(crate) fn register(registry: &mut NativeRegistry) {
    registry.register(Namespace::Method, "spawndrone", spawn);
    registry.register(Namespace::Method, "makefakeai", |world, receiver, _| {
        let id = entity_id(world, receiver)?;
        if world.resource::<RoundScript>().entities[&id].kind != EntityKind::Drone {
            return Err("makefakeai requires a drone script mover".into());
        }
        initialize(&mut world.resource_mut::<RoundScript>(), id);
        super::presence::settle_collision(world);
        Ok(Value::Undefined)
    });
}
