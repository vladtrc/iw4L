use super::super::arrays::new_array;
use super::super::entities::EntityKind;
use crate::script::{Namespace, NativeRegistry, Runtime, Value};
use bevy_ecs::prelude::World;

pub(crate) fn register(registry: &mut NativeRegistry) {
    use Namespace::{Function, Method};
    registry.register(Function, "getplayers", |world, _, args| {
        let team = optional_team(args)?;
        let players = players(world)
            .into_iter()
            .filter(|&id| team.as_deref().is_none_or(|team| on_team(world, id, team)))
            .map(Value::Object)
            .collect();
        new_array(world, players)
    });
    registry.register(Function, "getaiarray", |world, _, args| {
        let team = optional_team(args)?;
        let actors = living_actors(world, team.as_deref())
            .into_iter()
            .map(Value::Object)
            .collect();
        new_array(world, actors)
    });
    // `GetAISpeciesArray(team, species)`: every actor here is of one species.
    registry.register(Function, "getaispeciesarray", |world, _, args| {
        let team = optional_team(&args[..args.len().min(1)])?;
        let actors = living_actors(world, team.as_deref())
            .into_iter()
            .map(Value::Object)
            .collect();
        new_array(world, actors)
    });
    registry.register(Function, "issentient", |world, _, args| {
        let sentient = match world
            .resource::<Runtime>()
            .entity(super::super::args::arg(args, 0)?)
        {
            Some((_, entity)) => matches!(entity.kind, EntityKind::Player | EntityKind::Actor),
            None => false,
        };
        Ok(Value::Int(sentient.into()))
    });
    registry.register(Function, "getspawnerarray", |world, _, args| {
        no_args(args)?;
        let spawners = spawners(world).into_iter().map(Value::Object).collect();
        new_array(world, spawners)
    });
    // Script pacing on snapshot acknowledgement is left to the network layer:
    // scripts see no remote clients and wait a tenth of a second instead.
    registry.register(Function, "numremoteclients", |_, _, args| {
        no_args(args)?;
        Ok(Value::Int(0))
    });
    registry.register(Function, "getdifficulty", |_, _, args| {
        no_args(args)?;
        Ok(Value::string("medium"))
    });
    registry.register(Function, "issaverecentlyloaded", |_, _, args| {
        no_args(args)?;
        Ok(Value::Int(0))
    });
    // The host keeps no AI budget, water simulation or exploder ids of its own.
    registry.register(Function, "setailimit", |_, _, _| Ok(Value::Undefined));
    registry.register(Function, "watersimenable", |_, _, _| Ok(Value::Undefined));
    registry.register(Method, "setexploderid", |_, _, _| Ok(Value::Undefined));
    // Script-state tracing is a developer aid; eye glow is drawn from the head.
    registry.register(Method, "trackscriptstate", |_, _, _| Ok(Value::Undefined));
    registry.register(Method, "haseyes", |_, _, _| Ok(Value::Undefined));
    registry.register(Method, "isnotarget", |world, receiver, _| {
        let id = match receiver {
            Value::Object(id) => *id,
            _ => return Err("receiver is not an entity".into()),
        };
        let notarget = world.resource_mut::<Runtime>().object_field(id, "notarget");
        Ok(Value::Int(i32::from(
            matches!(notarget, Value::Int(n) if n != 0),
        )))
    });
    registry.register(Function, "getnumconnectedplayers", |world, _, args| {
        no_args(args)?;
        Ok(Value::Int(players(world).len() as i32))
    });
    // Scripts wait while fewer than the expected players are connected: the
    // host's lobby members, or whoever is connected outside a lobby.
    registry.register(Function, "getnumexpectedplayers", |world, _, args| {
        no_args(args)?;
        let expected = world.resource::<Runtime>().expected_players;
        Ok(Value::Int(players(world).len().max(expected) as i32))
    });
    // Hints to engine systems the host does not model: AI perception events,
    // threat bias between groups (zombies always hunt the closest player),
    // engagement ranges, player shove and knockback, the health shield, view
    // blur, rope physics, weapon hide-tags on display models, the transporter
    // screen effect, save games and network stat reports.
    for name in [
        "addaieventlistener",
        "setthreatbiasgroup",
        "setteamforentity",
        "setengagementmindist",
        "setengagementmaxdist",
        "pushplayer",
        "playerknockback",
        "enablehealthshield",
        "startfadingblur",
        "setblur",
        "useweaponhidetags",
        "settransported",
        // A perk machine shaking as the power comes on.
        "vibrate",
        // The downed player's "being revived" view; the revive itself is scripted.
        "startrevive",
        "stoprevive",
        // Gibbed limbs are not thrown yet; scripts swap the damaged models.
        "gib",
    ] {
        registry.register(Method, name, |_, _, _| Ok(Value::Undefined));
    }
    // Stance and melee locks (while a perk bottle is drunk) are accepted but
    // not enforced yet.
    for name in [
        "allowlean",
        "allowcrouch",
        "allowprone",
        "allowstand",
        "allowmelee",
    ] {
        registry.register(Method, name, |world, receiver, _| {
            super::player::player(world, receiver)?;
            Ok(Value::Undefined)
        });
    }
    registry.register(Method, "enableinvulnerability", |world, receiver, _| {
        set_invulnerable(world, receiver, true)
    });
    registry.register(Method, "disableinvulnerability", |world, receiver, _| {
        set_invulnerable(world, receiver, false)
    });
    // No search for a spot that sees a lost enemy: the reacquire steps fail
    // and the animscripts fall back to running at the enemy along the paths.
    registry.register(Method, "reacquirestep", |_, _, _| Ok(Value::Int(0)));
    registry.register(Method, "usereacquirenode", |_, _, _| Ok(Value::Int(0)));
    for name in [
        "findreacquirenode",
        "getreacquirenode",
        "flagenemyunattackable",
    ] {
        registry.register(Method, name, |_, _, _| Ok(Value::Undefined));
    }
    // No gas weapons (nova gas) exist in the zombie maps this host runs.
    registry.register(Function, "weaponisgasweapon", |_, _, _| Ok(Value::Int(0)));
    // A dual-wield weapon's left hand shares its clip here: there is no
    // separate left-hand weapon.
    registry.register(Function, "weapondualwieldweaponname", |_, _, _| {
        Ok(Value::string("none"))
    });
    // No water volumes are simulated.
    registry.register(Method, "depthinwater", |_, _, _| Ok(Value::Float(0.0)));
    // The last-stand vision set is not drawn yet.
    registry.register(Method, "visionsetlaststand", |world, receiver, _| {
        super::player::player(world, receiver)?;
        Ok(Value::Undefined)
    });
    for name in [
        "createthreatbiasgroup",
        "setthreatbias",
        "ropesetflag",
        "savegame",
        "reportmtu",
        "stopallrumbles",
        "disablegrenadesuicide",
    ] {
        registry.register(Function, name, |_, _, _| Ok(Value::Undefined));
    }
}

