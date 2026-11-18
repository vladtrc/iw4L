use super::*;

pub(super) fn register(registry: &mut NativeRegistry) {
    registry.register(
        Namespace::Method,
        "setlookatentity",
        |world, receiver, args| {
            let id = actor_id(world, receiver)?;
            let target = args
                .first()
                .map(|value| entity_id(world, value))
                .transpose()?
                .filter(|target| *target != id);
            world
                .resource_mut::<RoundScript>()
                .actors
                .get_mut(&id)
                .unwrap()
                .gaze_target = target;
            perf::instant!("gsc_actor_gaze", "iw4l.sim", "actor" => id, "target" => target);
            Ok(Value::Undefined)
        },
    );
}

pub(crate) fn publish(world: &World, snapshot: &mut crate::Snapshot) {
    let Some(runtime) = world.get_resource::<RoundScript>() else {
        return;
    };
    let reference = |number: i32| {
        let generation = *snapshot
            .meta
            .generations
            .get(usize::try_from(number).ok()?)?;
        crate::EntityRef::from_parts(number, generation).ok()
    };
    let mut gazes = Vec::new();
    for (&id, actor) in &runtime.actors {
        let Some(target) = actor.gaze_target.filter(|_| actor.alive) else {
            continue;
        };
        if !runtime.can_receive_call(&id) || !runtime.can_receive_call(&target) {
            continue;
        }
        let Some(entity) = runtime.entities.get(&id) else {
            continue;
        };
        let Some(target_entity) = runtime.entities.get(&target) else {
            continue;
        };
        let Some(actor) = reference(entity.number) else {
            continue;
        };
        let Ok(target_number) = u16::try_from(target_entity.number) else {
            continue;
        };
        let Some(&target_generation) = snapshot.meta.generations.get(usize::from(target_number))
        else {
            continue;
        };
        let origin_symbol = runtime
            .program
            .as_ref()
            .and_then(|program| program.symbol_ids.get("origin"))
            .or_else(|| runtime.dynamic_symbols.get("origin"));
        let target_origin = origin_symbol
            .and_then(|symbol| runtime.objects.get(&target)?.get(symbol))
            .and_then(|value| {
                if let Value::Vector(origin) = value {
                    Some(*origin)
                } else {
                    None
                }
            });
        if actor.number() != target_entity.number {
            gazes.push(crate::ActorGaze {
                actor,
                target_number,
                target_generation,
                target_origin,
                target_is_actor: matches!(
                    target_entity.kind,
                    super::super::entities::EntityKind::Actor
                        | super::super::entities::EntityKind::ActorCorpse { .. }
                ),
            });
        }
    }
    gazes.sort_unstable_by_key(|gaze| gaze.actor.number());
    snapshot.meta.actor_gazes = gazes;
}
