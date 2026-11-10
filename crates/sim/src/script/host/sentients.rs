use super::{
    args::{arg, int, string},
    entities::EntityKind,
    natives::engine::entity_id,
};
use crate::script::{Namespace, NativeRegistry, RoundScript, Value};

#[derive(Clone, Debug)]
pub(crate) struct Sentient {
    pub slot: u8,
    pub team: u8,
    pub expendable: bool,
    pub threat_group: u8,
}

const MAX_SENTIENTS: u8 = 50;
const MAX_OTHER_SENTIENTS: usize = 16;

fn protected(runtime: &RoundScript, id: u64) -> bool {
    matches!(
        runtime.entities[&id].kind,
        EntityKind::Player | EntityKind::Actor
    )
}

pub(super) fn allocate(
    runtime: &mut RoundScript,
    id: u64,
    team: u8,
    expendable: bool,
) -> Result<(), String> {
    let count = runtime
        .sentients
        .keys()
        .filter(|owner| !protected(runtime, **owner))
        .count();
    if !protected(runtime, id) && count >= MAX_OTHER_SENTIENTS {
        let victim = runtime
            .sentients
            .iter()
            .filter(|(_, s)| s.expendable)
            .min_by_key(|(_, s)| s.slot)
            .map(|(owner, _)| *owner)
            .ok_or("too many sentients")?;
        super::actors::suppression::dissociate(runtime, victim);
        runtime.sentients.remove(&victim);
    }
    let slot = (0..MAX_SENTIENTS)
        .find(|slot| runtime.sentients.values().all(|s| s.slot != *slot))
        .ok_or("too many sentients")?;
    runtime.sentients.insert(
        id,
        Sentient {
            slot,
            team,
            expendable,
            threat_group: 0,
        },
    );
    Ok(())
}

pub(crate) fn load_field(runtime: &RoundScript, id: u64, field: &str) -> Option<Value> {
    if field != "team" {
        return None;
    }
    let sentient = runtime.sentients.get(&id)?;
    Some(Value::string(match sentient.team {
        1 => "axis",
        2 => "allies",
        3 => "team3",
        4 => "neutral",
        _ => "free",
    }))
}

pub(crate) fn store_field(
    runtime: &mut RoundScript,
    id: u64,
    field: &str,
    value: &Value,
) -> Result<bool, String> {
    if field != "team" || !runtime.sentients.contains_key(&id) {
        return Ok(false);
    }
    let Value::String(name) = value else {
        return Err("sentient team must be a string".into());
    };
    let team = super::spawners::team(name)?;
    runtime.sentients.get_mut(&id).unwrap().team = team;
    Ok(true)
}

pub(crate) fn check_array_write(runtime: &RoundScript, id: u64, field: &str) -> Result<(), String> {
    if field == "team" && runtime.sentients.contains_key(&id) {
        return Err("sentient team cannot be an array".into());
    }
    Ok(())
}

#[derive(Clone, Debug)]
pub(crate) struct ThreatGroups {
    names: Vec<Option<String>>,
}

impl Default for ThreatGroups {
    fn default() -> Self {
        Self { names: vec![None] }
    }
}

impl ThreatGroups {
    fn find(&self, name: &str) -> Option<u8> {
        self.names
            .iter()
            .position(|entry| entry.as_deref() == Some(name))
            .map(|index| index as u8)
    }
    fn create(&mut self, name: String) {
        if self.names.len() < 16 && self.find(&name).is_none() {
            self.names.push(Some(name));
        }
    }
    fn name(&self, group: u8) -> &str {
        self.names
            .get(usize::from(group))
            .and_then(Option::as_deref)
            .unwrap_or("")
    }
}

