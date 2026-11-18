use super::*;

pub(super) fn register(registry: &mut NativeRegistry) {
    for (name, hidden) in [("hideonclient", true), ("showonclient", false)] {
        let native: super::super::registry::Native = if hidden { hide } else { show };
        registry.register(Namespace::Method, name, native);
    }
}
fn hide(world: &mut World, receiver: &Value, args: &[Value]) -> Result<Value, String> {
    set(world, receiver, args, true)
}
fn show(world: &mut World, receiver: &Value, args: &[Value]) -> Result<Value, String> {
    set(world, receiver, args, false)
}
fn set(world: &mut World, receiver: &Value, args: &[Value], hidden: bool) -> Result<Value, String> {
    let object = super::super::natives::engine::entity_id(world, receiver)?;
    let viewer = super::super::natives::player::player(world, super::super::args::arg(args, 0)?)?;
    let bit = 1u64
        .checked_shl(viewer)
        .ok_or("visibility viewer exceeds client mask capacity")?;
    if let EntityKind::Corpse {
        slot,
        entnum,
        generation,
        ..
    } = world.resource::<RoundScript>().entities[&object].kind
    {
        let frame = FrameWorld::from_world(world);
        if frame.entity_kernel().reserved_generation(entnum) != Some(generation)
            || !frame
                .corpses()
                .slots
                .get(usize::from(slot))
                .is_some_and(|corpse| corpse.occupied && corpse.entnum == entnum)
        {
            return Err("visibility corpse identity is no longer live".into());
        }
    }
    let mut runtime = world.resource_mut::<RoundScript>();
    if !runtime.can_receive_call(&object) {
        return Err("visibility receiver is no longer live".into());
    }
    let entity = runtime.entities.get_mut(&object).unwrap();
    if hidden {
        entity.hidden_from |= bit;
        entity.shown_to &= !bit;
    } else {
        entity.hidden_from &= !bit;
        if entity.hidden {
            entity.shown_to |= bit;
        }
    }
    Ok(Value::Undefined)
}

// Resolve identities against this frame's payloads, not a script object's old
// entity number, which may already belong to a later allocation.
fn number(
    runtime: &RoundScript,
    object: u64,
    entity: &super::super::entities::ScriptEntity,
    snapshot: &crate::Snapshot,
) -> Option<i32> {
    if let Some(client) = runtime.player_client(object) {
        return snapshot
            .players
            .iter()
            .any(|(id, _)| id.0 == client)
            .then_some(client as i32);
    }
    match entity.kind {
        EntityKind::Missile(id) => snapshot
            .projectiles
            .iter()
            .find(|projectile| projectile.id == id && runtime.missiles.get(&id) == Some(&object))
            .map(|projectile| projectile.entnum),
        EntityKind::Item(number) => snapshot
            .meta
            .item_ammo
            .iter()
            .any(|item| item.entnum == number)
            .then_some(number),
        EntityKind::Corpse {
            slot,
            entnum,
            generation,
            ..
        } => snapshot
            .meta
            .corpses
            .slots
            .get(usize::from(slot))
            .filter(|corpse| {
                corpse.occupied
                    && corpse.entnum == entnum
                    && snapshot.meta.generations.get(entnum as usize) == Some(&generation)
            })
            .map(|corpse| corpse.entnum),
        _ => entity.presence.and_then(|presence| {
            snapshot
                .meta
                .script_movers
                .iter()
                .find(|mover| mover.id == presence)
                .map(|mover| mover.state.number)
        }),
    }
}

pub(super) fn publish(world: &World, snapshot: &mut crate::Snapshot) {
    let Some(runtime) = world.get_resource::<RoundScript>() else {
        return;
    };
    let mut masks = std::collections::BTreeMap::<u16, u64>::new();
    for (&object, entity) in &runtime.entities {
        let mask = entity.hidden_from | if entity.hidden { !entity.shown_to } else { 0 };
        if mask == 0 || !runtime.can_receive_call(&object) {
            continue;
        }
        if let Some(number) =
            number(runtime, object, entity, snapshot).and_then(|number| u16::try_from(number).ok())
        {
            *masks.entry(number).or_default() |= mask;
        }
    }
    snapshot.meta.entity_hidden_from = masks.into_iter().collect();
}
