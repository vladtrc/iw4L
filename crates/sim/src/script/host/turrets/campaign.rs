use super::super::entities::EntityKind;
use super::Turret;
use crate::frame::FrameWorld;
use crate::script::{RoundScript, Value};
use bevy_ecs::prelude::World;

pub(crate) fn init_map_turrets(world: &mut World) {
    let turrets: Vec<(u64, String, [f32; 3])> = {
        let mut runtime = world.resource_mut::<RoundScript>();
        let ids: Vec<u64> = runtime
            .entities
            .iter()
            .filter(|(_, e)| e.kind == EntityKind::Map && &*e.classname == "misc_turret")
            .map(|(id, _)| *id)
            .collect();
        ids.into_iter()
            .filter_map(|id| {
                let Value::String(info) = runtime.object_field(id, "weaponinfo") else {
                    return None;
                };
                let origin = match runtime.object_field(id, "origin") {
                    Value::Vector(v) => v,
                    _ => [0.0; 3],
                };
                Some((id, info.to_string(), origin))
            })
            .collect()
    };
    for (id, info, origin) in turrets {
        let weapon = match crate::script_player::weapon_named(&FrameWorld::from_world(world), &info)
        {
            Ok(weapon) => weapon,
            Err(message) => {
                diag::warn!(Sim, "gsc: level turret: {message}");
                continue;
            }
        };
        let Ok(presence) = super::super::presence::spawn_presence(world, origin) else {
            continue;
        };
        let mut runtime = world.resource_mut::<RoundScript>();
        let entity = runtime.entities.get_mut(&id).unwrap();
        entity.kind = EntityKind::Spawned;
        entity.presence = Some(presence);
        runtime.new_models.push(id);
        runtime.engine.turrets.insert(
            id,
            Turret {
                weapon,
                mode: "manual".into(),
                owner: None,
                team: None,
                carried: false,
                fire_enabled: true,
                manual: None,
                target: None,
                firing: false,
                on_target: false,
                aim: [1.0, 0.0, 0.0],
                aim_initialized: false,
                scan_direction: 1.0,
                arcs: [180.0, 180.0, 90.0, 90.0],
                drop_pitch: 0.0,
                convergence: [0.0; 2],
                minimap: false,
                solid: false,
                minigun: false,
                mode_change_wait: 0.0,
            },
        );
    }
}

pub(super) fn fire_event(world: &mut World, object: u64, from: [f32; 3], weapon: u32) {
    let Some(presence) = world
        .resource::<RoundScript>()
        .entities
        .get(&object)
        .and_then(|e| e.presence)
    else {
        return;
    };
    let frame = super::super::presence::settled(world);
    let Some(number) = frame.gentity_number(presence) else {
        return;
    };
    let bone = frame
        .entity_collision_capabilities()
        .iter()
        .find(|row| row.owner.script_model() == Some(presence))
        .and_then(|row| row.dobj.as_ref()?.tag_bone("tag_flash"))
        .and_then(|bone| i32::try_from(bone).ok())
        .unwrap_or(-1);
    let tick = world.resource::<crate::step::StepRequest>().tick;
    FrameWorld::from_world(world).push_entity_event(
        tick,
        crate::EventAudience::All,
        entity_iw4::EntityEventKind::FIRE_WEAPON,
        crate::EntityEventPayload {
            number,
            weapon,
            event_parm: bone,
            origin: from,
            ..Default::default()
        },
    );
}

