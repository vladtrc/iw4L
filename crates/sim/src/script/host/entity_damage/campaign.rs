use bevy_ecs::prelude::World;

use super::ScriptHit;
use crate::frame::FrameWorld;
use crate::script::Namespace::Method;
use crate::script::host::args::arg;
use crate::script::host::natives::engine::entity_id;
use crate::script::{NativeRegistry, RoundScript, Value};
use crate::world::ClientId;

pub(super) fn invulnerable(world: &World, object: u64) -> bool {
    world
        .resource::<RoundScript>()
        .campaign
        .invulnerable
        .contains(&object)
}

pub(super) fn shield_player(world: &mut World, client: ClientId, hit: &mut ScriptHit) -> bool {
    let runtime = world.resource::<RoundScript>();
    let Some(object) = runtime.players.get(&client.0).map(|slot| slot.object) else {
        return true;
    };
    if hit.means == "MOD_SUICIDE" {
        return true;
    }
    if runtime.campaign.invulnerable.contains(&object) {
        return false;
    }
    if runtime.campaign.death_shield.contains(&object) {
        let health = FrameWorld::from_world(world)
            .player(client)
            .map_or(0, |ps| ps.health);
        if health > 1 {
            hit.amount = hit.amount.min(health - 1);
        }
    }
    true
}

fn set(world: &mut World, receiver: &Value, on: bool, shield: bool) -> Result<Value, String> {
    let id = entity_id(world, receiver)?;
    let mut runtime = world.resource_mut::<RoundScript>();
    let set = if shield {
        &mut runtime.campaign.death_shield
    } else {
        &mut runtime.campaign.invulnerable
    };
    if on {
        set.insert(id);
    } else {
        set.remove(&id);
    }
    Ok(Value::Undefined)
}

pub(crate) fn register(registry: &mut NativeRegistry) {
    registry.register(Method, "enableinvulnerability", |world, receiver, _| {
        set(world, receiver, true, false)
    });
    registry.register(Method, "disableinvulnerability", |world, receiver, _| {
        set(world, receiver, false, false)
    });
    registry.register(
        crate::script::Namespace::Function,
        "isexplosivedamagemod",
        |_, _, args| {
            let explosive = matches!(
                arg(args, 0)?,
                Value::String(m) if matches!(
                    &*m.to_string(),
                    "MOD_EXPLOSIVE_BULLET"
                        | "MOD_GRENADE"
                        | "MOD_GRENADE_SPLASH"
                        | "MOD_PROJECTILE"
                        | "MOD_PROJECTILE_SPLASH"
                        | "MOD_EXPLOSIVE"
                )
            );
            Ok(Value::Int(explosive.into()))
        },
    );
    registry.register(Method, "enablehealthshield", |world, receiver, args| {
        let id = entity_id(world, receiver)?;
        let on = !matches!(arg(args, 0)?, Value::Int(0) | Value::Undefined);
        let off = &mut world
            .resource_mut::<RoundScript>()
            .campaign
            .health_shield_off;
        if on {
            off.remove(&id);
        } else {
            off.insert(id);
        }
        Ok(Value::Undefined)
    });
    registry.register(Method, "enabledeathshield", |world, receiver, args| {
        let on = !matches!(arg(args, 0)?, Value::Int(0) | Value::Undefined);
        set(world, receiver, on, true)
    });
}
