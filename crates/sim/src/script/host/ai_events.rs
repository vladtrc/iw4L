use super::{args::arg, natives::engine::entity_id};
use crate::frame::FrameWorld;
use crate::script::{MatchScript, Namespace, NativeRegistry, RoundScript, Value};
use bevy_ecs::prelude::World;

#[derive(Clone, Copy, Debug)]
pub(crate) enum Event {
    NewEnemy = 5,
    Death = 7,
    Explode = 8,
    GrenadeDanger = 9,
    ProjectileDanger = 10,
    Gunshot = 11,
    GunshotTeammate = 12,
    SilencedShot = 13,
    BulletWhizby = 16,
    ProjectileImpact = 17,
}
impl Event {
    fn parse(value: &Value) -> Result<Self, String> {
        let Value::String(name) = value else {
            return Err("AI event name must be a string".into());
        };
        match &**name {
            "new_enemy" => Ok(Self::NewEnemy),
            "death" => Ok(Self::Death),
            "explode" => Ok(Self::Explode),
            "grenade danger" => Ok(Self::GrenadeDanger),
            "gunshot" => Ok(Self::Gunshot),
            "gunshot_teammate" => Ok(Self::GunshotTeammate),
            "silenced_shot" => Ok(Self::SilencedShot),
            "bulletwhizby" => Ok(Self::BulletWhizby),
            "projectile_impact" => Ok(Self::ProjectileImpact),
            _ => Err(format!("unable to find AI event for '{name}'")),
        }
    }
    fn name(self) -> &'static str {
        match self {
            Self::NewEnemy => "new_enemy",
            Self::Death => "death",
            Self::Explode => "explode",
            Self::GrenadeDanger | Self::ProjectileDanger => "grenade danger",
            Self::Gunshot => "gunshot",
            Self::GunshotTeammate => "gunshot_teammate",
            Self::SilencedShot => "silenced_shot",
            Self::BulletWhizby => "bulletwhizby",
            Self::ProjectileImpact => "projectile_impact",
        }
    }
    fn distance(self) -> (&'static str, f32, f32) {
        match self {
            Self::NewEnemy => ("newenemy", 1024., 256.),
            Self::Death => ("death", 1024., 256.),
            Self::Explode => ("explosion", 1024., 1024.),
            Self::GrenadeDanger => ("grenadeping", 512., 256.),
            Self::ProjectileDanger => ("projping", 128., 128.),
            Self::Gunshot => ("gunshot", 2048., 400.),
            Self::GunshotTeammate => ("gunshotteam", 2048., 256.),
            Self::SilencedShot => ("silencedshot", 128., 128.),
            Self::BulletWhizby => ("bullet", 128., 128.),
            Self::ProjectileImpact => ("projimpact", 256., 256.),
        }
    }
}

#[derive(Clone, Debug, Default)]
pub(crate) struct Listeners {
    entries: Vec<(u64, u32)>,
    grenade_ping_times: std::collections::BTreeMap<u64, i32>,
    pub deliveries: u64,
}
impl Listeners {
    fn add(&mut self, owner: u64, event: Event) -> Result<(), String> {
        let bit = 1 << event as u32;
        if let Some((_, mask)) = self.entries.iter_mut().find(|(id, _)| *id == owner) {
            *mask |= bit;
        } else {
            if self.entries.len() == 32 {
                return Err("maximum AI event listeners exceeded".into());
            }
            self.entries.push((owner, bit));
        }
        Ok(())
    }
    fn remove(&mut self, owner: u64, event: Option<Event>) {
        if let Some(index) = self.entries.iter().position(|(id, _)| *id == owner) {
            if let Some(event) = event {
                self.entries[index].1 &= !(1 << event as u32);
            } else {
                self.entries[index].1 = 0;
            }
            if self.entries[index].1 == 0 {
                self.entries.swap_remove(index);
            }
        }
    }
    pub(super) fn remove_entity(&mut self, owner: u64) {
        self.remove(owner, None);
        self.grenade_ping_times.remove(&owner);
    }
}

pub(super) fn register(registry: &mut NativeRegistry) {
    registry.register(
        Namespace::Method,
        "addaieventlistener",
        |world, receiver, args| {
            let owner = entity_id(world, receiver)?;
            let event = Event::parse(arg(args, 0)?)?;
            world
                .resource_mut::<RoundScript>()
                .ai_event_listeners
                .add(owner, event)?;
            Ok(Value::Undefined)
        },
    );
    registry.register(
        Namespace::Method,
        "removeaieventlistener",
        |world, receiver, args| {
            let owner = entity_id(world, receiver)?;
            let event = args.first().map(Event::parse).transpose()?;
            world
                .resource_mut::<RoundScript>()
                .ai_event_listeners
                .remove(owner, event);
            Ok(Value::Undefined)
        },
    );
}

