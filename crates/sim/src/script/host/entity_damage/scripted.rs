use super::*;
use crate::script::host::{args, natives::engine::entity_id, players};
use crate::script::{Namespace, NativeRegistry};

// Kill exposes this damage amount to script observers. Half the signed range
// leaves headroom for health subtraction while exceeding ordinary entity health.
const SCRIPT_KILL_DAMAGE: i32 = i32::MAX / 2;
// The PC DoDamage method attributes an unspecified scripted hit to the head.
const SCRIPT_DAMAGE_HITLOC: u8 = 2;

fn position(world: &mut World, object: u64) -> Result<[f32; 3], String> {
    match players::entity_field(world, object, "origin") {
        Value::Vector(at) if at.into_iter().all(f32::is_finite) => Ok(at),
        _ => Err("damage target origin must be a finite vector".into()),
    }
}

fn source(args: &[Value], index: usize) -> Result<[f32; 3], String> {
    let at = args::vector(args, index)?;
    if !at.into_iter().all(f32::is_finite) {
        return Err("damage source position must be finite".into());
    }
    Ok(at)
}

fn attribution(world: &World, args: &[Value], index: usize) -> Result<Value, String> {
    match args.get(index) {
        Some(value) => entity_id(world, value).map(Value::Object),
        None => Ok(world
            .resource::<RoundScript>()
            .engine
            .world
            .map_or(Value::Undefined, Value::Object)),
    }
}

fn weapon(world: &mut World, inflictor: &Value) -> u32 {
    let Value::Object(object) = inflictor else {
        return 0;
    };
    let client = world.resource::<RoundScript>().player_client(*object);
    if let Some(client) = client {
        return FrameWorld::from_world(world)
            .player(ClientId(client))
            .map_or(0, |ps| ps.weapon);
    }
    match world
        .resource_mut::<RoundScript>()
        .object_field(*object, "weapon")
    {
        Value::String(name) => FrameWorld::from_world(world)
            .weapon_index_by_script_name(&name)
            .unwrap_or(0),
        _ => 0,
    }
}

fn hit(
    world: &mut World,
    object: u64,
    amount: i32,
    point: [f32; 3],
    attacker: Value,
    inflictor: Value,
) -> Result<ObjectHit, String> {
    // Compute in f64 so finite script coordinates cannot overflow normalization.
    let at = position(world, object)?;
    let delta = std::array::from_fn::<_, 3, _>(|i| f64::from(at[i]) - f64::from(point[i]));
    let length = delta.iter().map(|n| n * n).sum::<f64>().sqrt();
    Ok(ObjectHit {
        amount,
        attacker,
        weapon: weapon(world, &inflictor),
        inflictor,
        means: "MOD_UNKNOWN",
        point,
        dir: if length > 0.0 {
            delta.map(|n| (n / length) as f32)
        } else {
            [0.0; 3]
        },
        bone: None,
        hitloc: 0,
        flags: 0,
    })
}

pub(crate) fn register(registry: &mut NativeRegistry) {
    registry.register(Namespace::Method, "dodamage", |world, receiver, args| {
        if !(2..=4).contains(&args.len()) {
            return Err(
                "DoDamage expects amount, source position, optional attacker and inflictor".into(),
            );
        }
        let object = entity_id(world, receiver)?;
        let amount = args::float(args, 0)?;
        if !amount.is_finite() {
            return Err("damage amount must be finite".into());
        }
        let point = source(args, 1)?;
        let attacker = attribution(world, args, 2)?;
        let inflictor = attribution(world, args, 3)?;
        let mut hit = hit(world, object, amount as i32, point, attacker, inflictor)?;
        hit.hitloc = SCRIPT_DAMAGE_HITLOC;
        Ok(Value::Int(i32::from(apply_object_hit(world, object, &hit))))
    });
    registry.register(Namespace::Method, "kill", |world, receiver, args| {
        if args.len() > 3 {
            return Err("Kill expects optional source position, attacker and inflictor".into());
        }
        let object = entity_id(world, receiver)?;
        if world
            .resource::<RoundScript>()
            .actors
            .get(&object)
            .is_some_and(|actor| actor.damage_shield)
        {
            return Err("cannot Kill an actor with damageShield enabled".into());
        }
        let point = if args.is_empty() {
            position(world, object)?
        } else {
            source(args, 0)?
        };
        let attacker = attribution(world, args, 1)?;
        let inflictor = attribution(world, args, 2)?;
        let hit = hit(
            world,
            object,
            SCRIPT_KILL_DAMAGE,
            point,
            attacker,
            inflictor,
        )?;
        let client = world.resource::<RoundScript>().player_client(object);
        world
            .resource_mut::<RoundScript>()
            .entities
            .get_mut(&object)
            .unwrap()
            .can_damage = true;
        if let Some(client) = client {
            world
                .resource_mut::<RoundScript>()
                .players
                .get_mut(&client)
                .unwrap()
                .campaign_damage
                .disable_death_shield();
            FrameWorld::from_world(world)
                .client_meta_mut(ClientId(client))
                .god_mode = false;
        }
        let health = players::entity_field(world, object, "health");
        let alive = match health {
            Value::Int(n) => n > 0,
            Value::Float(n) => n > 0.0,
            _ => false,
        };
        if alive && !apply_object_hit(world, object, &hit) {
            return Err("Kill damage was rejected by the entity".into());
        }
        Ok(Value::Undefined)
    });
}
