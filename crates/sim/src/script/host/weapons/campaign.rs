use super::super::entities::EntityKind;
use crate::frame::FrameWorld;
use crate::script::{RoundScript, Value};
use crate::world::ClientId;
use bevy_ecs::prelude::World;

pub(crate) fn owner(world: &World) -> Option<ClientId> {
    let runtime = world.resource::<RoundScript>();
    if runtime.campaign.nodes.is_empty() {
        return None;
    }
    runtime.players.keys().next().copied().map(ClientId)
}

pub(crate) fn place_weapon(
    world: &mut World,
    name: &str,
    origin: [f32; 3],
    angles: [f32; 3],
) -> Result<Option<i32>, String> {
    let tick = world.resource::<crate::step::StepRequest>().tick;
    let mut frame = FrameWorld::from_world(world);
    let weapon = crate::script_player::weapon_named(&frame, name)?;
    Ok(crate::item::place_weapon(
        &mut frame, tick, weapon, origin, angles,
    ))
}

fn combat_facts(
    world: &mut World,
    args: &[Value],
) -> Result<weapon_iw4::WeaponCombatFacts, String> {
    let name = super::super::args::string(args, 0)?;
    if name == "none" || name.is_empty() {
        return Ok(Default::default());
    }
    let frame = FrameWorld::from_world(world);
    let index = frame
        .weapon_index_by_script_name(&name)
        .ok_or_else(|| format!("unknown weapon '{name}'"))?;
    Ok(frame.weapon_combat_row(index).unwrap_or_default())
}

const GRENADE_GRAVITY: f32 = 800.0;

#[derive(Clone, Debug)]
pub(crate) struct MagicGrenade {
    due: i64,
    at: [f32; 3],
    weapon: u32,
    model: Option<u64>,
    flight: Option<(crate::equipment::ProjectileState, i32)>,
    team: Value,
}

pub(crate) struct LiveGrenade {
    pub model: u64,
    pub at: [f32; 3],
    pub due: i64,
    pub weapon: u32,
    pub team: Value,
}

pub(crate) fn live_grenades(world: &World) -> Vec<LiveGrenade> {
    world
        .resource::<RoundScript>()
        .campaign
        .magic_grenades
        .iter()
        .filter_map(|g| {
            Some(LiveGrenade {
                model: g.model?,
                at: g.at,
                due: g.due,
                weapon: g.weapon,
                team: g.team.clone(),
            })
        })
        .collect()
}

pub(crate) fn delay_grenade(world: &mut World, model: u64, ms: i64) {
    let mut runtime = world.resource_mut::<RoundScript>();
    if let Some(g) = runtime
        .campaign
        .magic_grenades
        .iter_mut()
        .find(|g| g.model == Some(model))
    {
        g.due += ms;
    }
}

pub(crate) fn take_grenade(world: &mut World, model: u64) -> Option<(u32, i64)> {
    let now = super::super::players::now_ms(world);
    let mut runtime = world.resource_mut::<RoundScript>();
    let index = runtime
        .campaign
        .magic_grenades
        .iter()
        .position(|g| g.model == Some(model))?;
    let g = runtime.campaign.magic_grenades.remove(index);
    if runtime.entities.contains_key(&model) {
        runtime.request_delete(model);
    }
    Some((g.weapon, (g.due - now).max(0)))
}

fn arc(from: [f32; 3], at: [f32; 3], fuse_ms: i32) -> ([f32; 3], f32) {
    let d: [f32; 3] = std::array::from_fn(|k| at[k] - from[k]);
    let reach = (d[0] * d[0] + d[1] * d[1] + d[2] * d[2]).sqrt();
    let fuse = (fuse_ms as f32 / 1000.0).max(0.1);
    let t = (reach / 600.0).clamp(0.3, 1.5).min(fuse);
    (
        [d[0] / t, d[1] / t, d[2] / t + 0.5 * GRENADE_GRAVITY * t],
        t,
    )
}

struct Flight {
    model: Option<u64>,
    state: Option<(crate::equipment::ProjectileState, i32)>,
    lands: Option<[f32; 3]>,
}

