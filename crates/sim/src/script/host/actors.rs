use super::entities::EntityKind;
use super::natives::engine::entity_id;
use crate::script::runtime::{raise, run_now};
use crate::script::{Namespace, NativeRegistry, Runtime, Value};
use bevy_ecs::prelude::World;

/// The spawner keys an actor keeps as its own; the rest describe the spawner.
const SPAWNER_ONLY_FIELDS: [&str; 3] = ["count", "spawnflags", "export"];

pub(crate) fn register(registry: &mut NativeRegistry) {
    use Namespace::Method;
    registry.register(Method, "codespawnerforcespawn", |world, receiver, _| {
        spawn_from(world, receiver)
    });
    registry.register(Method, "dospawn", |world, receiver, _| {
        spawn_from(world, receiver)
    });
}

/// Spawns an actor from a map spawner: it takes the spawner's place and keys,
/// runs its aitype script, and the spawner is told `spawned`.
fn spawn_from(world: &mut World, spawner: &Value) -> Result<Value, String> {
    let spawner_id = entity_id(world, spawner)?;
    let (classname, count) = {
        let mut runtime = world.resource_mut::<Runtime>();
        let classname = runtime
            .entities
            .get(&spawner_id)
            .map(|entity| entity.classname.clone())
            .ok_or("spawner is gone")?;
        (classname, runtime.object_field(spawner_id, "count"))
    };
    let Some(aitype) = classname.strip_prefix("actor_") else {
        return Err(format!("{classname} is not an actor spawner"));
    };
    if matches!(count, Value::Int(n) if n <= 0) {
        return Ok(Value::Undefined);
    }
    let (origin, angles) = {
        let mut runtime = world.resource_mut::<Runtime>();
        let pose = |runtime: &mut Runtime, name| match runtime.object_field(spawner_id, name) {
            Value::Vector(v) => v,
            _ => [0.0; 3],
        };
        (pose(&mut runtime, "origin"), pose(&mut runtime, "angles"))
    };

    let presence = super::presence::spawn_presence(world, origin)?;
    let number = crate::frame::FrameWorld::from_world(world).gentity_number(presence);
    let id = {
        let mut runtime = world.resource_mut::<Runtime>();
        let id = runtime.create_entity(EntityKind::Actor, &classname)?;
        let skipped: Vec<u32> = SPAWNER_ONLY_FIELDS
            .iter()
            .map(|name| runtime.symbol(name))
            .collect();
        let inherited: Vec<(u32, Value)> = runtime
            .objects
            .get(&spawner_id)
            .into_iter()
            .flatten()
            .filter(|(symbol, _)| !skipped.contains(symbol))
            .map(|(symbol, value)| (*symbol, value.clone()))
            .collect();
        if let Some(fields) = runtime.objects.get_mut(&id) {
            fields.extend(inherited);
        }
        runtime.set_object_field(id, "origin", Value::Vector(origin));
        runtime.set_object_field(id, "angles", Value::Vector(angles));
        runtime.set_object_field(id, "code_classname", Value::string("actor"));
        runtime.set_object_field(id, "movemode", Value::string("run"));
        runtime.set_object_field(id, "ignoreme", Value::Int(0));
        runtime.set_object_field(id, "ignoreall", Value::Int(0));
        if let Value::Int(n) = count {
            runtime.set_object_field(spawner_id, "count", Value::Int(n - 1));
        }
        let entity = runtime.entities.get_mut(&id).ok_or("actor is gone")?;
        entity.presence = Some(presence);
        entity.can_damage = true;
        entity.can_radius_damage = true;
        if let Some(number) = number {
            entity.number = number;
        }
        id
    };

    let now = super::players::now_ms(world);
    let main = format!("aitype/{aitype}::main");
    run_now(world, &main, Value::Object(id), Vec::new(), now)
        .map_err(|fault| format!("{main}: {fault:?}"))?;
    let dog = matches!(
        world.resource_mut::<Runtime>().object_field(id, "type"),
        Value::String(kind) if kind.as_bytes() == b"zombie_dog"
    );
    {
        let mut runtime = world.resource_mut::<Runtime>();
        runtime.set_object_field(id, "isdog", Value::Int(dog.into()));
        runtime.set_object_field(id, "delayeddeath", Value::Int(0));
    }
    {
        let mut runtime = world.resource_mut::<Runtime>();
        let model = runtime.object_field(id, "model");
        let entity = &runtime.entities[&id];
        diag::debug!(
            Sim,
            "actor {id}: spawned from {classname} model={model:?} attachments={:?} hidden={}",
            entity.attachments,
            entity.hidden
        );
    }
    let tree = if dog { "zombie_dog" } else { "generic_human" };
    let _ = super::actor_anims::attach(world, id, tree);
    super::actor_brain::begin(world, id)?;
    raise(world, spawner.clone(), "spawned", vec![Value::Object(id)]);
    Ok(Value::Object(id))
}

const ACTOR_DAMAGE: &str = "maps/_callbacksetup::codecallback_actordamage";
const ACTOR_KILLED: &str = "maps/_callbacksetup::codecallback_actorkilled";