fn recipients(world: &World, event: Event, source: Option<u64>, teams: u32) -> Vec<u64> {
    let Some(runtime) = world.get_resource::<RoundScript>() else {
        return vec![];
    };
    if runtime.profile != asset_core::RuntimeProfile::Campaign || !runtime.started {
        return vec![];
    }
    runtime
        .ai_event_listeners
        .entries
        .iter()
        .filter_map(|&(owner, mask)| {
            (mask & (1 << event as u32) != 0
                && runtime.live(&owner)
                && (Some(owner) != source || matches!(event, Event::Death))
                && runtime
                    .sentients
                    .get(&owner)
                    .is_none_or(|s| teams & (1 << s.team) != 0))
            .then_some(owner)
        })
        .collect()
}

fn radius(world: &mut World, event: Event, owner: u64, source: Option<u64>) -> f32 {
    let busy = {
        let mut runtime = world.resource_mut::<RoundScript>();
        runtime
            .actors
            .get(&owner)
            .is_some_and(|a| a.sight.has_threatening_enemy)
            && runtime.object_field(owner, "enemy")
                != source.map_or(Value::Undefined, Value::Object)
    };
    let (suffix, normal, occupied) = event.distance();
    let key = format!("ai_{}eventdist{suffix}", if busy { "busy" } else { "" });
    world
        .resource::<MatchScript>()
        .dvars
        .get(&key)
        .and_then(|v| v.parse().ok())
        .unwrap_or(if busy { occupied } else { normal })
}

fn origin(world: &mut World, owner: u64) -> Option<[f32; 3]> {
    match super::players::entity_field(world, owner, "origin") {
        Value::Vector(at) => Some(at),
        _ => None,
    }
}

fn notify(world: &mut World, event: Event, owner: u64, source: Option<u64>, at: [f32; 3]) {
    let mut runtime = world.resource_mut::<RoundScript>();
    runtime.ai_event_listeners.deliveries += 1;
    runtime.pending_notifies.push((
        Value::Object(owner),
        "ai_event".into(),
        vec![
            Value::string(event.name()),
            source.map_or(Value::Undefined, Value::Object),
            Value::Vector(at),
        ],
    ));
}

pub(crate) fn point(
    world: &mut World,
    event: Event,
    source: Option<u64>,
    at: [f32; 3],
    teams: u32,
) {
    for owner in recipients(world, event, source, teams) {
        let Some(origin) = origin(world, owner) else {
            continue;
        };
        let radius = radius(world, event, owner, source);
        if (0..3).map(|i| (origin[i] - at[i]).powi(2)).sum::<f32>() <= radius * radius {
            notify(world, event, owner, source, at);
        }
    }
}

fn line(world: &mut World, event: Event, source: u64, start: [f32; 3], end: [f32; 3], teams: u32) {
    let delta: [f32; 3] = std::array::from_fn(|i| end[i] - start[i]);
    let length_sq = delta.iter().map(|v| v * v).sum::<f32>();
    for owner in recipients(world, event, Some(source), teams) {
        let Some(origin) = origin(world, owner) else {
            continue;
        };
        let projection = (0..3)
            .map(|i| (origin[i] - start[i]) * delta[i])
            .sum::<f32>();
        if projection < 0. {
            continue;
        }
        let fraction = if length_sq > 0. {
            (projection / length_sq).min(1.)
        } else {
            1.
        };
        let at: [f32; 3] = std::array::from_fn(|i| start[i] + fraction * delta[i]);
        let (number, player) = {
            let runtime = world.resource::<RoundScript>();
            (
                runtime.entities[&owner].number,
                runtime.player_client(owner),
            )
        };
        let midpoint = if let Some(client) = player {
            FrameWorld::from_world(world)
                .player(crate::ClientId(client))
                .map_or([0.; 3], |ps| {
                    [
                        0.,
                        0.,
                        if ps.pm_flags & 1 != 0 {
                            15.
                        } else if ps.pm_flags & 2 != 0 {
                            25.
                        } else {
                            35.
                        },
                    ]
                })
        } else {
            FrameWorld::from_world(world)
                .script_mover_by_number(number)
                .map_or([0.; 3], |m| m.box_mid)
        };
        let center: [f32; 3] = std::array::from_fn(|i| origin[i] + midpoint[i]);
        let radius = radius(world, event, owner, Some(source));
        if (center[0] - at[0]).powi(2) + (center[1] - at[1]).powi(2) <= radius * radius
            && (center[2] - at[2]).abs() < 45.
        {
            notify(world, event, owner, Some(source), at);
        }
    }
}