fn fly(
    world: &mut World,
    weapon: u32,
    from: [f32; 3],
    velocity: [f32; 3],
    at: [f32; 3],
    fuse_ms: i32,
) -> Flight {
    let tick = world.resource::<crate::step::StepRequest>().tick;
    let time = crate::level_time_ms(tick);
    let frame = FrameWorld::from_world(world);
    let bounces = frame
        .equipment_facts_for(weapon)
        .is_some_and(|f| f.parallel_bounce.is_some() && f.perpendicular_bounce.is_some());
    let state = bounces.then(|| crate::equipment::ProjectileState {
        id: crate::ProjectileId(u32::MAX),
        owner: ClientId(u32::MAX),
        owner_life: crate::LifeSequence::default(),
        weapon,
        origin: from,
        velocity,
        pos: entity_iw4::init_grenade_pos(from, velocity, time),
        apos: Default::default(),
        entnum: -1,
        launch_time: time,
        spawn_time_ms: time,
        detonate_at_ms: None,
        cleanup_at_ms: i32::MAX,
        travel_distance: 0.0,
        live: true,
        stuck_pane: None,
        grounded: false,
        guide: Default::default(),
        attached_to: None,
    });
    let lands = state.and_then(|state| {
        crate::equipment::predict_projectile(&frame, state, time, fuse_ms.max(0))
    });
    let model = spawn_model(world, weapon, from);
    if state.is_none()
        && let Some(model) = model
    {
        ride_arc(world, model, from, at, fuse_ms);
    }
    Flight {
        model,
        state: state.map(|state| (state, time)),
        lands,
    }
}

fn spawn_model(world: &mut World, weapon: u32, from: [f32; 3]) -> Option<u64> {
    use crate::script::Namespace::{Function, Method};
    let model = FrameWorld::from_world(world)
        .weapon_world_model(weapon)?
        .0
        .to_owned();
    let natives = world.resource::<RoundScript>().restart.clone()?;
    let native = |namespace, name| natives.natives.get(namespace, name);
    let (spawn, set_model) = (native(Function, "spawn")?, native(Method, "setmodel")?);
    let entity = spawn(
        world,
        &Value::Undefined,
        &[Value::string("script_model"), Value::Vector(from)],
    )
    .ok()?;
    set_model(world, &entity, &[Value::string(&model)]).ok()?;
    match entity {
        Value::Object(id) => Some(id),
        _ => None,
    }
}

fn ride_arc(world: &mut World, model: u64, from: [f32; 3], at: [f32; 3], fuse_ms: i32) {
    let Some(move_gravity) = world
        .resource::<RoundScript>()
        .restart
        .clone()
        .and_then(|r| {
            r.natives
                .get(crate::script::Namespace::Method, "movegravity")
        })
    else {
        return;
    };
    let (velocity, t) = arc(from, at, fuse_ms);
    let _ = move_gravity(
        world,
        &Value::Object(model),
        &[Value::Vector(velocity), Value::Float(t)],
    );
}

const REST_STEP: f32 = 0.5;

fn step_flights(world: &mut World) {
    let tick = world.resource::<crate::step::StepRequest>().tick;
    let now = crate::level_time_ms(tick);
    let flights: Vec<(usize, crate::equipment::ProjectileState, i32)> = world
        .resource::<RoundScript>()
        .campaign
        .magic_grenades
        .iter()
        .enumerate()
        .filter_map(|(i, g)| g.flight.map(|(state, stepped)| (i, state, stepped)))
        .filter(|(_, state, stepped)| {
            *stepped < now && state.pos.tr_type != entity_iw4::TR_STATIONARY
        })
        .collect();
    if flights.is_empty() {
        return;
    }
    let frame = FrameWorld::from_world(world);
    let stepped: Vec<(usize, crate::equipment::ProjectileState)> = flights
        .into_iter()
        .map(|(i, state, from)| {
            let next = crate::equipment::predict_projectile_state(&frame, state, from, now - from)
                .filter(|next| {
                    let d: [f32; 3] = std::array::from_fn(|k| next.origin[k] - state.origin[k]);
                    (d[0] * d[0] + d[1] * d[1] + d[2] * d[2]).sqrt() >= REST_STEP
                });
            (i, next.unwrap_or_else(|| at_rest(state)))
        })
        .collect();
    let mut runtime = world.resource_mut::<RoundScript>();
    for (i, state) in stepped {
        let grenade = &mut runtime.campaign.magic_grenades[i];
        grenade.flight = Some((state, now));
        if let Some(model) = grenade.model
            && runtime.entities.contains_key(&model)
        {
            runtime.set_object_field(model, "origin", Value::Vector(state.origin));
        }
    }
}