pub(crate) fn register(registry: &mut crate::script::NativeRegistry) {
    use super::super::natives::engine::entity_id;
    use crate::script::Namespace::Method;
    registry.register(Method, "useturret", use_turret);
    registry.register(Method, "restoredefaultdroppitch", |world, receiver, _| {
        let object = turret_object(world, receiver)?;
        if let Some(t) = world
            .resource_mut::<RoundScript>()
            .engine
            .turrets
            .get_mut(&object)
        {
            t.drop_pitch = 0.0;
        }
        Ok(Value::Undefined)
    });
    registry.register(Method, "setaispread", |world, receiver, args| {
        set_spread(world, receiver, args, AI_SPREAD)
    });
    registry.register(Method, "setplayerspread", |world, receiver, args| {
        set_spread(world, receiver, args, PLAYER_SPREAD)
    });
    registry.register(Method, "setsuppressiontime", |world, receiver, args| {
        let object = turret_object(world, receiver)?;
        let ms = (super::super::args::float(args, 0)? * 1000.0) as i32;
        world.resource_mut::<RoundScript>().set_object_field(
            object,
            SUPPRESSION_TIME,
            Value::Int(ms),
        );
        Ok(Value::Undefined)
    });
    registry.register(
        crate::script::Namespace::Function,
        "isturretactive",
        |world, _, args| {
            let object = turret_object(world, super::super::args::arg(args, 0)?)?;
            let runtime = world.resource::<RoundScript>();
            Ok(Value::Int(
                (runtime.campaign.turret_gunners.contains_key(&object)
                    || runtime.campaign.turret_users.contains_key(&object))
                .into(),
            ))
        },
    );
    registry.register(Method, "getmode", |world, receiver, _| {
        let object = turret_object(world, receiver)?;
        let runtime = world.resource::<RoundScript>();
        Ok(Value::string(&runtime.engine.turrets[&object].mode))
    });
    registry.register(Method, "startbarrelspin", |world, receiver, _| {
        let object = turret_object(world, receiver)?;
        set_barrel_spin(world, object, true);
        Ok(Value::Undefined)
    });
    registry.register(Method, "stopbarrelspin", |world, receiver, _| {
        let object = turret_object(world, receiver)?;
        set_barrel_spin(world, object, false);
        Ok(Value::Undefined)
    });
    registry.register(Method, "getbarrelspinrate", |world, receiver, _| {
        let object = turret_object(world, receiver)?;
        Ok(Value::Float(barrel_spin(world, object)))
    });
    registry.register(Method, "setturretanim", |world, receiver, args| {
        let object = turret_object(world, receiver)?;
        let anim = super::super::args::arg(args, 0)?.clone();
        world
            .resource_mut::<RoundScript>()
            .set_object_field(object, "turret_anim", anim);
        Ok(Value::Undefined)
    });
    registry.register(Method, "setturretcanaidetach", |world, receiver, args| {
        let object = turret_object(world, receiver)?;
        let can_detach =
            super::super::args::optional(args, 0, super::super::args::int)?.unwrap_or(1) != 0;
        world.resource_mut::<RoundScript>().set_object_field(
            object,
            "turret_ai_can_detach",
            Value::Int(can_detach.into()),
        );
        Ok(Value::Undefined)
    });
    registry.register(Method, "setturretignoregoals", |world, receiver, args| {
        let object = turret_object(world, receiver)?;
        let ignore =
            super::super::args::optional(args, 0, super::super::args::int)?.unwrap_or(1) != 0;
        world.resource_mut::<RoundScript>().set_object_field(
            object,
            "turret_ignore_goals",
            Value::Int(ignore.into()),
        );
        Ok(Value::Undefined)
    });
    registry.register(Method, "disableturretdismount", |world, receiver, _| {
        let object = turret_object(world, receiver)?;
        world.resource_mut::<RoundScript>().set_object_field(
            object,
            "turret_dismount_disabled",
            Value::Int(1),
        );
        Ok(Value::Undefined)
    });
    registry.register(Method, "startfiring", |world, receiver, _| {
        let object = entity_id(world, receiver)?;
        let mut runtime = world.resource_mut::<RoundScript>();
        if !runtime.engine.turrets.contains_key(&object) {
            return Err("startfiring: not a turret".into());
        }
        runtime.campaign.turret_firing.entry(object).or_insert(0);
        Ok(Value::Undefined)
    });
    registry.register(Method, "stopfiring", |world, receiver, _| {
        let object = entity_id(world, receiver)?;
        let mut runtime = world.resource_mut::<RoundScript>();
        runtime.campaign.turret_firing.remove(&object);
        Ok(Value::Undefined)
    });
    registry.register(Method, "stopuseturret", |world, receiver, _| {
        stop_use_turret(world, receiver)
    });
    registry.register(Method, "canuseturret", |world, receiver, args| {
        super::super::natives::engine::entity_id(world, receiver)?;
        Ok(Value::Int(
            super::turret_of(world, super::super::args::arg(args, 0)?)
                .is_ok()
                .into(),
        ))
    });
    registry.register(Method, "getturret", |world, receiver, _| {
        let gunner = super::super::natives::engine::entity_id(world, receiver)?;
        Ok(world
            .resource::<RoundScript>()
            .campaign
            .turret_gunners
            .iter()
            .find(|(_, (g, _))| *g == gunner)
            .map_or(Value::Undefined, |(t, _)| Value::Object(*t)))
    });
    registry.register(
        crate::script::Namespace::Method,
        "useby",
        |world, receiver, args| {
            let object = super::turret_of(world, receiver)?;
            let user = super::super::args::arg(args, 0)?.clone();
            let runtime = world.resource::<RoundScript>();
            let Some(client) = runtime.player_client_of(&user) else {
                return Ok(Value::Undefined);
            };
            if runtime
                .campaign
                .turret_users
                .get(&object)
                .is_some_and(|(c, _)| *c == client)
            {
                release(world, object, client);
                return Ok(Value::Undefined);
            }
            let player = runtime.players.get(&client).map(|slot| slot.object);
            let mut runtime = world.resource_mut::<RoundScript>();
            runtime.campaign.turret_users.insert(object, (client, 0));
            if let Some(turret) = runtime.engine.turrets.get_mut(&object) {
                turret.owner = player;
            }
            set_weapons_held(world, client, true);
            Ok(Value::Undefined)
        },
    );
    registry.register(Method, "setautorotationdelay", |world, receiver, args| {
        let object = turret_object(world, receiver)?;
        let delay = super::super::args::float(args, 0)?;
        world.resource_mut::<RoundScript>().set_object_field(
            object,
            "autorotationdelay",
            Value::Float(delay),
        );
        Ok(Value::Undefined)
    });
}

