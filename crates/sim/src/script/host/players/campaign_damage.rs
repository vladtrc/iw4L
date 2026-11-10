use super::*;
use crate::script::{MatchScript, Namespace, NativeRegistry};

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct PlayerDamage {
    pub multiplier: f32,
    pub critical_bullet_dist: f32,
    pub invulnerable_time: i32,
    pub attacker_count: u8,
    health_shield: bool,
    death_shield: bool,
    activated: bool,
    expires_ms: i64,
    blood: i32,
    from_world: bool,
    from: [f32; 3],
}

impl Default for PlayerDamage {
    fn default() -> Self {
        Self {
            multiplier: 1.0,
            critical_bullet_dist: 0.0,
            invulnerable_time: 0,
            attacker_count: 0,
            health_shield: true,
            death_shield: false,
            activated: false,
            expires_ms: 0,
            blood: 0,
            from_world: false,
            from: [0.0; 3],
        }
    }
}

const FIELDS: &[&str] = &[
    "damagemultiplier",
    "criticalbulletdamagedist",
    "deathinvulnerabletime",
    "attackercount",
];

pub(super) fn load_field(world: &World, client: u32, name: &str) -> Option<Value> {
    let runtime = world.resource::<RoundScript>();
    if runtime.profile != asset_core::RuntimeProfile::Campaign {
        return None;
    }
    let state = &runtime.players.get(&client)?.campaign_damage;
    Some(match name {
        "damagemultiplier" => Value::Float(state.multiplier),
        "criticalbulletdamagedist" => Value::Float(state.critical_bullet_dist),
        "deathinvulnerabletime" => Value::Int(state.invulnerable_time),
        "attackercount" => Value::Int(i32::from(state.attacker_count)),
        _ => return None,
    })
}

pub(super) fn store_field(
    world: &mut World,
    client: u32,
    name: &str,
    value: &Value,
) -> Result<bool, String> {
    if world.resource::<RoundScript>().profile != asset_core::RuntimeProfile::Campaign
        || !FIELDS.contains(&name)
    {
        return Ok(false);
    }
    let number = super::super::args::float(std::slice::from_ref(value), 0)?;
    let integer = super::super::args::int(std::slice::from_ref(value), 0)?;
    let mut runtime = world.resource_mut::<RoundScript>();
    let state = &mut runtime
        .players
        .get_mut(&client)
        .ok_or("player has disconnected")?
        .campaign_damage;
    match name {
        "damagemultiplier" => state.multiplier = number,
        "criticalbulletdamagedist" => state.critical_bullet_dist = number,
        "deathinvulnerabletime" => state.invulnerable_time = integer,
        _ => state.attacker_count = integer as u8,
    }
    Ok(true)
}

pub(crate) fn register(registry: &mut NativeRegistry) {
    registry.register(Namespace::Method, "painvisionon", |world, receiver, _| {
        let client = super::super::natives::player::player(world, receiver)?;
        if let Some(ps) = FrameWorld::from_world(world).player_mut(ClientId(client)) {
            ps.other_flags |= playerstate_iw4::other_flags::PAIN_VISION;
        }
        Ok(Value::Undefined)
    });
    registry.register(Namespace::Method, "painvisionoff", |world, receiver, _| {
        let client = super::super::natives::player::player(world, receiver)?;
        if let Some(ps) = FrameWorld::from_world(world).player_mut(ClientId(client)) {
            ps.other_flags &= !playerstate_iw4::other_flags::PAIN_VISION;
        }
        Ok(Value::Undefined)
    });
    registry.register(
        Namespace::Method,
        "enablehealthshield",
        |world, receiver, args| {
            let client = super::super::natives::player::player(world, receiver)?;
            let on = super::super::args::int(args, 0)? != 0;
            world
                .resource_mut::<RoundScript>()
                .players
                .get_mut(&client)
                .unwrap()
                .campaign_damage
                .health_shield = on;
            Ok(Value::Undefined)
        },
    );
    registry.register(
        Namespace::Method,
        "enabledeathshield",
        |world, receiver, args| {
            let client = super::super::natives::player::player(world, receiver)?;
            let on = super::super::args::int(args, 0)? != 0;
            world
                .resource_mut::<RoundScript>()
                .players
                .get_mut(&client)
                .unwrap()
                .campaign_damage
                .death_shield = on;
            Ok(Value::Undefined)
        },
    );
    registry.register(
        Namespace::Method,
        "enableinvulnerability",
        |world, receiver, _| {
            let client = super::super::natives::player::player(world, receiver)?;
            if let Some(ps) = FrameWorld::from_world(world).player_mut(ClientId(client)) {
                ps.other_flags |= 1;
            }
            Ok(Value::Undefined)
        },
    );
    registry.register(
        Namespace::Method,
        "disableinvulnerability",
        |world, receiver, _| {
            let client = super::super::natives::player::player(world, receiver)?;
            if let Some(ps) = FrameWorld::from_world(world).player_mut(ClientId(client)) {
                ps.other_flags &= !1;
            }
            Ok(Value::Undefined)
        },
    );
}