fn at_rest(mut state: crate::equipment::ProjectileState) -> crate::equipment::ProjectileState {
    state.velocity = [0.0; 3];
    state.pos.tr_type = entity_iw4::TR_STATIONARY;
    state.pos.tr_base = state.origin;
    state.pos.tr_delta = [0.0; 3];
    state
}

fn magic_grenade(
    world: &mut World,
    receiver: &Value,
    args: &[Value],
    manual: bool,
) -> Result<Value, String> {
    use super::super::args::{float, optional, string, vector};
    let method = matches!(args.first(), Some(Value::Vector(_)));
    let (name, rest) = if method {
        let id = super::super::natives::engine::entity_id(world, receiver)?;
        let name = match world
            .resource_mut::<RoundScript>()
            .object_field(id, "grenadeweapon")
        {
            Value::String(name) => name.to_string(),
            _ => "fraggrenade".to_owned(),
        };
        (name, args)
    } else {
        (string(args, 0)?, &args[1..])
    };
    let frame = FrameWorld::from_world(world);
    let weapon = crate::script_player::weapon_named(&frame, &name)?;
    let fuse = frame
        .equipment_facts_for(weapon)
        .map_or(3000, |facts| facts.fuse_time_ms.max(0));
    let from = vector(rest, 0)?;
    let ms = optional(rest, 2, float)?.map_or(fuse, |s| (s * 1000.0) as i32);
    let (at, velocity) = if manual {
        let velocity = vector(rest, 1)?;
        let t = ms as f32 / 1000.0;
        let to = [
            from[0] + velocity[0] * t,
            from[1] + velocity[1] * t,
            from[2] + velocity[2] * t - 0.5 * GRENADE_GRAVITY * t * t,
        ];
        let trace = super::super::presence::settled(world).trace_world(
            from,
            to,
            [0.0; 3],
            [0.0; 3],
            crate::bullet_collision::MASK_SHOT,
        );
        let at = std::array::from_fn(|k| from[k] + (to[k] - from[k]) * trace.fraction);
        (at, velocity)
    } else {
        let at = vector(rest, 1)?;
        (at, arc(from, at, ms).0)
    };
    let due = super::super::players::now_ms(world) + i64::from(ms);
    let flight = fly(world, weapon, from, velocity, at, ms);
    let team = if method {
        let id = super::super::natives::engine::entity_id(world, receiver)?;
        world.resource_mut::<RoundScript>().object_field(id, "team")
    } else {
        Value::Undefined
    };
    world
        .resource_mut::<RoundScript>()
        .campaign
        .magic_grenades
        .push(MagicGrenade {
            due,
            at: flight.lands.unwrap_or(at),
            weapon,
            model: flight.model,
            flight: flight.state,
            team,
        });
    Ok(Value::Undefined)
}

pub(crate) fn schedule_grenade(
    world: &mut World,
    name: &str,
    from: [f32; 3],
    at: [f32; 3],
    team: Value,
    fuse: Option<i64>,
) -> Result<(), String> {
    let frame = FrameWorld::from_world(world);
    let weapon = crate::script_player::weapon_named(&frame, name)?;
    let fuse = fuse.map_or_else(
        || {
            frame
                .equipment_facts_for(weapon)
                .map_or(3000, |facts| facts.fuse_time_ms.max(0))
        },
        |ms| i32::try_from(ms).unwrap_or(i32::MAX),
    );
    let due = super::super::players::now_ms(world) + i64::from(fuse);
    let flight = fly(world, weapon, from, arc(from, at, fuse).0, at, fuse);
    world
        .resource_mut::<RoundScript>()
        .campaign
        .magic_grenades
        .push(MagicGrenade {
            due,
            at: flight.lands.unwrap_or(at),
            weapon,
            model: flight.model,
            flight: flight.state,
            team,
        });
    Ok(())
}