pub(crate) fn bullet(
    world: &mut World,
    shooter: u64,
    start: [f32; 3],
    end: [f32; 3],
    silenced: bool,
    initial: bool,
) {
    let Some(team) = world
        .get_resource::<RoundScript>()
        .and_then(|r| r.sentients.get(&shooter))
        .map(|s| s.team)
    else {
        return;
    };
    let enemies = 0x1e & !(1 << team);
    if initial {
        point(
            world,
            if silenced {
                Event::SilencedShot
            } else {
                Event::Gunshot
            },
            Some(shooter),
            start,
            enemies,
        );
    }
    let direction: [f32; 3] = std::array::from_fn(|i| start[i] - end[i]);
    let length = direction.iter().map(|v| v * v).sum::<f32>().sqrt();
    let extended = if length > 0. {
        std::array::from_fn(|i| start[i] + direction[i] * (200. / length))
    } else {
        start
    };
    let friendly = world
        .resource::<MatchScript>()
        .dvars
        .get("ai_friendlysuppression")
        .is_none_or(|v| v != "0");
    line(
        world,
        Event::BulletWhizby,
        shooter,
        extended,
        end,
        if friendly { 0x1e } else { enemies },
    );
}

pub(super) fn missile(
    world: &mut World,
    object: u64,
    projectile: &crate::ProjectileState,
    now: i32,
    grenade: bool,
) {
    let due = if grenade {
        let mut runtime = world.resource_mut::<RoundScript>();
        let last = runtime
            .ai_event_listeners
            .grenade_ping_times
            .entry(object)
            .or_insert(projectile.spawn_time_ms);
        if now - *last >= 250 {
            *last = now;
            true
        } else {
            false
        }
    } else {
        true
    };
    if due {
        if grenade {
            super::actors::grenades::observe(world, projectile, now);
        }
        point(
            world,
            if grenade {
                Event::GrenadeDanger
            } else {
                Event::ProjectileDanger
            },
            Some(object),
            projectile.origin,
            u32::MAX,
        );
    }
}

pub(crate) fn new_enemy(world: &mut World, source: u64, target: u64, now: i32) {
    let (team, information, source_is_actor, receivers) = {
        let runtime = world.resource::<RoundScript>();
        if runtime.profile != asset_core::RuntimeProfile::Campaign
            || !runtime.started
            || !runtime.can_receive_call(&source)
            || !runtime.can_receive_call(&target)
            || !runtime.sentients.contains_key(&target)
            || runtime
                .actors
                .get(&target)
                .is_some_and(|actor| !actor.alive || actor.delayed_death)
        {
            return;
        }
        let Some(source_state) = runtime.sentients.get(&source) else {
            return;
        };
        let source_actor = runtime.actors.get(&source);
        if source_actor.is_some_and(|actor| !actor.alive) {
            return;
        }
        let information = source_actor
            .and_then(|actor| actor.sentient_info.get(&target))
            .cloned();
        let species = source_actor.map_or(u8::MAX, |actor| actor.talk_to_species);
        let listeners = runtime
            .actors
            .iter()
            .filter(|(id, actor)| {
                **id != source
                    && runtime.can_receive_call(id)
                    && actor.alive
                    && runtime
                        .sentients
                        .get(id)
                        .is_some_and(|sentient| sentient.team == source_state.team)
                    && species & (1 << actor.species as u8) != 0
            })
            .map(|(&id, actor)| (actor.slot, id))
            .collect::<Vec<_>>();
        (
            source_state.team,
            information,
            source_actor.is_some(),
            listeners,
        )
    };
    let Some(at) = origin(world, source) else {
        return;
    };
    let information = if source_is_actor {
        information
    } else {
        let Some(position) = origin(world, target) else {
            return;
        };
        Some(super::actors::perception::SentientInfo {
            last_known_time: now,
            last_known_position: Some(position),
            ..Default::default()
        })
    };
    let mut receivers = receivers;
    receivers.sort_unstable();
    for (_, listener) in receivers {
        let Some(information) = information.as_ref() else {
            break;
        };
        let Some(position) = origin(world, listener) else {
            continue;
        };
        let range = radius(world, Event::NewEnemy, listener, Some(source));
        if glam::Vec3::from_array(position).distance_squared(glam::Vec3::from_array(at))
            > range * range
        {
            continue;
        }
        let mut runtime = world.resource_mut::<RoundScript>();
        let current = runtime
            .actors
            .get_mut(&listener)
            .unwrap()
            .sentient_info
            .entry(target)
            .or_default();
        if information.last_known_position.is_some()
            && (current.last_known_position.is_none()
                || information.last_known_time > current.last_known_time)
        {
            current.last_known_position = information.last_known_position;
            current.last_known_time = information.last_known_time;
        }
        // Reported contact is knowledge, not proof of this listener's sight.
    }
    point(world, Event::NewEnemy, Some(source), at, 1u32 << team);
}
