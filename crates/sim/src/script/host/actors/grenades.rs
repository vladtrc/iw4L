use super::*;
use crate::frame::FrameWorld;
use crate::script::host::{args, players};

mod held;
mod response;
mod trajectory;
pub(crate) use held::{Contact, advance, contact_value, observe, release};
pub(crate) use response::{Response, behavior, cancel, facing, movement_goal, think};

// Script API defaults, in milliseconds and world units, recovered from SP.
const MAGIC_FUSE_MS: i32 = 5000;
const SAFETY_MARGIN_UNITS: f64 = 128.0;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Method {
    Energy,
    Fast,
    Slow,
    Unlimited,
}

#[derive(Clone, Debug)]
pub(crate) struct Plan {
    origin: [f32; 3],
    velocity: [f32; 3],
    offset: [f32; 3],
    methods: Vec<Method>,
    bounce: bool,
    track: bool,
    target: Target,
}

#[derive(Clone, Copy, Debug)]
struct Target {
    position: [f32; 3],
    velocity: [f32; 3],
}

fn vector(args: &[Value], index: usize) -> Result<[f32; 3], String> {
    let point = args::vector(args, index)?;
    if !point.into_iter().all(f32::is_finite) {
        return Err("grenade vector must be finite".into());
    }
    Ok(point)
}

fn position(world: &mut World, object: u64) -> Result<[f32; 3], String> {
    let value = players::entity_field(world, object, "origin");
    vector(&[value], 0)
}

fn local_origin(world: &mut World, actor: u64, offset: [f32; 3]) -> Result<[f32; 3], String> {
    let at = position(world, actor)?;
    let value = players::entity_field(world, actor, "angles");
    let angles = vector(&[value], 0)?;
    let axis = math_iw4::angles_to_axis(angles);
    let point =
        std::array::from_fn(|i| at[i] + (0..3).map(|j| axis[j][i] * offset[j]).sum::<f32>());
    vector(&[Value::Vector(point)], 0)
}

fn owner(world: &World, actor: Option<u64>) -> Result<crate::ProjectileOwner, String> {
    let Some(actor) = actor else {
        return Ok(crate::ProjectileOwner::World);
    };
    let runtime = world.resource::<RoundScript>();
    let entity = &runtime.entities[&actor];
    Ok(crate::ProjectileOwner::Entity {
        model: entity
            .presence
            .ok_or("grenade actor has no physical presence")?,
        number: entity.number,
    })
}

fn weapon(world: &mut World, actor: u64) -> Result<u32, String> {
    let name = world
        .resource_mut::<RoundScript>()
        .object_field(actor, "grenadeweapon");
    let Value::String(name) = name else {
        return Err("actor has no grenade weapon".into());
    };
    FrameWorld::from_world(world)
        .weapon_index_by_script_name(&name)
        .ok_or("actor grenade weapon is unknown".into())
}

fn facts(world: &mut World, weapon: u32) -> Result<crate::EquipmentRuntimeFacts, String> {
    let facts = FrameWorld::from_world(world)
        .equipment_facts_for(weapon)
        .ok_or("grenade equipment facts are unavailable")?;
    if facts.weap_type != weapon_iw4::WEAPTYPE_GRENADE || !facts.is_usable() {
        return Err("weapon is not an executable grenade".into());
    }
    if !facts.proj_impact_explode
        && facts.stickiness == 0
        && (facts.parallel_bounce.is_none() || facts.perpendicular_bounce.is_none())
    {
        return Err("grenade surface bounce data is unavailable".into());
    }
    Ok(facts)
}

fn ammo(world: &mut World, actor: u64) -> i32 {
    match world
        .resource_mut::<RoundScript>()
        .object_field(actor, "grenadeammo")
    {
        Value::Int(n) => n,
        Value::Float(n) => n as i32,
        _ => 0,
    }
}