pub(crate) fn advance_magic_grenades(world: &mut World) {
    step_flights(world);
    let now = super::super::players::now_ms(world);
    let due: Vec<MagicGrenade> = {
        let mut runtime = world.resource_mut::<RoundScript>();
        let (due, later) = std::mem::take(&mut runtime.campaign.magic_grenades)
            .into_iter()
            .partition(|g| g.due <= now);
        runtime.campaign.magic_grenades = later;
        due
    };
    let tick = world.resource::<crate::step::StepRequest>().tick;
    for grenade in due {
        let at = grenade.flight.map_or(grenade.at, |(state, _)| state.origin);
        if let Some(model) = grenade.model {
            let mut runtime = world.resource_mut::<RoundScript>();
            if runtime.entities.contains_key(&model) {
                runtime.request_delete(model);
            }
        }
        let mut frame = FrameWorld::from_world(world);
        let Some(facts) = frame.equipment_facts_for(grenade.weapon) else {
            continue;
        };
        frame.push_entity_event(
            tick,
            crate::EventAudience::All,
            entity_iw4::EntityEventKind::GRENADE_EXPLODE,
            crate::EntityEventPayload {
                number: i32::from(trace_iw4::ENTITYNUM_WORLD),
                attacker_entity_num: i32::from(trace_iw4::ENTITYNUM_WORLD),
                weapon: grenade.weapon,
                origin: at,
                direction: [0.0, 0.0, 1.0],
                ..Default::default()
            },
        );
        world
            .resource_mut::<RoundScript>()
            .blasts
            .push(super::super::entity_damage::ScriptBlast {
                origin: at,
                radius: facts.explosion_radius.max(0) as f32,
                max: facts.explosion_inner_damage.max(0) as f32,
                min: facts.explosion_outer_damage.max(0) as f32,
                attacker: None,
                inflictor: None,
                means: "MOD_GRENADE_SPLASH",
                weapon: grenade.weapon,
            });
    }
}

const MAX_TARGETS: usize = 32;

fn target_on_screen(world: &mut World, args: &[Value]) -> Result<Option<(f32, f32)>, String> {
    use super::super::args::{arg, float};
    use super::super::natives::engine::entity_id;
    let target = entity_id(world, arg(args, 0)?)?;
    let player = entity_id(world, arg(args, 1)?)?;
    let fov = float(args, 2)?;
    let runtime = world.resource::<RoundScript>();
    let offset = runtime
        .campaign
        .targets
        .iter()
        .find(|(id, _)| *id == target)
        .map_or([0.0; 3], |(_, offset)| *offset);
    let client = runtime
        .player_client(player)
        .ok_or("target test needs a player")?;
    let Value::Vector(at) = super::super::players::entity_field(world, target, "origin") else {
        return Ok(None);
    };
    let Some(ps) = FrameWorld::from_world(world)
        .player(ClientId(client))
        .copied()
    else {
        return Ok(None);
    };
    let eye = glam::Vec3::from_array(ps.origin) + glam::Vec3::Z * ps.view_height_current;
    let delta = glam::Vec3::from_array(at) + glam::Vec3::from_array(offset) - eye;
    let (forward, right, up) = math_iw4::angle_vectors(ps.viewangles);
    let depth = delta.dot(glam::Vec3::from_array(forward));
    if depth <= 0.0 {
        return Ok(None);
    }
    let scale = 320.0 / (fov.to_radians() * 0.5).tan() / depth;
    Ok(Some((
        delta.dot(glam::Vec3::from_array(right)) * scale,
        delta.dot(glam::Vec3::from_array(up)) * scale,
    )))
}

