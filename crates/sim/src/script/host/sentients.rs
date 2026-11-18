use super::{
    args::{arg, int, string},
    entities::EntityKind,
    natives::engine::entity_id,
};
use crate::script::{Namespace, NativeRegistry, RoundScript, Value};

pub(crate) mod nearest;

#[derive(Clone, Debug)]
pub(crate) struct Sentient {
    pub slot: u8,
    pub team: u8,
    pub expendable: bool,
    pub threat_group: u8,
    pub max_visible_dist: f32,
    pub ignore_all: u8,
    pub ignore_me: u8,
    pub last_attacker: Option<u64>,
    pub target: Option<u64>,
    pub announced_target: Option<u64>,
    pub enemy_notify_at: Option<i32>,
    pub nearest_node: Option<u16>,
    pub nearest_node_valid: bool,
    pub nearest_node_bad: bool,
}

const MAX_SENTIENTS: u8 = 50;
const MAX_OTHER_SENTIENTS: usize = 16;
// Script-visible threat bias value used by SetIgnoreMeGroup/GetThreatBias.
pub(super) const IGNORE_GROUP_BIAS: i32 = i32::MIN;

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
    let target = match runtime.object_field(id, "enemy") {
        Value::Object(target) => Some(target),
        _ => None,
    };
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
        super::actors::targeting::dissociate(runtime, victim);
        runtime.sentients.remove(&victim);
    }
    let slot = (0..MAX_SENTIENTS)
        .find(|slot| runtime.sentients.values().all(|s| s.slot != *slot))
        .ok_or("too many sentients")?;
    let field = runtime.symbol("enemy");
    if let Some(fields) = runtime.objects.get_mut(&id) {
        fields.remove(&field);
    }
    runtime.sentients.insert(
        id,
        Sentient {
            slot,
            team,
            expendable,
            threat_group: 0,
            max_visible_dist: 8192.,
            ignore_all: 0,
            ignore_me: 0,
            last_attacker: None,
            target,
            announced_target: None,
            enemy_notify_at: None,
            nearest_node: None,
            nearest_node_valid: false,
            nearest_node_bad: false,
        },
    );
    Ok(())
}

pub(crate) fn load_field(runtime: &RoundScript, id: u64, field: &str) -> Option<Value> {
    if matches!(field, "ignoreall" | "ignoreme") {
        return runtime.sentients.get(&id).map(|sentient| {
            Value::Int(i32::from(if field == "ignoreall" {
                sentient.ignore_all
            } else {
                sentient.ignore_me
            }))
        });
    }
    if field == "enemy" {
        return runtime.sentients.get(&id).map(|sentient| {
            sentient
                .target
                .filter(|target| runtime.can_receive_call(target))
                .map_or(Value::Undefined, Value::Object)
        });
    }
    if field == "lastattacker" {
        return runtime
            .sentients
            .get(&id)
            .map(|s| s.last_attacker.map_or(Value::Undefined, Value::Object));
    }

    if field == "maxvisibledist" {
        return runtime
            .sentients
            .get(&id)
            .map(|s| Value::Float(s.max_visible_dist));
    }
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
    if matches!(field, "ignoreall" | "ignoreme")
        && let Some(sentient) = runtime.sentients.get_mut(&id)
    {
        let byte = super::args::int(std::slice::from_ref(value), 0)? as u8;
        if field == "ignoreall" {
            sentient.ignore_all = byte;
        } else {
            sentient.ignore_me = byte;
        }
        return Ok(true);
    }
    if field == "enemy" && runtime.sentients.contains_key(&id) {
        match value {
            Value::Undefined => {}
            Value::Object(target)
                if runtime.entities.contains_key(target) && runtime.can_receive_call(target) => {}
            _ => return Err("enemy must be a live entity or undefined".into()),
        }
        runtime.set_object_field(id, "enemy", value.clone());
        return Ok(true);
    }
    if field == "maxvisibledist"
        && let Some(sentient) = runtime.sentients.get_mut(&id)
    {
        let value = super::args::float(std::slice::from_ref(value), 0)?;
        if !value.is_finite() {
            return Err("maximum visible distance must be finite".into());
        }
        sentient.max_visible_dist = value;
        return Ok(true);
    }
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
    if ["team", "maxvisibledist", "enemy", "ignoreall", "ignoreme"].contains(&field)
        && runtime.sentients.contains_key(&id)
    {
        return Err("native sentient field cannot be an array".into());
    }
    Ok(())
}