fn methods(args: &[Value], first: usize) -> Result<Vec<Method>, String> {
    if args.len() < first {
        return Err("not enough grenade throw parameters".into());
    }
    if args.len().saturating_sub(first) > 3 {
        return Err("at most three grenade toss methods are allowed".into());
    }
    args[first..]
        .iter()
        .map(|value| {
            let Value::String(name) = value else {
                return Err("grenade toss method must be a string".into());
            };
            match &**name {
                "min energy" => Ok(Method::Energy),
                "min time" => Ok(Method::Fast),
                "max time" => Ok(Method::Slow),
                "infinite energy" => Ok(Method::Unlimited),
                _ => Err(format!("unknown grenade toss method '{name}'")),
            }
        })
        .collect()
}

fn enemy_target(world: &mut World, actor: u64) -> Result<Option<Target>, String> {
    let target = world.resource::<RoundScript>().sentients[&actor]
        .target
        .filter(|target| {
            world
                .resource::<RoundScript>()
                .sentients
                .contains_key(target)
        });
    let Some(target) = target else {
        return Ok(None);
    };
    let position = position(world, target)?;
    let client = world.resource::<RoundScript>().player_client(target);
    let velocity = if let Some(client) = client {
        FrameWorld::from_world(world)
            .player(crate::ClientId(client))
            .map_or([0.; 3], |ps| ps.velocity)
    } else {
        world
            .resource::<RoundScript>()
            .actors
            .get(&target)
            .map_or([0.; 3], |state| state.motion.velocity)
    };
    Ok(Some(Target { position, velocity }))
}

fn fuse(args: &[Value], index: usize) -> Result<i32, String> {
    if args.len() == index {
        return Ok(MAGIC_FUSE_MS);
    }
    let seconds = args::float(args, index)?;
    if !seconds.is_finite() {
        return Err("grenade fuse must be finite".into());
    }
    Ok((seconds * 1000.0).min(crate::equipment::GRENADE_FUSE_CAP_MS as f32) as i32)
}

fn motion(origin: [f32; 3], velocity: [f32; 3], now: i32) -> crate::equipment::LaunchMotion {
    crate::equipment::LaunchMotion {
        origin,
        pos: entity_iw4::Trajectory {
            tr_type: entity_iw4::TR_GRAVITY,
            tr_time: now,
            tr_base: origin,
            tr_delta: velocity,
            ..Default::default()
        },
        apos: entity_iw4::Trajectory {
            tr_type: entity_iw4::TR_STATIONARY,
            tr_base: math_iw4::vect_to_angles(velocity),
            ..Default::default()
        },
        launch_time: now,
    }
}

fn launch(
    world: &mut World,
    actor: Option<u64>,
    weapon: u32,
    origin: [f32; 3],
    velocity: [f32; 3],
    fuse: i32,
) -> Result<Value, String> {
    facts(world, weapon)?;
    let owner = owner(world, actor)?;
    let tick = world.resource::<crate::step::StepRequest>().tick;
    let now = crate::level_time_ms(tick);
    let projectile = crate::equipment::spawn_script_grenade(
        &mut FrameWorld::from_world(world),
        owner,
        weapon,
        tick,
        motion(origin, velocity, now),
        fuse,
    )
    .ok_or("grenade could not be allocated")?;
    let object = match super::super::weapons::adopt(world, &projectile, "grenade") {
        Ok(object) => object,
        Err(error) => {
            FrameWorld::from_world(world).finish_projectile(projectile.entnum, None);
            return Err(error);
        }
    };
    let name = crate::script_player::weapon_name(&FrameWorld::from_world(world), weapon);
    {
        let mut runtime = world.resource_mut::<RoundScript>();
        runtime.set_object_field(object, "weaponname", Value::string(&name));
        runtime.set_object_field(object, "nativename", Value::string(&name));
    }
    if let Some(actor) = actor {
        crate::script::runtime::raise(
            world,
            Value::Object(actor),
            "grenade_fire",
            vec![Value::Object(object), Value::string(&name)],
        );
    }
    Ok(Value::Object(object))
}