fn use_turret(world: &mut World, receiver: &Value, args: &[Value]) -> Result<Value, String> {
    let gunner = super::super::natives::engine::entity_id(world, receiver)?;
    let object = super::turret_of(world, super::super::args::arg(args, 0)?)?;
    let mut runtime = world.resource_mut::<RoundScript>();
    let team = match runtime.object_field(gunner, "team") {
        Value::String(team) => Some(std::sync::Arc::<str>::from(&*team)),
        _ => None,
    };
    if let Some(turret) = runtime.engine.turrets.get_mut(&object) {
        turret.owner = Some(gunner);
        turret.team = team;
        turret.mode = "auto_ai".into();
        turret.fire_enabled = true;
    }
    runtime.campaign.turret_gunners.insert(object, (gunner, 0));
    Ok(Value::Int(1))
}

fn stop_use_turret(world: &mut World, receiver: &Value) -> Result<Value, String> {
    let gunner = super::super::natives::engine::entity_id(world, receiver)?;
    let runtime = world.resource_mut::<RoundScript>();
    let manned: Vec<u64> = runtime
        .campaign
        .turret_gunners
        .iter()
        .filter(|(_, (g, _))| *g == gunner)
        .map(|(t, _)| *t)
        .collect();
    drop(runtime);
    for object in manned {
        unman(world, object);
    }
    Ok(Value::Undefined)
}

