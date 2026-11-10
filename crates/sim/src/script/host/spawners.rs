mod spawn;

use bevy_ecs::prelude::World;
use std::collections::BTreeSet;

use super::{args::string, arrays::new_array, natives::engine::entity_id};
use crate::script::{Fault, Location, Namespace, NativeRegistry, RoundScript, Value};

#[derive(Clone, Debug)]
pub(crate) struct Spawner {
    pub module: String,
    pub team: u8,
    pub last_spawn_time: i32,
}

pub(crate) fn aitype_module(classname: &str) -> Option<String> {
    classname
        .strip_prefix("actor_")
        .map(|name| format!("aitype/{}", name.to_ascii_lowercase()))
}

pub(crate) fn install_map_actor(
    runtime: &mut RoundScript,
    id: u64,
    pairs: &[(String, String)],
) -> Result<(), String> {
    let classname = runtime.entities[&id].classname.clone();
    let flags = match runtime.object_field(id, "spawnflags") {
        Value::Int(flags) => flags,
        _ => 0,
    };
    if flags & 1 == 0 {
        let slot = super::actors::free_slot(runtime)?;
        let previous = runtime.entities[&id].kind.clone();
        runtime.entities.get_mut(&id).unwrap().kind = super::entities::EntityKind::Actor;
        if let Err(error) = super::actors::allocate(runtime, id, slot) {
            runtime.entities.get_mut(&id).unwrap().kind = previous;
            return Err(error);
        }
        runtime.set_object_field(id, "model", Value::Undefined);
        return Ok(());
    }
    let has_count = pairs
        .iter()
        .any(|(key, _)| key.eq_ignore_ascii_case("count"));
    if !has_count {
        runtime.set_object_field(id, "count", Value::Int(1));
    }
    runtime.set_object_field(id, "model", Value::Undefined);
    let entity = runtime.entities.get_mut(&id).unwrap();
    entity.solid = false;
    entity.hidden = true;
    entity.contents = 0;
    runtime.spawners.insert(
        id,
        Spawner {
            module: aitype_module(&classname).unwrap(),
            team: 0,
            last_spawn_time: -1,
        },
    );
    Ok(())
}

pub(crate) fn finish_spawning(world: &mut World) -> Result<(), Fault> {
    let runtime = world.resource::<RoundScript>();
    if runtime.profile != asset_core::RuntimeProfile::Campaign {
        return Ok(());
    }
    let mut owners: Vec<_> = runtime
        .entities
        .iter()
        .filter(|(id, _)| runtime.spawners.contains_key(id) || runtime.actors.contains_key(id))
        .map(|(id, entity)| {
            (
                *id,
                aitype_module(&entity.classname).unwrap(),
                entity.number,
            )
        })
        .collect();
    owners.sort_by_key(|(_, _, number)| *number);
    super::actors::spatial::initialize_pending(world).map_err(|message| {
        Fault::at(
            &Location {
                module: "<engine>".into(),
                function: "actor spatial initialization".into(),
                line: 0,
                column: 0,
            },
            message,
        )
    })?;
    let mut precached = BTreeSet::new();
    for (id, module, _) in owners {
        if !world.resource::<RoundScript>().live(&id) {
            continue;
        }
        for (entry, receiver) in [
            (
                world
                    .resource::<RoundScript>()
                    .spawners
                    .contains_key(&id)
                    .then(|| format!("{module}::spawner")),
                Value::Object(id),
            ),
            (
                precached
                    .insert(module.clone())
                    .then(|| format!("{module}::precache")),
                Value::level(),
            ),
        ] {
            let Some(entry) = entry else {
                continue;
            };
            if !world
                .resource::<RoundScript>()
                .program
                .as_ref()
                .unwrap()
                .names
                .contains_key(entry.as_str())
            {
                return Err(Fault::at(
                    &Location {
                        module: module.clone(),
                        function: "<actor initialization>".into(),
                        line: 0,
                        column: 0,
                    },
                    format!("required actor type entry {entry} is missing"),
                ));
            }
            let errors = world.resource::<RoundScript>().errors.clone();
            crate::script::runtime::run_now(world, &entry, receiver, vec![], 0)?;
            if let Some(((site, message), _)) = world
                .resource::<RoundScript>()
                .errors
                .iter()
                .find(|(key, hits)| errors.get(*key) != Some(*hits))
            {
                return Err(Fault::at(
                    &Location {
                        module: module.clone(),
                        function: entry.clone(),
                        line: 0,
                        column: 0,
                    },
                    format!("actor initialization failed at {site}: {message}"),
                ));
            }
        }
    }
    let mut actors: Vec<_> = world
        .resource::<RoundScript>()
        .actors
        .iter()
        .map(|(&id, actor)| (actor.slot, id))
        .collect();
    actors.sort_unstable();
    for (_, id) in actors {
        if !world.resource::<RoundScript>().can_receive_call(&id) {
            continue;
        }
        super::actors::startup::initialize(world, id, 0).map_err(|message| {
            Fault::at(
                &Location {
                    module: "<engine>".into(),
                    function: "actor script initialization".into(),
                    line: 0,
                    column: 0,
                },
                message,
            )
        })?;
    }
    world.resource_mut::<RoundScript>().started = false;
    Ok(())
}

pub(super) fn team(name: &str) -> Result<u8, String> {
    match name {
        "axis" => Ok(1),
        "allies" => Ok(2),
        "team3" => Ok(3),
        "neutral" => Ok(4),
        _ => Err(format!("unknown team '{name}'")),
    }
}

pub(super) fn team_flags(args: &[Value]) -> Result<u8, String> {
    let mut flags = 0;
    for index in 0..args.len() {
        let name = string(args, index)?;
        if name == "all" {
            flags |= 0x1e;
            break;
        }
        flags |= if name == "bad_guys" {
            0x0a
        } else {
            1 << team(&name)?
        };
    }
    Ok(flags)
}

fn matching(world: &World, flags: Option<u8>) -> Vec<Value> {
    let runtime = world.resource::<RoundScript>();
    let mut nodes: Vec<_> = runtime
        .spawners
        .iter()
        .filter(|(id, spawner)| {
            runtime.live(id) && flags.is_none_or(|flags| flags & (1 << spawner.team) != 0)
        })
        .map(|(id, _)| (runtime.entities[id].number, *id))
        .collect();
    nodes.sort_by_key(|(number, _)| *number);
    nodes.into_iter().map(|(_, id)| Value::Object(id)).collect()
}

pub(crate) fn register(registry: &mut NativeRegistry) {
    registry.register(Namespace::Method, "stalingradspawn", spawn::forced);
    registry.register(Namespace::Function, "getspawnerarray", |world, _, args| {
        if !args.is_empty() {
            return Err("cannot call getspawnerarray with parameters".into());
        }
        new_array(world, matching(world, None))
    });
    registry.register(
        Namespace::Function,
        "getspawnerteamarray",
        |world, _, args| {
            let flags = team_flags(args)?;
            if flags == 0 {
                return Err("no team was specified - use getspawnerarray instead".into());
            }
            new_array(world, matching(world, Some(flags)))
        },
    );
    registry.register(
        Namespace::Method,
        "setspawnerteam",
        |world, receiver, args| {
            let id = entity_id(world, receiver)?;
            if !world.resource::<RoundScript>().spawners.contains_key(&id) {
                return Err("setspawnerteam can only be applied to AI spawners".into());
            }
            let team = team(&string(args, 0)?)?;
            world
                .resource_mut::<RoundScript>()
                .spawners
                .get_mut(&id)
                .unwrap()
                .team = team;
            Ok(Value::Undefined)
        },
    );
}