/// A hit on an actor goes to the actor damage callback, which finishes it.
pub(crate) fn damage(
    world: &mut World,
    actor: u64,
    hit: &super::entity_damage::EntityHit,
    attacker: Value,
    weapon: &str,
    tag: &str,
) {
    diag::debug!(
        Sim,
        "actor {actor}: hit {} {} by {weapon} at {tag:?}",
        hit.amount,
        hit.means
    );
    let args = vec![
        attacker.clone(),
        attacker,
        Value::Int(hit.amount),
        Value::Int(hit.flags),
        Value::string(hit.means),
        Value::string(weapon),
        Value::Vector(hit.point),
        Value::Vector(hit.dir),
        Value::string(hit_location(tag)),
        Value::Int(0),
        Value::Int(0),
    ];
    let now = super::players::now_ms(world);
    if let Err(fault) = run_now(world, ACTOR_DAMAGE, Value::Object(actor), args.clone(), now) {
        diag::warn!(Sim, "actor damage callback: {fault:?}");
        let _ = finish_damage(world, actor, &args);
    }
}

/// `FinishActorDamage(inflictor, attacker, damage, flags, means, weapon,
/// point, dir, hitLoc, modelIndex, timeOffset)`: applies the damage the
/// callback settled on and kills the actor at zero health.
fn finish_damage(world: &mut World, actor: u64, args: &[Value]) -> Result<Value, String> {
    let amount = match args.get(2) {
        Some(Value::Int(n)) => *n,
        Some(Value::Float(f)) => *f as i32,
        _ => return Err("damage must be a number".into()),
    };
    let arg = |index: usize| args.get(index).cloned().unwrap_or(Value::Undefined);
    let (before, model) = {
        let mut runtime = world.resource_mut::<Runtime>();
        let before = match runtime.object_field(actor, "health") {
            Value::Int(n) => n,
            Value::Float(f) => f as i32,
            _ => 0,
        };
        runtime.set_object_field(actor, "health", Value::Int(before - amount));
        // The last hit, as the engine records it on the actor for scripts.
        runtime.set_object_field(actor, "damagetaken", Value::Int(amount));
        runtime.set_object_field(actor, "damagemod", arg(4));
        runtime.set_object_field(actor, "damageweapon", arg(5));
        runtime.set_object_field(actor, "damagedir", arg(7));
        runtime.set_object_field(actor, "damagelocation", arg(8));
        if !matches!(arg(1), Value::Undefined) {
            runtime.set_object_field(actor, "attacker", arg(1));
        }
        (before, runtime.object_field(actor, "model"))
    };
    raise(
        world,
        Value::Object(actor),
        "damage",
        vec![
            Value::Int(amount),
            arg(1),
            arg(7),
            arg(6),
            arg(4),
            model,
            Value::string(""),
            Value::string(""),
            arg(3),
            arg(5),
        ],
    );
    if before > 0 && before - amount <= 0 {
        let killed = vec![
            arg(0),
            arg(1),
            Value::Int(amount),
            arg(4),
            arg(5),
            arg(7),
            arg(8),
            Value::Int(0),
        ];
        let now = super::players::now_ms(world);
        if let Err(fault) = run_now(world, ACTOR_KILLED, Value::Object(actor), killed, now) {
            diag::warn!(Sim, "actor killed callback: {fault:?}");
        }
        raise(world, Value::Object(actor), "death", vec![arg(1)]);
        super::actor_brain::kill(world, actor);
    }
    Ok(Value::Undefined)
}

pub(crate) fn register_damage(registry: &mut NativeRegistry) {
    registry.register(
        Namespace::Method,
        "finishactordamage",
        |world, receiver, args| {
            let id = entity_id(world, receiver)?;
            finish_damage(world, id, args)
        },
    );
}

/// The hit location a bone falls in.
fn hit_location(tag: &str) -> &'static str {
    let tag = tag.to_ascii_lowercase();
    let side = |left: &'static str, right: &'static str| {
        if tag.ends_with("_le") { left } else { right }
    };
    if tag.contains("head") || tag.contains("helmet") || tag.contains("eye") {
        "head"
    } else if tag.contains("neck") {
        "neck"
    } else if tag.contains("shoulder") || tag.contains("bicep") {
        side("left_arm_upper", "right_arm_upper")
    } else if tag.contains("elbow") {
        side("left_arm_lower", "right_arm_lower")
    } else if tag.contains("wrist") || tag.contains("hand") || tag.contains("finger") {
        side("left_hand", "right_hand")
    } else if tag.contains("hip") {
        side("left_leg_upper", "right_leg_upper")
    } else if tag.contains("knee") {
        side("left_leg_lower", "right_leg_lower")
    } else if tag.contains("ankle") || tag.contains("ball") {
        side("left_foot", "right_foot")
    } else if tag.contains("mainroot") || tag.contains("pelvis") || tag.contains("spinelower") {
        "torso_lower"
    } else {
        "torso_upper"
    }
}