fn unman(world: &mut World, object: u64) {
    let mut runtime = world.resource_mut::<RoundScript>();
    runtime.campaign.turret_gunners.remove(&object);
    if let Some(turret) = runtime.engine.turrets.get_mut(&object) {
        turret.owner = None;
        turret.mode = "manual".into();
        turret.firing = false;
    }
    super::raise(
        world,
        Value::Object(object),
        "turret_deactivate",
        Vec::new(),
    );
}

pub(crate) fn gunner_script(world: &mut World, gunner: u64) -> Option<String> {
    let (weapon, scripts) = {
        let runtime = world.resource::<RoundScript>();
        let (turret, _) = runtime
            .campaign
            .turret_gunners
            .iter()
            .find(|(_, (g, _))| *g == gunner)?;
        (
            runtime.engine.turrets.get(turret)?.weapon,
            runtime.campaign.turret_ai_scripts.clone(),
        )
    };
    let name = crate::script_player::weapon_name(&FrameWorld::from_world(world), weapon);
    scripts.get(&name).cloned()
}

pub(super) fn gunner_turrets(world: &mut World) {
    let gunners: Vec<(u64, u64, i64)> = world
        .resource::<RoundScript>()
        .campaign
        .turret_gunners
        .iter()
        .map(|(turret, (gunner, next))| (*turret, *gunner, *next))
        .collect();
    let now = super::super::players::now_ms(world);
    for (object, gunner, next) in gunners {
        let state = {
            let mut runtime = world.resource_mut::<RoundScript>();
            let alive = runtime.live(&gunner)
                && !matches!(runtime.object_field(gunner, "health"), Value::Int(h) if h <= 0);
            match runtime.engine.turrets.get(&object) {
                Some(turret) if alive => Some((
                    turret.on_target && turret.fire_enabled,
                    turret.weapon,
                    turret.aim,
                )),
                _ => None,
            }
        };
        let Some((firing, weapon, aim)) = state else {
            unman(world, object);
            continue;
        };
        if !firing || now < next || gunner_script(world, gunner).is_some() {
            continue;
        }
        let interval = FrameWorld::from_world(world)
            .combat_facts_for(weapon)
            .map_or(100, |facts| facts.fire_time_ms.max(1));
        if let Some(entry) = world
            .resource_mut::<RoundScript>()
            .campaign
            .turret_gunners
            .get_mut(&object)
        {
            entry.1 = now + i64::from(interval);
        }
        let from = super::muzzle(world, object);
        let aim = spread(world, object, AI_SPREAD, from, aim);
        super::fire_bullet(world, object, from, aim, weapon, None);
    }
}

const SUPPRESSION_TIME: &str = "suppressiontime";
const SUPPRESSION_STAY: f32 = 64.0;
const SUPPRESSION_SIDE: f32 = 30.0;

pub(super) fn sentient_targets(world: &mut World, turret: &Turret) -> Vec<(u64, glam::Vec3)> {
    if turret.team.is_none() && turret.owner.is_none() {
        return Vec::new();
    }
    let players: Vec<u64> = world
        .resource::<RoundScript>()
        .players
        .values()
        .map(|slot| slot.object)
        .collect();
    super::super::actors::combatants(world)
        .into_iter()
        .filter(|(id, _)| !players.contains(id))
        .filter_map(
            |(id, _)| match super::super::players::entity_field(world, id, "origin") {
                Value::Vector(origin) => Some((
                    id,
                    glam::Vec3::from_array(origin) + glam::Vec3::Z * super::TARGET_HEIGHT,
                )),
                _ => None,
            },
        )
        .collect()
}