fn dvar(world: &World, name: &str, default: f32) -> f32 {
    world
        .resource::<MatchScript>()
        .dvars
        .get(name)
        .and_then(|value| value.parse().ok())
        .unwrap_or(default)
}

fn bullet(means: &str) -> bool {
    matches!(
        means,
        "MOD_PISTOL_BULLET" | "MOD_RIFLE_BULLET" | "MOD_EXPLOSIVE" | "MOD_HEAD_SHOT"
    )
}

pub(crate) fn apply(
    world: &mut World,
    tick: crate::Tick,
    hit: &crate::script_player::Hit,
    attacker_override: Option<Value>,
) {
    let Value::Object(object) = player_object(world, hit.victim.0) else {
        return;
    };
    let runtime = world.resource::<RoundScript>();
    if !runtime.entities[&object].accepts_damage(hit.flags)
        || hit.hitloc == crate::shield::HITLOC
        || (runtime.engine.players_ignore_radius_damage
            && hit.flags & crate::script_player::IDFLAGS_RADIUS != 0)
    {
        return;
    }
    let Some(ps) = FrameWorld::from_world(world).player(hit.victim).copied() else {
        return;
    };
    if ps.other_flags & 1 != 0 || ps.pm_type >= playerstate_iw4::PM_TYPE_DEAD {
        return;
    }
    let attacker = attacker_override.unwrap_or_else(|| {
        hit.attacker
            .map(|id| player_object(world, id.0))
            .filter(|v| *v != Value::Undefined)
            .unwrap_or_else(|| world_entity(world))
    });
    let mut state = world.resource::<RoundScript>().players[&hit.victim.0]
        .campaign_damage
        .clone();
    let mut amount = hit.amount;
    if hit.means != "MOD_SUICIDE" {
        amount = if hit.flags & crate::script_player::IDFLAGS_RADIUS != 0 {
            (amount as f32 * dvar(world, "player_radiusdamagemultiplier", 1.0)) as i32
        } else if hit.means == "MOD_MELEE" {
            (amount as f32 * dvar(world, "player_meleedamagemultiplier", 1.0)) as i32
        } else {
            (amount as f32 * dvar(world, "player_damagemultiplier", 1.0) * state.multiplier) as i32
        };
    }
    if let Value::Object(attacker) = attacker
        && world
            .resource::<RoundScript>()
            .player_client(attacker)
            .is_some()
    {
        amount = (amount as f32 * dvar(world, "g_friendlyfiredamagescale", 1.0)) as i32;
    }
    amount = amount.max(1);
    let mut direction = hit.dir;
    let length = direction.iter().map(|n| n * n).sum::<f32>().sqrt();
    if length > 0.0 {
        direction = direction.map(|n| n / length);
    }
    let knockback = ((amount as f32
        * if ps.pm_flags & 1 != 0 {
            0.01
        } else if ps.pm_flags & 2 != 0 {
            0.025
        } else {
            0.05
        }) as i32)
        .min(60);
    if hit.direction_present && hit.flags & 4 == 0 && ps.e_flags & 0xc00 == 0 && knockback != 0 {
        let scale = (knockback as f32 * dvar(world, "g_knockback", 1000.0)) / 250.0;
        if let Some(ps) = FrameWorld::from_world(world).player_mut(hit.victim) {
            for (velocity, dir) in ps.velocity.iter_mut().zip(direction) {
                *velocity += scale * dir;
            }
            if attacker == Value::Object(object)
                && matches!(
                    hit.means,
                    "MOD_GRENADE"
                        | "MOD_GRENADE_SPLASH"
                        | "MOD_PROJECTILE"
                        | "MOD_PROJECTILE_SPLASH"
                )
            {
                ps.velocity[2] *= 0.25;
            }
            if ps.pm_time == 0 {
                ps.pm_time = (knockback * 2).clamp(50, 200);
                ps.pm_flags |= 0x100;
            }
        }
    }
    if crate::script_player::god_mode(&FrameWorld::from_world(world), hit.victim) {
        return;
    }
    state.blood = state.blood.wrapping_add(amount);
    state.from_world = !hit.direction_present;
    state.from = if hit.direction_present {
        direction
    } else {
        ps.origin
    };
    let now = i64::from(tick.0) * i64::from(crate::MATCH_TICK_MS);
    let old_health = ps.health;
    let mut health = old_health.wrapping_sub(amount);
    if state.health_shield {
        if old_health == ps.max_health {
            state.activated = false;
        }
        if old_health != health {
            let bullet = bullet(hit.means);
            if bullet
                && state.critical_bullet_dist != 0.0
                && !(state.activated && now < state.expires_ms)
            {
                let from = entity_field(
                    world,
                    match &attacker {
                        Value::Object(id) => *id,
                        _ => object,
                    },
                    "origin",
                );
                if let Value::Vector(from) = from {
                    let distance_sq = (from[1] - ps.origin[1]).powi(2)
                        + (from[0] - ps.origin[0]).powi(2)
                        + (from[2] - ps.origin[2]).powi(2);
                    if distance_sq < state.critical_bullet_dist * state.critical_bullet_dist {
                        health = 0;
                    }
                }
            }
            let eligible = bullet
                || (matches!(hit.means, "MOD_PROJECTILE" | "MOD_PROJECTILE_SPLASH")
                    && dvar(world, "player_deathinvulnerabletoprojectile", 1.0) != 0.0)
                || (hit.means == "MOD_MELEE"
                    && dvar(world, "player_deathinvulnerabletomelee", 0.0) != 0.0);
            if health <= 0 && old_health > 0 && eligible {
                if !state.activated {
                    state.activated = true;
                    state.expires_ms = now + i64::from(state.invulnerable_time);
                }
                if now < state.expires_ms {
                    health = 1;
                }
            }
        }
    }
    let death_shield = state.death_shield && health != old_health && health <= 0 && old_health > 0;
    if death_shield {
        health = 1;
    }
    world
        .resource_mut::<RoundScript>()
        .players
        .get_mut(&hit.victim.0)
        .unwrap()
        .campaign_damage = state;
    let flinch_time = (amount as f32 * dvar(world, "player_dmgtimer_timeperpoint", 100.0)) as i32;
    let flinch_max = dvar(world, "player_dmgtimer_maxtime", 750.0);
    let flinch_time = ps.damage_timer.wrapping_add(flinch_time);
    let flinch_time = if flinch_max < flinch_time as f32 {
        flinch_max as i32
    } else {
        flinch_time
    };
    if let Some(ps) = FrameWorld::from_world(world).player_mut(hit.victim) {
        ps.health = health;
        movement_iw4::update_damage_timer(ps, amount, hit.direction_present.then_some(hit.dir));
        ps.damage_timer = flinch_time;
        ps.damage_duration = flinch_time;
    }
    let weapon = if hit.weapon == 0 {
        Value::Undefined
    } else {
        Value::string(&crate::script_player::weapon_name(
            &FrameWorld::from_world(world),
            hit.weapon,
        ))
    };
    let args = vec![
        Value::Int(amount),
        attacker.clone(),
        Value::Vector(if hit.direction_present {
            hit.dir
        } else {
            [0.0; 3]
        }),
        Value::Vector(hit.point),
        Value::string(hit.means),
        Value::string(""),
        Value::string(""),
        Value::string(""),
        Value::Int(hit.flags),
        weapon.clone(),
    ];
    if death_shield {
        raise(world, Value::Object(object), "deathshield", args.clone());
        crate::script::runtime::deliver_external(world, now);
    }
    raise(world, Value::Object(object), "damage", args);
    crate::script::runtime::deliver_external(world, now);
    if FrameWorld::from_world(world)
        .player(hit.victim)
        .is_some_and(|ps| ps.health <= 0)
    {
        crate::script_player::kill(
            &mut FrameWorld::from_world(world),
            tick,
            hit.victim,
            hit.attacker,
            hit.commit,
        );
        raise(
            world,
            Value::Object(object),
            "death",
            vec![attacker, Value::string(hit.means), weapon],
        );
        crate::script::runtime::deliver_external(world, now);
    }
}

