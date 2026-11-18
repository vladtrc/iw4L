use super::args::{int, string};
use super::entities::EntityKind;
use crate::frame::FrameWorld;
use crate::script::{Namespace, NativeRegistry, RoundScript, Value};
use bevy_ecs::prelude::World;

pub(crate) mod player_appearance;
pub(crate) mod player_models;
mod visibility;

fn capability(
    world: &mut World,
    args: &[Value],
) -> Result<std::sync::Arc<xmodel_runtime::RetainedModelCapability>, String> {
    let model = string(args, 0)?;
    FrameWorld::from_world(world)
        .model_capability(&model.to_ascii_lowercase())
        .flatten()
        .ok_or_else(|| format!("model '{model}' has no retained bone data"))
}

pub(crate) fn register(registry: &mut NativeRegistry) {
    visibility::register(registry);
    player_models::register(registry);
    registry.register(Namespace::Function, "getnumparts", |world, _, args| {
        if args.len() != 1 {
            return Err("GetNumParts expects one model name".into());
        }
        let model = capability(world, args)?;
        Ok(Value::Int(i32::try_from(model.pose.num_bones).map_err(
            |_| "model bone count exceeds script integer range",
        )?))
    });
    registry.register(Namespace::Function, "getpartname", |world, _, args| {
        if args.len() != 2 {
            return Err("GetPartName expects model name and bone index".into());
        }
        let model = capability(world, args)?;
        let index = int(args, 1)?;
        let name = usize::try_from(index)
            .ok()
            .filter(|index| *index < model.pose.num_bones)
            .and_then(|index| model.pose.bone_names.get(index))
            .filter(|name| !name.is_empty())
            .ok_or("model bone index is out of range or has no name")?;
        Ok(Value::string(name))
    });
    registry.register(
        Namespace::Method,
        "dontinterpolate",
        |world, receiver, _| {
            let object = super::natives::engine::entity_id(world, receiver)?;
            let runtime = world.resource::<RoundScript>();
            if !runtime.can_receive_call(&object) {
                return Err("DontInterpolate receiver is no longer live".into());
            }
            let client = runtime.player_client(object);
            let kind = runtime.entities[&object].kind.clone();
            if let Some(client) = client {
                FrameWorld::from_world(world)
                    .player_mut(crate::ClientId(client))
                    .ok_or("DontInterpolate player is disconnected")?
                    .e_flags ^= playerstate_iw4::eflags::TELEPORT;
            } else if let EntityKind::Corpse {
                slot,
                entnum,
                generation,
                ..
            } = kind
            {
                let mut frame = FrameWorld::from_world(world);
                if frame.entity_kernel().reserved_generation(entnum) != Some(generation) {
                    return Err("DontInterpolate corpse identity is no longer live".into());
                }
                let corpse = frame
                    .corpses_mut()
                    .slots
                    .get_mut(usize::from(slot))
                    .filter(|corpse| corpse.occupied && corpse.entnum == entnum)
                    .ok_or("DontInterpolate corpse is no longer live")?;
                corpse.e_flags ^= playerstate_iw4::eflags::TELEPORT;
            } else if let EntityKind::Item(number) = kind {
                FrameWorld::from_world(world)
                    .dropped_item_mut_by_number(number)
                    .ok_or("DontInterpolate item is no longer live")?
                    .state
                    .e_flags ^= playerstate_iw4::eflags::TELEPORT;
            } else {
                let mut runtime = world.resource_mut::<RoundScript>();
                let entity = runtime.entities.get_mut(&object).unwrap();
                entity.interpolation_toggle = !entity.interpolation_toggle;
            }
            Ok(Value::Undefined)
        },
    );
}

// EntityState already carries the discontinuity bit through the wire and into
// CEntityRuntime. Keep the script toggle independent of physical trajectory
// updates and match actual model/projectile identity, never a reused number.
pub(crate) fn publish(world: &World, snapshot: &mut crate::Snapshot) {
    super::actors::gaze::publish(world, snapshot);
    visibility::publish(world, snapshot);
    player_appearance::publish(world, snapshot);
    let Some(runtime) = world.get_resource::<RoundScript>() else {
        return;
    };
    let mut numbers = std::collections::BTreeSet::new();
    for (&object, entity) in &runtime.entities {
        if !entity.interpolation_toggle || !runtime.can_receive_call(&object) {
            continue;
        }
        let number = match entity.kind {
            EntityKind::Missile(id) => snapshot
                .projectiles
                .iter()
                .find(|projectile| {
                    projectile.id == id && runtime.missiles.get(&id) == Some(&object)
                })
                .map(|projectile| projectile.entnum),
            _ => entity.presence.and_then(|presence| {
                snapshot
                    .meta
                    .script_movers
                    .iter()
                    .find(|mover| mover.id == presence)
                    .map(|mover| mover.state.number)
            }),
        };
        if let Some(number) = number {
            numbers.insert(number);
        }
    }
    for state in &mut snapshot.meta.entities {
        if numbers.contains(&state.number) {
            state.e_flags ^= playerstate_iw4::eflags::TELEPORT;
        }
    }
    for mover in &mut snapshot.meta.script_movers {
        if numbers.contains(&mover.state.number) {
            mover.state.e_flags ^= playerstate_iw4::eflags::TELEPORT;
        }
    }
}