fn register_targets(registry: &mut crate::script::NativeRegistry) {
    use super::super::args::{arg, float, optional, vector};
    use super::super::natives::engine::entity_id;
    use crate::script::Namespace::Function;
    registry.register(Function, "target_set", |world, _, args| {
        let id = entity_id(world, arg(args, 0)?)?;
        let offset = optional(args, 1, vector)?.unwrap_or([0.0; 3]);
        let targets = &mut world.resource_mut::<RoundScript>().campaign.targets;
        if let Some(target) = targets.iter_mut().find(|(target, _)| *target == id) {
            target.1 = offset;
        } else if targets.len() >= MAX_TARGETS {
            return Err("Maximum number of targets exceeded".into());
        } else {
            targets.push((id, offset));
        }
        Ok(Value::Undefined)
    });
    registry.register(Function, "target_remove", |world, _, args| {
        let id = entity_id(world, arg(args, 0)?)?;
        let targets = &mut world.resource_mut::<RoundScript>().campaign.targets;
        targets.retain(|(target, _)| *target != id);
        Ok(Value::Undefined)
    });
    registry.register(Function, "target_istarget", |world, _, args| {
        let id = entity_id(world, arg(args, 0)?)?;
        let targets = &world.resource::<RoundScript>().campaign.targets;
        Ok(Value::Int(
            targets.iter().any(|(target, _)| *target == id).into(),
        ))
    });
    registry.register(Function, "target_getarray", |world, _, _| {
        let mut runtime = world.resource_mut::<RoundScript>();
        let runtime = &mut *runtime;
        let entities = &runtime.entities;
        runtime
            .campaign
            .targets
            .retain(|(target, _)| entities.contains_key(target));
        let targets = runtime
            .campaign
            .targets
            .iter()
            .map(|(target, _)| Value::Object(*target))
            .collect();
        super::super::arrays::new_array(world, targets)
    });
    registry.register(Function, "target_isinrect", |world, _, args| {
        let (w, h) = (float(args, 3)?, float(args, 4)?);
        let inside =
            target_on_screen(world, args)?.is_some_and(|(x, y)| x.abs() < w && y.abs() < h);
        Ok(Value::Int(inside.into()))
    });
    registry.register(Function, "target_isincircle", |world, _, args| {
        let radius = float(args, 3)?;
        let inside =
            target_on_screen(world, args)?.is_some_and(|(x, y)| x * x + y * y < radius * radius);
        Ok(Value::Int(inside.into()))
    });
    registry.register(Function, "target_setjavelinonly", |world, _, args| {
        let id = entity_id(world, arg(args, 0)?)?;
        let javelin_only = optional(args, 1, super::super::args::int)?.unwrap_or(1) != 0;
        let mut runtime = world.resource_mut::<RoundScript>();
        runtime.set_object_field(id, "target_javelin_only", Value::Int(javelin_only.into()));
        Ok(Value::Undefined)
    });
    registry.register(Function, "target_setattackmode", |world, _, args| {
        let id = entity_id(world, arg(args, 0)?)?;
        let mode = super::super::args::string(args, 1)?;
        let mut runtime = world.resource_mut::<RoundScript>();
        runtime.set_object_field(id, "target_attack_mode", Value::string(&mode));
        Ok(Value::Undefined)
    });
    registry.register(Function, "target_setshader", |world, _, args| {
        let id = entity_id(world, arg(args, 0)?)?;
        let shader = super::super::args::string(args, 1)?;
        let mut runtime = world.resource_mut::<RoundScript>();
        runtime.set_object_field(id, "target_shader", Value::string(&shader));
        Ok(Value::Undefined)
    });
    registry.register(Function, "target_setoffscreenshader", |world, _, args| {
        let id = entity_id(world, arg(args, 0)?)?;
        let shader = super::super::args::string(args, 1)?;
        let mut runtime = world.resource_mut::<RoundScript>();
        runtime.set_object_field(id, "target_offscreenshader", Value::string(&shader));
        Ok(Value::Undefined)
    });
    registry.register(Function, "target_hidefromplayer", |world, _, args| {
        let id = entity_id(world, arg(args, 0)?)?;
        let mut runtime = world.resource_mut::<RoundScript>();
        runtime.set_object_field(id, "target_hidden_from_player", Value::Int(1));
        Ok(Value::Undefined)
    });
    registry.register(Function, "target_showtoplayer", |world, _, args| {
        let id = entity_id(world, arg(args, 0)?)?;
        let mut runtime = world.resource_mut::<RoundScript>();
        runtime.set_object_field(id, "target_hidden_from_player", Value::Int(0));
        Ok(Value::Undefined)
    });
}

