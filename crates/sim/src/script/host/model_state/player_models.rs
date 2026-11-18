use crate::frame::FrameWorld;
use crate::script::{Namespace, NativeRegistry, RoundScript, Value};
use bevy_ecs::prelude::World;
use std::sync::Arc;

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct PlayerWeaponModels {
    pub(crate) weapon: Option<Arc<str>>,
    pub(crate) knife: Option<Arc<str>>,
}

/// One source for script queries and model-change notifications. The weapon's
/// selected authored appearance and the actual torso animation determine the
/// pair; a melee capability alone does not mean a knife is currently attached.
pub(crate) fn current(world: &mut World, client: u32) -> Result<PlayerWeaponModels, String> {
    let frame = FrameWorld::from_world(world);
    let ps = frame
        .player(crate::ClientId(client))
        .ok_or("player is disconnected")?;
    let turret =
        playerstate_iw4::eflags::TURRET_ACTIVE_PRONE | playerstate_iw4::eflags::TURRET_ACTIVE_DUCK;
    if ps.weapon == 0 || ps.e_flags & turret != 0 {
        return Ok(PlayerWeaponModels::default());
    }
    let library = frame.content();
    let content = library.weapons();
    let index = ps.weapon as usize;
    let (base, _) = content
        .weapon_world_models
        .get(index)
        .ok_or("player weapon has no captured model data")?;
    let variant = weapon_iw4::weapon_model_for_held(&ps.weapons, &ps.weapon_data, ps.weapon);
    let weapon = if variant == 0 {
        Some(base.as_str())
    } else {
        content
            .weapon_world_model_variants
            .get(index)
            .and_then(|variants| variants.iter().find(|(slot, _)| *slot == variant))
            .map(|(_, name)| name.as_str())
    };
    let knife = frame
        .player_knife_melee_animation(ps.torso_anim)
        .then(|| {
            content
                .weapon_world_knife_models
                .get(index)
                .and_then(Option::as_deref)
        })
        .flatten();
    let name = |model: Option<&str>| model.filter(|name| !name.is_empty()).map(Arc::from);
    Ok(PlayerWeaponModels {
        weapon: name(weapon),
        knife: name(knife),
    })
}

fn query(world: &mut World, receiver: &Value, knife: bool) -> Result<Value, String> {
    let client = super::super::natives::player::player(world, receiver)?;
    let runtime = world.resource::<RoundScript>();
    let slot = runtime
        .players
        .get(&client)
        .ok_or("player is disconnected")?;
    if receiver != &Value::Object(slot.object) || !runtime.can_receive_call(&slot.object) {
        return Err("player identity is no longer live".into());
    }
    let models = current(world, client)?;
    Ok(if knife { models.knife } else { models.weapon }
        .map_or(Value::Undefined, |name| Value::string(&name)))
}

pub(crate) fn register(registry: &mut NativeRegistry) {
    registry.register(
        Namespace::Method,
        "getplayerweaponmodel",
        |world, receiver, _| query(world, receiver, false),
    );
    registry.register(
        Namespace::Method,
        "getplayerknifemodel",
        |world, receiver, _| query(world, receiver, true),
    );
}

pub(crate) fn sync(world: &mut World) {
    let players: Vec<_> = world
        .resource::<RoundScript>()
        .players
        .iter()
        .map(|(&client, slot)| (client, slot.object))
        .collect();
    for (client, object) in players {
        let Ok(models) = current(world, client) else {
            continue;
        };
        {
            let mut runtime = world.resource_mut::<RoundScript>();
            let Some(slot) = runtime.players.get_mut(&client) else {
                continue;
            };
            if slot.weapon_models == models {
                continue;
            }
            slot.weapon_models = models;
        }
        crate::script::runtime::raise(world, Value::Object(object), "weaponchange", vec![]);
    }
}