fn check(
    world: &mut World,
    receiver: &Value,
    args: &[Value],
    explicit: bool,
) -> Result<Value, String> {
    let actor = actor_id(world, receiver)?;
    if ammo(world, actor) <= 0 {
        return Ok(Value::Undefined);
    }
    let mut target = if explicit {
        Target {
            position: vector(args, 1)?,
            velocity: [0.; 3],
        }
    } else {
        let Some(target) = enemy_target(world, actor)? else {
            return Ok(Value::Undefined);
        };
        target
    };
    let offset = vector(args, 0)?;
    let bounce = if explicit {
        args::int(args, 2)? != 0
    } else {
        true
    };
    let methods = methods(args, if explicit { 3 } else { 2 })?;
    if !explicit {
        let spread = args::float(args, 1)?;
        if !spread.is_finite() {
            return Err("grenade target spread must be finite".into());
        }
        let spread = spread.abs();
        if spread > 0.0 {
            let mut runtime = world.resource_mut::<RoundScript>();
            for axis in &mut target.position[..2] {
                *axis += crate::item::random_signed(&mut runtime.rng) * spread;
            }
        }
    }
    let weapon = weapon(world, actor)?;
    facts(world, weapon)?;
    let origin = local_origin(world, actor, offset)?;
    let owner = owner(world, Some(actor))?;
    let solution = trajectory::solve(
        world,
        owner,
        weapon,
        origin,
        target,
        trajectory::Options {
            methods: &methods,
            bounce,
        },
    )?;
    let plan = solution.map(|velocity| Plan {
        origin,
        velocity,
        offset,
        methods,
        bounce,
        track: !explicit,
        target,
    });
    let result = plan
        .as_ref()
        .map_or(Value::Undefined, |plan| Value::Vector(plan.velocity));
    world
        .resource_mut::<RoundScript>()
        .actors
        .get_mut(&actor)
        .unwrap()
        .grenade_throw = plan;
    Ok(result)
}

fn throw(world: &mut World, receiver: &Value, _: &[Value]) -> Result<Value, String> {
    let actor = actor_id(world, receiver)?;
    let health = world
        .resource_mut::<RoundScript>()
        .object_field(actor, "health");
    let health = match health {
        Value::Int(n) => n,
        Value::Float(n) => n as i32,
        _ => 0,
    };
    if health <= 0 {
        held::release(world, actor)?;
        world
            .resource_mut::<RoundScript>()
            .actors
            .get_mut(&actor)
            .unwrap()
            .grenade_throw = None;
        return Ok(Value::Undefined);
    }
    let plan = world.resource::<RoundScript>().actors[&actor]
        .grenade_throw
        .clone()
        .ok_or("ThrowGrenade requires a successful grenade throw check")?;
    let held = held::projectile(world, actor);
    let weapon = if let Some(projectile) = held {
        projectile.weapon
    } else {
        weapon(world, actor)?
    };
    let equipment = facts(world, weapon)?;
    let mut origin = plan.origin;
    let mut velocity = plan.velocity;
    if plan.track
        && let Some(target) = enemy_target(world, actor)?
    {
        let from = local_origin(world, actor, plan.offset)?;
        if let Some(new) = trajectory::solve(
            world,
            owner(world, Some(actor))?,
            weapon,
            from,
            target,
            trajectory::Options {
                methods: &plan.methods,
                bounce: plan.bounce,
            },
        )? {
            origin = from;
            velocity = new;
        }
    }
    let fuse = if equipment.fuse_time_ms > 0 {
        equipment.fuse_time_ms
    } else {
        crate::equipment::GRENADE_DEFAULT_FUSE_MS
    };
    if let Some(projectile) = held {
        origin = held::hand(world, actor)?.0;
        let target = if plan.track {
            enemy_target(world, actor)?.unwrap_or(plan.target)
        } else {
            plan.target
        };
        if let Some(rechecked) = trajectory::solve(
            world,
            owner(world, Some(actor))?,
            weapon,
            origin,
            target,
            trajectory::Options {
                methods: &plan.methods,
                bounce: plan.bounce,
            },
        )? {
            velocity = rechecked;
        }
        held::rethrow(world, actor, projectile, origin, velocity)?;
    } else {
        launch(world, Some(actor), weapon, origin, velocity, fuse)?;
    }
    let remaining = ammo(world, actor);
    let mut runtime = world.resource_mut::<RoundScript>();
    runtime.actors.get_mut(&actor).unwrap().grenade_throw = None;
    if held.is_none() && remaining > 0 {
        runtime.set_object_field(actor, "grenadeammo", Value::Int(remaining - 1));
    }
    Ok(Value::Undefined)
}