fn set_invulnerable(world: &mut World, receiver: &Value, on: bool) -> Result<Value, String> {
    let client = super::player::player(world, receiver)?;
    if let Some(slot) = world.resource_mut::<Runtime>().players.get_mut(&client) {
        slot.invulnerable = on;
    }
    Ok(Value::Undefined)
}

fn optional_team(args: &[Value]) -> Result<Option<String>, String> {
    match args {
        [] | [Value::Undefined] => Ok(None),
        [Value::String(team)] if team.as_bytes() == b"all" => Ok(None),
        [Value::String(team)] => Ok(Some(team.to_string())),
        _ => Err("takes an optional team name".into()),
    }
}

fn on_team(world: &mut World, id: u64, team: &str) -> bool {
    match world.resource_mut::<Runtime>().object_field(id, "team") {
        Value::String(own) => own.as_bytes() == team.as_bytes(),
        _ => true,
    }
}

/// Map actors flagged as spawners, in entity order.
fn spawners(world: &mut World) -> Vec<u64> {
    let candidates: Vec<u64> = world
        .resource::<Runtime>()
        .entities
        .iter()
        .filter(|(_, entity)| entity.classname.starts_with("actor_"))
        .map(|(id, _)| *id)
        .collect();
    let mut runtime = world.resource_mut::<Runtime>();
    candidates
        .into_iter()
        .filter(|&id| matches!(runtime.object_field(id, "spawnflags"), Value::Int(flags) if flags & 1 != 0))
        .collect()
}

/// Actors alive on `team` (or any team), in entity order.
fn living_actors(world: &mut World, team: Option<&str>) -> Vec<u64> {
    let actors: Vec<u64> = world
        .resource::<Runtime>()
        .entities
        .iter()
        .filter(|(_, entity)| entity.kind == EntityKind::Actor)
        .map(|(id, _)| *id)
        .collect();
    let mut runtime = world.resource_mut::<Runtime>();
    actors
        .into_iter()
        .filter(|&id| {
            let alive = match runtime.object_field(id, "health") {
                Value::Int(n) => n > 0,
                Value::Float(f) => f > 0.0,
                _ => false,
            };
            let on_team = team.is_none_or(|team| {
                matches!(runtime.object_field(id, "team"), Value::String(own) if own.as_bytes() == team.as_bytes())
            });
            alive && on_team
        })
        .collect()
}

fn no_args(args: &[Value]) -> Result<(), String> {
    if args.is_empty() {
        Ok(())
    } else {
        Err("takes no arguments".into())
    }
}

/// Player entities in client order.
fn players(world: &mut World) -> Vec<u64> {
    let mut players: Vec<(i32, u64)> = world
        .resource::<Runtime>()
        .entities
        .iter()
        .filter(|(_, entity)| entity.kind == EntityKind::Player)
        .map(|(id, entity)| (entity.number, *id))
        .collect();
    players.sort_unstable();
    players.into_iter().map(|(_, id)| id).collect()
}