pub(crate) fn feedback(world: &mut World) {
    if world.resource::<RoundScript>().profile != asset_core::RuntimeProfile::Campaign {
        return;
    }
    let clients: Vec<_> = world
        .resource::<RoundScript>()
        .players
        .keys()
        .copied()
        .collect();
    for client in clients {
        let id = ClientId(client);
        let Some(ps) = FrameWorld::from_world(world).player(id).copied() else {
            continue;
        };
        if ps.pm_type >= playerstate_iw4::PM_TYPE_DEAD {
            continue;
        }
        let state = &world.resource::<RoundScript>().players[&client].campaign_damage;
        let count = if ps.max_health > 0 {
            (state.blood.wrapping_mul(100) / ps.max_health).min(127)
        } else {
            0
        };
        let feedback =
            (state.blood > 0 && ps.max_health > 0).then_some((count, state.from_world, state.from));
        if let Some(ps) = FrameWorld::from_world(world).player_mut(id) {
            ps.damage_count = 0;
            if let Some((count, from_world, direction)) = feedback {
                ps.aim_spread_scale = (ps.aim_spread_scale + count as f32).min(255.0);
                ps.aim_spread_scale = (ps.aim_spread_scale + count as f32).min(255.0);
                if from_world {
                    ps.damage_pitch = 255;
                    ps.damage_yaw = 255;
                } else {
                    let angles = math_iw4::vect_to_angles(direction);
                    ps.damage_pitch = (angles[0] / 360.0 * 256.0) as u32 & 255;
                    ps.damage_yaw = (angles[1] / 360.0 * 256.0) as u32 & 255;
                }
                ps.damage_event = ps.damage_event.wrapping_add(1);
                ps.damage_count = count;
            }
        }
        if feedback.is_some() {
            let mut runtime = world.resource_mut::<RoundScript>();
            let state = &mut runtime.players.get_mut(&client).unwrap().campaign_damage;
            state.blood = 0;
            state.from_world = false;
        }
    }
}