pub(super) fn suppression(
    world: &mut World,
    object: u64,
    from: [f32; 3],
    found: Option<(u64, glam::Vec3)>,
) -> Option<(u64, glam::Vec3)> {
    let now = super::super::players::now_ms(world);
    let mut runtime = world.resource_mut::<RoundScript>();
    if let Some((target, at)) = found {
        runtime
            .campaign
            .turret_seen
            .insert(object, (target, at.to_array(), now));
        return found;
    }
    let Value::Int(window) = runtime.object_field(object, SUPPRESSION_TIME) else {
        return None;
    };
    let (target, at, seen) = runtime.campaign.turret_seen.get(&object).copied()?;
    if now - seen > i64::from(window) || !runtime.live(&target) {
        runtime.campaign.turret_seen.remove(&object);
        return None;
    }
    drop(runtime);
    let at = glam::Vec3::from_array(at);
    let Value::Vector(origin) = super::super::players::entity_field(world, target, "origin") else {
        return None;
    };
    let now_at = glam::Vec3::from_array(origin) + glam::Vec3::Z * super::TARGET_HEIGHT;
    if now_at.distance(at) > SUPPRESSION_STAY {
        return None;
    }
    let side = (at - glam::Vec3::from_array(from))
        .cross(glam::Vec3::Z)
        .normalize_or_zero();
    Some((target, at + side * SUPPRESSION_SIDE))
}

pub(super) const AI_SPREAD: &str = "aispread";
const PLAYER_SPREAD: &str = "playerspread";

fn turret_object(world: &mut World, receiver: &Value) -> Result<u64, String> {
    let object = super::super::natives::engine::entity_id(world, receiver)?;
    if !world
        .resource::<RoundScript>()
        .engine
        .turrets
        .contains_key(&object)
    {
        return Err("entity is not a turret".into());
    }
    Ok(object)
}

fn set_spread(
    world: &mut World,
    receiver: &Value,
    args: &[Value],
    field: &str,
) -> Result<Value, String> {
    let object = turret_object(world, receiver)?;
    let spread = super::super::args::float(args, 0)?;
    world
        .resource_mut::<RoundScript>()
        .set_object_field(object, field, Value::Float(spread));
    Ok(Value::Undefined)
}

const BARREL_SPIN_MS: f32 = 1_000.0;

fn barrel_spin(world: &mut World, object: u64) -> f32 {
    let now = super::super::players::now_ms(world);
    let mut runtime = world.resource_mut::<RoundScript>();
    let (Value::Float(rate), Value::Int(at)) = (
        runtime.object_field(object, "barrelspinrate"),
        runtime.object_field(object, "barrelspintime"),
    ) else {
        return 0.0;
    };
    let spinning = runtime.object_field(object, "barrelspinning") == Value::Int(1);
    spin_rate(rate, (now - i64::from(at)) as f32, spinning)
}

fn spin_rate(rate: f32, elapsed_ms: f32, spinning: bool) -> f32 {
    let step = elapsed_ms / BARREL_SPIN_MS;
    (if spinning { rate + step } else { rate - step }).clamp(0.0, 1.0)
}

fn set_barrel_spin(world: &mut World, object: u64, spinning: bool) {
    let rate = barrel_spin(world, object);
    let now = super::super::players::now_ms(world);
    let mut runtime = world.resource_mut::<RoundScript>();
    runtime.set_object_field(object, "barrelspinrate", Value::Float(rate));
    runtime.set_object_field(object, "barrelspintime", Value::Int(now as i32));
    runtime.set_object_field(object, "barrelspinning", Value::Int(spinning.into()));
}

pub(super) fn spread(
    world: &mut World,
    object: u64,
    field: &str,
    from: [f32; 3],
    aim: [f32; 3],
) -> [f32; 3] {
    let spread = match world
        .resource_mut::<RoundScript>()
        .object_field(object, field)
    {
        Value::Float(spread) => spread,
        _ => return aim,
    };
    let draws = (
        super::super::natives::math::random_unit(world),
        super::super::natives::math::random_unit(world),
    );
    let end = super::super::natives::math::spread_end(
        from,
        std::array::from_fn(|k| from[k] + aim[k]),
        spread,
        draws,
    );
    (glam::Vec3::from(end) - glam::Vec3::from(from))
        .normalize_or(glam::Vec3::from(aim))
        .to_array()
}