pub(crate) fn register_sp(registry: &mut crate::script::NativeRegistry) {
    use crate::script::Namespace::{Function, Method};
    register_targets(registry);
    for namespace in [Function, Method] {
        registry.register(namespace, "magicgrenade", |world, receiver, args| {
            magic_grenade(world, receiver, args, false)
        });
        registry.register(namespace, "magicgrenademanual", |world, receiver, args| {
            magic_grenade(world, receiver, args, true)
        });
    }
    registry.register(Function, "weaponhasthermalscope", |world, _, args| {
        let name = super::super::args::string(args, 0)?;
        let frame = FrameWorld::from_world(world);
        Ok(Value::Int(
            frame
                .weapon_index_by_script_name(&name)
                .is_some_and(|w| frame.weapon_thermal_scope(w))
                .into(),
        ))
    });
    registry.register(Function, "weaponisauto", |world, _, args| {
        Ok(Value::Int(
            (combat_facts(world, args)?.fire_type == 0).into(),
        ))
    });
    registry.register(Function, "weaponburstcount", |world, _, args| {
        let fire_type = combat_facts(world, args)?.fire_type;
        Ok(Value::Int(if (2..=4).contains(&fire_type) {
            fire_type
        } else {
            0
        }))
    });
    registry.register(Function, "weaponisboltaction", |world, _, args| {
        Ok(Value::Int(combat_facts(world, args)?.bolt_action.into()))
    });
    registry.register(Function, "getweaponclipmodel", |world, _, args| {
        let name = super::super::args::string(args, 0)?;
        if name.is_empty() || name.eq_ignore_ascii_case("none") {
            return Ok(Value::String("".into()));
        }
        let frame = FrameWorld::from_world(world);
        let _weapon = crate::script_player::weapon_named(&frame, &name)?;
        Ok(Value::String("".into()))
    });
    registry.register(Method, "setstablemissile", |world, receiver, args| {
        let id = super::super::natives::engine::entity_id(world, receiver)?;
        let stable =
            super::super::args::optional(args, 0, super::super::args::int)?.unwrap_or(1) != 0;
        world.resource_mut::<RoundScript>().set_object_field(
            id,
            "stable_missile",
            Value::Int(stable.into()),
        );
        Ok(Value::Undefined)
    });
}

pub(crate) fn place_map_weapons(world: &mut World) {
    let weapons: Vec<(u64, String)> = world
        .resource::<RoundScript>()
        .entities
        .iter()
        .filter(|(_, e)| e.kind == EntityKind::Map)
        .filter_map(|(id, e)| Some((*id, e.classname.strip_prefix("weapon_")?.to_owned())))
        .collect();
    for (id, name) in weapons {
        let (origin, angles) = {
            let mut runtime = world.resource_mut::<RoundScript>();
            let vector = |value| match value {
                Value::Vector(v) => v,
                _ => [0.0; 3],
            };
            (
                vector(runtime.object_field(id, "origin")),
                vector(runtime.object_field(id, "angles")),
            )
        };
        match place_weapon(world, &name, origin, angles) {
            Ok(Some(number)) => {
                if let Some(entity) = world.resource_mut::<RoundScript>().entities.get_mut(&id) {
                    entity.kind = EntityKind::Item(number);
                }
            }
            Ok(None) => {}
            Err(message) => diag::warn!(Sim, "gsc: level weapon {name}: {message}"),
        }
    }
}

pub(crate) fn mark_ownerless(world: &mut World, missile: &Value) {
    let Value::Object(object) = *missile else {
        return;
    };
    let mut runtime = world.resource_mut::<RoundScript>();
    let id = runtime
        .missiles
        .iter()
        .find_map(|(id, o)| (*o == object).then_some(*id));
    if let Some(id) = id {
        runtime.campaign.ownerless_missiles.insert(id);
    }
}

pub(crate) fn projectile_impacts(world: &mut World, notes: &[super::WeaponNote]) {
    if !world.resource::<RoundScript>().campaign.active {
        return;
    }
    let mut runtime = world.resource_mut::<RoundScript>();
    let runtime = &mut *runtime;
    let missiles = &runtime.missiles;
    runtime
        .campaign
        .ownerless_missiles
        .retain(|id| missiles.contains_key(id));
    for note in notes {
        let super::WeaponNote::Detonated {
            id,
            owner,
            weapon,
            origin,
        } = *note
        else {
            continue;
        };
        if world
            .resource_mut::<RoundScript>()
            .campaign
            .ownerless_missiles
            .remove(&id)
        {
            continue;
        }
        let Value::Object(player) = super::super::players::player_object(world, owner.0) else {
            continue;
        };
        let frame = FrameWorld::from_world(world);
        let facts = frame.equipment_facts_for(weapon);
        let projectile = frame
            .combat_facts_for(weapon)
            .is_some_and(|facts| facts.weap_type == weapon_iw4::WEAPTYPE_PROJECTILE);
        if let Some(facts) = facts.filter(|facts| facts.explosion_inner_damage != 0) {
            let name = super::super::players::script_weapon(world, owner.0, weapon);
            let args = vec![
                super::weapon_name(world, name),
                Value::Vector(origin),
                Value::Float(facts.explosion_radius as f32),
            ];
            super::raise(world, Value::Object(player), "projectile_impact", args);
        }
        if projectile {
            super::super::actors::projectile_impact(world, player, origin);
        }
    }
}