#[derive(Clone, Debug)]
pub(crate) struct ThreatGroups {
    names: Vec<Option<String>>,
    biases: std::collections::BTreeMap<(u8, u8), i32>,
    row_defaults: std::collections::BTreeMap<u8, i32>,
}

impl Default for ThreatGroups {
    fn default() -> Self {
        Self {
            names: vec![None],
            biases: Default::default(),
            row_defaults: Default::default(),
        }
    }
}

impl ThreatGroups {
    fn find(&self, name: &str) -> Option<u8> {
        if name.is_empty() {
            return Some(0);
        }
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
    pub(super) fn bias(&self, from: u8, to: u8) -> i32 {
        self.biases
            .get(&(from, to))
            .copied()
            .or_else(|| self.row_defaults.get(&from).copied())
            .unwrap_or(0)
    }
    fn set_all(&mut self, from: u8, bias: i32) {
        self.row_defaults.insert(from, bias);
        self.biases.retain(|&(source, _), _| source != from);
    }
    fn require(&self, name: &str) -> Result<u8, String> {
        self.find(name)
            .ok_or_else(|| format!("unknown threat bias group `{name}`"))
    }
}

pub(crate) fn register(registry: &mut NativeRegistry) {
    registry.register(Namespace::Function, "setignoremegroup", |world, _, args| {
        if args.len() != 2 {
            return Err("setignoremegroup expects the ignored and observing group names".into());
        }
        let ignored = string(args, 0)?;
        let observing = string(args, 1)?;
        let groups = &mut world.resource_mut::<RoundScript>().threat_groups;
        let key = (groups.require(&ignored)?, groups.require(&observing)?);
        groups.biases.insert(key, IGNORE_GROUP_BIAS);
        Ok(Value::Undefined)
    });
    registry.register(Namespace::Function, "getthreatbias", |world, _, args| {
        if args.len() != 2 {
            return Err("getthreatbias expects two group names".into());
        }
        let from = string(args, 0)?;
        let to = string(args, 1)?;
        let groups = &world.resource::<RoundScript>().threat_groups;
        Ok(Value::Int(
            groups.bias(groups.require(&from)?, groups.require(&to)?),
        ))
    });
    registry.register(Namespace::Function, "setthreatbias", |world, _, args| {
        if args.len() != 3 {
            return Err("setthreatbias expects two group names and an integer bias".into());
        }
        let from = string(args, 0)?;
        let to = string(args, 1)?;
        let bias = int(args, 2)?;
        let groups = &mut world.resource_mut::<RoundScript>().threat_groups;
        let key = (groups.require(&from)?, groups.require(&to)?);
        groups.biases.insert(key, bias);
        Ok(Value::Undefined)
    });
    registry.register(
        Namespace::Function,
        "setthreatbiasagainstall",
        |world, _, args| {
            if args.len() != 2 {
                return Err(
                    "setthreatbiasagainstall expects a group name and an integer bias".into(),
                );
            }
            let from = string(args, 0)?;
            let bias = int(args, 1)?;
            let groups = &mut world.resource_mut::<RoundScript>().threat_groups;
            let from = groups.require(&from)?;
            groups.set_all(from, bias);
            Ok(Value::Undefined)
        },
    );
    registry.register(Namespace::Method, "isindoor", |world, receiver, _| {
        let id = entity_id(world, receiver)?;
        if !world.resource::<RoundScript>().sentients.contains_key(&id) {
            return Err("entity is not a sentient".into());
        }
        let origin = super::natives::engine::origin_of(world, receiver)?;
        let node = super::actors::navigation::nearest(world, id, origin);
        let runtime = world.resource::<RoundScript>();
        Ok(Value::Int(i32::from(
            node.and_then(|node| runtime.path_nodes.at(node))
                .is_some_and(|node| node.spawnflags & 0x2000 != 0),
        )))
    });
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
            super::actors::targeting::dissociate(&mut runtime, id);
            runtime.sentients.remove(&id);
            Ok(Value::Undefined)
        },
    );
}