pub(super) fn scripted_fire(world: &mut World) {
    let now = super::super::players::now_ms(world);
    let firing: Vec<(u64, i64)> = world
        .resource::<RoundScript>()
        .campaign
        .turret_firing
        .iter()
        .map(|(turret, next)| (*turret, *next))
        .collect();
    for (object, next) in firing {
        let turret = world
            .resource::<RoundScript>()
            .engine
            .turrets
            .get(&object)
            .map(|t| (t.weapon, t.aim, t.fire_enabled));
        let Some((weapon, aim, enabled)) = turret else {
            world
                .resource_mut::<RoundScript>()
                .campaign
                .turret_firing
                .remove(&object);
            continue;
        };
        if !enabled || now < next {
            continue;
        }
        let interval = FrameWorld::from_world(world)
            .combat_facts_for(weapon)
            .map_or(100, |facts| facts.fire_time_ms.max(1));
        world
            .resource_mut::<RoundScript>()
            .campaign
            .turret_firing
            .insert(object, now + i64::from(interval));
        let from = super::muzzle(world, object);
        super::fire_bullet(world, object, from, aim, weapon, None);
    }
}

fn set_weapons_held(world: &mut World, client: u32, held: bool) {
    let mut frame = FrameWorld::from_world(world);
    let id = crate::world::ClientId(client);
    if frame.client_meta(id).is_some() {
        frame.client_meta_mut(id).controls.weapons_disabled = held;
    }
}

fn release(world: &mut World, object: u64, client: u32) {
    let mut runtime = world.resource_mut::<RoundScript>();
    runtime.campaign.turret_users.remove(&object);
    if let Some(turret) = runtime.engine.turrets.get_mut(&object) {
        turret.firing = false;
    }
    set_weapons_held(world, client, false);
    super::raise(
        world,
        Value::Object(object),
        "turret_deactivate",
        Vec::new(),
    );
}

pub(super) fn player_turrets(world: &mut World) {
    let users: Vec<(u64, u32, i64)> = world
        .resource::<RoundScript>()
        .campaign
        .turret_users
        .iter()
        .map(|(turret, (client, next))| (*turret, *client, *next))
        .collect();
    let now = super::super::players::now_ms(world);
    for (object, client, next) in users {
        let id = crate::world::ClientId(client);
        let live = {
            let runtime = world.resource::<RoundScript>();
            runtime.engine.turrets.contains_key(&object) && runtime.entities.contains_key(&object)
        };
        let (view, held) = {
            let mut frame = FrameWorld::from_world(world);
            let view = frame.player(id).map(|ps| ps.viewangles);
            (view, crate::script_player::buttons(&mut frame, id))
        };
        let Some(view) = view.filter(|_| live) else {
            release(world, object, client);
            continue;
        };
        let aim = math_iw4::angles_to_axis(view)[0];
        let firing = held & playerstate_iw4::buttons::ATTACK != 0;
        let weapon = {
            let mut runtime = world.resource_mut::<RoundScript>();
            let turret = runtime.engine.turrets.get_mut(&object).unwrap();
            turret.aim = aim;
            turret.aim_initialized = true;
            turret.firing = firing;
            turret.weapon
        };
        if !firing || now < next {
            continue;
        }
        let interval = FrameWorld::from_world(world)
            .combat_facts_for(weapon)
            .map_or(100, |facts| facts.fire_time_ms.max(1));
        if let Some(user) = world
            .resource_mut::<RoundScript>()
            .campaign
            .turret_users
            .get_mut(&object)
        {
            user.1 = now + i64::from(interval);
        }
        let from = super::muzzle(world, object);
        let aim = spread(world, object, PLAYER_SPREAD, from, aim);
        super::fire_bullet(world, object, from, aim, weapon, Some(id));
    }
}
