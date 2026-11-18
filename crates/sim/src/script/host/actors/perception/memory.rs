use super::*;

fn target(world: &World, args: &[Value]) -> Result<u64, String> {
    super::super::super::natives::engine::entity_id(world, arg(args, 0)?)
}

fn now(world: &World) -> i32 {
    super::super::super::players::now_ms(world) as i32
}

fn known_enemies(world: &mut World, id: u64) -> Vec<[f32; 3]> {
    // SP's region queries retain known positions for ten seconds. Do not
    // substitute a target's current position when it has moved out of sight.
    const MEMORY_MS: i32 = 10_000;
    let now = now(world);
    let candidates: Vec<_> = {
        let runtime = world.resource::<RoundScript>();
        let team = runtime.sentients[&id].team;
        runtime.actors[&id]
            .sentient_info
            .iter()
            .filter_map(|(&target, info)| {
                let position = info.last_known_position?;
                let sentient = runtime.sentients.get(&target)?;
                (runtime.live(&target)
                    && sentient.team != team
                    && now.saturating_sub(info.last_known_time) <= MEMORY_MS
                    && runtime
                        .actors
                        .get(&target)
                        .is_none_or(|actor| actor.alive && !actor.delayed_death))
                .then_some((target, position))
            })
            .collect()
    };
    candidates
        .into_iter()
        .filter_map(|(target, position)| {
            let health = super::super::super::players::entity_field(world, target, "health");
            let alive = match health {
                Value::Int(n) => n > 0,
                Value::Float(n) => n > 0.0,
                _ => false,
            };
            alive.then_some(position)
        })
        .collect()
}

pub(super) fn register(registry: &mut NativeRegistry) {
    registry.register(
        Namespace::Method,
        "lastknowntime",
        |world, receiver, args| {
            let id = actor_id(world, receiver)?;
            let target = target(world, args)?;
            let runtime = world.resource::<RoundScript>();
            if !runtime.sentients.contains_key(&target) {
                return Err("lastknowntime requires a sentient target".into());
            }
            Ok(Value::Int(
                runtime.actors[&id]
                    .sentient_info
                    .get(&target)
                    .map_or(0, |info| info.last_known_time),
            ))
        },
    );
    registry.register(
        Namespace::Method,
        "lastknownpos",
        |world, receiver, args| {
            let id = actor_id(world, receiver)?;
            let target = target(world, args)?;
            let now = now(world);
            let runtime = world.resource::<RoundScript>();
            if runtime.sentients.contains_key(&target)
                && let Some(info) = runtime.actors[&id].sentient_info.get(&target)
                && !info.visible
                && info.last_known_time > 0
                && now > info.last_known_time
                && let Some(position) = info.last_known_position
            {
                return Ok(Value::Vector(position));
            }
            Ok(super::super::super::players::entity_field(
                world, target, "origin",
            ))
        },
    );
    registry.register(
        Namespace::Method,
        "getenemyinfo",
        |world, receiver, args| {
            let id = actor_id(world, receiver)?;
            let target = target(world, args)?;
            if !world
                .resource::<RoundScript>()
                .sentients
                .contains_key(&target)
            {
                return Ok(Value::Undefined);
            }
            let Value::Vector(position) =
                super::super::super::players::entity_field(world, target, "origin")
            else {
                return Err("sentient has no origin".into());
            };
            let now = now(world);
            let mut runtime = world.resource_mut::<RoundScript>();
            let info = runtime
                .actors
                .get_mut(&id)
                .unwrap()
                .sentient_info
                .entry(target)
                .or_default();
            info.last_known_position = Some(position);
            info.last_known_time = now;
            // Learning a reported location does not prove line of sight.
            Ok(Value::Undefined)
        },
    );
    registry.register(Namespace::Method, "seerecently", |world, receiver, args| {
        let id = actor_id(world, receiver)?;
        let target = target(world, args)?;
        if world
            .resource::<RoundScript>()
            .sentients
            .contains_key(&target)
        {
            let seconds = float(args, 1)?;
            if !seconds.is_finite() || seconds < 0.0 || seconds > i32::MAX as f32 / 1000.0 {
                return Err(
                    "visibility interval must be finite, nonnegative and representable".into(),
                );
            }
            let window = (seconds * 1000.0) as i32;
            let now = now(world);
            if let Some(info) = world.resource::<RoundScript>().actors[&id]
                .sentient_info
                .get(&target)
                && info
                    .visibility_updated_at
                    .is_some_and(|time| now <= time.saturating_add(window))
            {
                return Ok(Value::Int(i32::from(
                    info.last_visible_time
                        .is_some_and(|time| now <= time.saturating_add(window)),
                )));
            }
        }
        Ok(Value::Int(i32::from(super::sight::can_see(
            world, id, target, 0,
        )?)))
    });
    registry.register(
        Namespace::Method,
        "isknownenemyinradius",
        |world, receiver, args| {
            let id = actor_id(world, receiver)?;
            let center = super::super::super::args::vector(args, 0)?;
            let radius = float(args, 1)?;
            if !center.into_iter().all(f32::is_finite) || !radius.is_finite() || radius < 0.0 {
                return Err("enemy region requires a finite center and nonnegative radius".into());
            }
            let found = radius > 0.0
                && known_enemies(world, id).into_iter().any(|position| {
                    // The script's region is a cylinder with an 80-unit vertical
                    // tolerance, independent of its horizontal radius.
                    const REGION_HEIGHT_TOLERANCE: f32 = 80.0;
                    (position[0] - center[0]).hypot(position[1] - center[1]) <= radius
                        && (position[2] - center[2]).abs() <= REGION_HEIGHT_TOLERANCE
                });
            // Despite their names, both PC and Xbox wrappers expose !found.
            Ok(Value::Int(i32::from(!found)))
        },
    );
    registry.register(
        Namespace::Method,
        "isknownenemyinvolume",
        |world, receiver, args| {
            let id = actor_id(world, receiver)?;
            let volume = target(world, args)?;
            for position in known_enemies(world, id) {
                if super::super::super::triggers::contact_point(world, volume, position)? {
                    return Ok(Value::Int(0));
                }
            }
            Ok(Value::Int(1))
        },
    );
}