fn magic(
    world: &mut World,
    receiver: &Value,
    args: &[Value],
    manual: bool,
    method: bool,
) -> Result<Value, String> {
    let first = usize::from(!method);
    if args.len() != first + 2 && args.len() != first + 3 {
        return Err("MagicGrenade expects origin, target/velocity, and optional fuse; global form also requires weapon".into());
    }
    let actor = if method {
        Some(actor_id(world, receiver)?)
    } else {
        None
    };
    let weapon = if let Some(actor) = actor {
        weapon(world, actor)?
    } else {
        let name = args::string(args, 0)?;
        FrameWorld::from_world(world)
            .weapon_index_by_script_name(&name)
            .ok_or("unknown grenade weapon")?
    };
    facts(world, weapon)?;
    let from = vector(args, first)?;
    let to = vector(args, first + 1)?;
    let fuse = fuse(args, first + 2)?;
    let velocity = if manual {
        to
    } else {
        let Some(velocity) = trajectory::solve(
            world,
            owner(world, actor)?,
            weapon,
            from,
            Target {
                position: to,
                velocity: [0.; 3],
            },
            trajectory::Options {
                methods: &[Method::Energy, Method::Fast, Method::Slow],
                bounce: true,
            },
        )?
        else {
            return Ok(Value::Undefined);
        };
        velocity
    };
    launch(world, actor, weapon, from, velocity, fuse)
}

pub(super) fn register(registry: &mut NativeRegistry) {
    registry.register(
        Namespace::Method,
        "isgrenadepossafe",
        |world, receiver, args| {
            let actor = actor_id(world, receiver)?;
            super::super::natives::engine::entity_id(world, args::arg(args, 0)?)?;
            let at = vector(args, 1)?;
            let radius = if args.len() > 2 {
                args::float(args, 2)?
            } else {
                0.0
            };
            if !radius.is_finite() {
                return Err("grenade safety radius must be finite".into());
            }
            let radius = if radius > 0.0 {
                f64::from(radius)
            } else {
                let weapon = weapon(world, actor)?;
                f64::from(facts(world, weapon)?.explosion_radius.max(0)) + SAFETY_MARGIN_UNITS
            };
            let team = world.resource::<RoundScript>().sentients[&actor].team;
            let friends: Vec<_> = world
                .resource::<RoundScript>()
                .sentients
                .iter()
                .filter(|(_, state)| state.team == team)
                .map(|(&id, _)| id)
                .collect();
            let content = FrameWorld::from_world(world).content();
            for friend in friends {
                let Some(pose) = content.simulation_pose_in(position(world, friend)?, at) else {
                    continue;
                };
                let origin = pose.w_axis.truncate().to_array();
                if (0..3)
                    .map(|i| (f64::from(at[i]) - f64::from(origin[i])).powi(2))
                    .sum::<f64>()
                    <= radius * radius
                {
                    return Ok(Value::Int(0));
                }
            }
            Ok(Value::Int(1))
        },
    );
    registry.register(Namespace::Method, "checkgrenadethrow", |w, r, a| {
        check(w, r, a, false)
    });
    registry.register(Namespace::Method, "checkgrenadethrowpos", |w, r, a| {
        check(w, r, a, true)
    });
    registry.register(Namespace::Method, "throwgrenade", throw);
    registry.register(Namespace::Method, "pickupgrenade", held::pickup);
    registry.register(Namespace::Method, "magicgrenade", |w, r, a| {
        magic(w, r, a, false, true)
    });
    registry.register(Namespace::Function, "magicgrenade", |w, r, a| {
        magic(w, r, a, false, false)
    });
    registry.register(Namespace::Method, "magicgrenademanual", |w, r, a| {
        magic(w, r, a, true, true)
    });
    registry.register(Namespace::Function, "magicgrenademanual", |w, r, a| {
        magic(w, r, a, true, false)
    });
}