pub(crate) fn register(registry: &mut NativeRegistry) {
    registry.register(
        Namespace::Function,
        "createthreatbiasgroup",
        |world, _, args| {
            if args.len() != 1 {
                return Err("createthreatbiasgroup expects one name".into());
            }
            let name = string(args, 0)?;
            world
                .resource_mut::<RoundScript>()
                .threat_groups
                .create(name);
            Ok(Value::Undefined)
        },
    );
    registry.register(
        Namespace::Function,
        "threatbiasgroupexists",
        |world, _, args| {
            if args.len() != 1 {
                return Err("threatbiasgroupexists expects one name".into());
            }
            let name = string(args, 0)?;
            Ok(Value::Int(i32::from(
                world
                    .resource::<RoundScript>()
                    .threat_groups
                    .find(&name)
                    .is_some(),
            )))
        },
    );
    registry.register(
        Namespace::Method,
        "setthreatbiasgroup",
        |world, receiver, args| {
            let id = entity_id(world, receiver)?;
            let mut runtime = world.resource_mut::<RoundScript>();
            if !runtime.sentients.contains_key(&id) {
                return Err("entity is not a sentient".into());
            }
            let group = if args.len() == 1 {
                let name = string(args, 0)?;
                runtime
                    .threat_groups
                    .find(&name)
                    .ok_or_else(|| format!("unknown threat bias group `{name}`"))?
            } else {
                0
            };
            runtime.sentients.get_mut(&id).unwrap().threat_group = group;
            Ok(Value::Undefined)
        },
    );
    registry.register(
        Namespace::Method,
        "getthreatbiasgroup",
        |world, receiver, _| {
            let id = entity_id(world, receiver)?;
            let runtime = world.resource::<RoundScript>();
            let sentient = runtime
                .sentients
                .get(&id)
                .ok_or("entity is not a sentient")?;
            Ok(Value::string(
                runtime.threat_groups.name(sentient.threat_group),
            ))
        },
    );

    registry.register(Namespace::Method, "isbadguy", |world, receiver, _| {
        let id = entity_id(world, receiver)?;
        let runtime = world.resource::<RoundScript>();
        let sentient = runtime
            .sentients
            .get(&id)
            .ok_or("entity is not a sentient")?;
        Ok(Value::Int(i32::from((1 << sentient.team) & 0x0a != 0)))
    });
    registry.register(Namespace::Function, "isenemyteam", |_, _, args| {
        if args.len() != 2 {
            return Err("isenemyteam expects two team strings".into());
        }
        let (Value::String(first), Value::String(second)) = (&args[0], &args[1]) else {
            return Err("teams must be strings".into());
        };
        if &**first == "dead" || &**second == "dead" {
            return Ok(Value::Int(0));
        }
        let a = super::spawners::team(first)?;
        let b = super::spawners::team(second)?;
        let masks = [0, 0x0c, 0x0a, 0x06, 0, 0];
        Ok(Value::Int(i32::from(masks[a as usize] & (1 << b) != 0)))
    });
    registry.register(Namespace::Function, "isai", |world, _, args| {
        let value = arg(args, 0)?;
        let runtime = world.resource::<RoundScript>();
        Ok(Value::Int(i32::from(runtime.entity(value).is_some_and(
            |(id, e)| {
                runtime.live(&id) && e.kind == EntityKind::Actor && runtime.actors.contains_key(&id)
            },
        ))))
    });
    registry.register(Namespace::Function, "issentient", |world, _, args| {
        let value = arg(args, 0)?;
        let runtime = world.resource::<RoundScript>();
        Ok(Value::Int(i32::from(runtime.entity(value).is_some_and(
            |(id, _)| runtime.live(&id) && runtime.sentients.contains_key(&id),
        ))))
    });
    registry.register(
        Namespace::Method,
        "makeentitysentient",
        |world, receiver, args| {
            let id = entity_id(world, receiver)?;
            if protected(world.resource::<RoundScript>(), id) {
                return Err("cannot use makeentitysentient on a player or actor".into());
            }
            let expendable = if args.len() > 1 {
                int(args, 1)? != 0
            } else {
                false
            };
            if let Some(sentient) = world.resource_mut::<RoundScript>().sentients.get_mut(&id) {
                sentient.expendable = expendable;
                return Ok(Value::Int(1));
            }
            let team = super::spawners::team(&string(args, 0)?)?;
            allocate(
                &mut world.resource_mut::<RoundScript>(),
                id,
                team,
                expendable,
            )?;
            Ok(Value::Int(1))
        },
    );
    registry.register(
        Namespace::Method,
        "freeentitysentient",
        |world, receiver, _| {
            let id = entity_id(world, receiver)?;
            if protected(world.resource::<RoundScript>(), id) {
                return Err("do not use freeentitysentient on players or actors".into());
            }
            let mut runtime = world.resource_mut::<RoundScript>();
            super::actors::suppression::dissociate(&mut runtime, id);
            runtime.sentients.remove(&id);
            Ok(Value::Undefined)
        },
    );
}
