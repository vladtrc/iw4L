use super::super::entities::EntityKind;
use super::*;

pub(crate) const MAX_CORPSES: usize = 16;
#[derive(Clone, Debug)]
pub(crate) struct Corpse {
    object: u64,
    remove_at: i32,
}
#[derive(Clone, Debug)]
pub(crate) struct Corpses {
    slots: [Option<Corpse>; MAX_CORPSES],
    count: usize,
}
impl Default for Corpses {
    fn default() -> Self {
        Self {
            slots: Default::default(),
            count: MAX_CORPSES,
        }
    }
}
impl Corpses {
    pub(crate) fn remove(&mut self, id: u64) {
        for entry in &mut self.slots {
            if entry.as_ref().is_some_and(|corpse| corpse.object == id) {
                *entry = None;
            }
        }
    }
}

pub(crate) fn handoff(world: &mut World, id: u64, now: i32) -> Result<(), String> {
    let free = {
        let runtime = world.resource::<RoundScript>();
        (0..runtime.actor_corpses.count).find(|&slot| runtime.actor_corpses.slots[slot].is_none())
    };
    let slot = match free {
        Some(slot) => slot,
        None => reclaim_slot(world)?,
    };
    crate::script::runtime::raise(world, Value::Object(id), "death", vec![]);
    let (number, presence) = {
        let mut runtime = world.resource_mut::<RoundScript>();
        let origin = runtime.object_field(id, "origin");
        let angles = runtime.object_field(id, "angles");
        let fields: Vec<_> = [
            "model",
            "classname",
            "code_classname",
            "maxhealth",
            "health",
            "spawnflags",
            "anglelerprate",
        ]
        .into_iter()
        .map(|key| (key, runtime.object_field(id, key)))
        .collect();
        let mut entity = runtime.entities[&id].clone();
        let animator = runtime.animators.remove(&id);
        // Native Actor_BecomeCorpse frees the script entity number, preserving
        // the authority gentity and DObj/tree. Old script fields/threads retire.
        runtime.entities.get_mut(&id).unwrap().presence = None;
        runtime.delete_entity(id);
        let object = runtime.new_object()?;
        entity.kind = EntityKind::ActorCorpse { slot: slot as u8 };
        entity.can_damage = false;
        entity.can_radius_damage = false;
        entity.contents = 0x04000000;
        entity.sound_wait = None;
        entity.usable = None;
        let (number, presence) = (entity.number, entity.presence);
        runtime.entities.insert(object, entity);
        runtime.set_object_field(object, "origin", origin);
        runtime.set_object_field(object, "angles", angles);
        for (key, value) in fields {
            runtime.set_object_field(object, key, value);
        }
        if let Some(animator) = animator {
            runtime.animators.insert(object, animator);
        }
        runtime.actor_corpses.slots[slot] = Some(Corpse {
            object,
            remove_at: 0,
        });
        (number, presence)
    };
    if let Some(presence) = presence {
        let mut frame = crate::frame::FrameWorld::from_world(world);
        let mover = frame
            .script_mover_mut_by_number(number)
            .ok_or("actor corpse presence missing")?;
        if mover.id != presence {
            return Err("actor corpse presence identity changed".into());
        }
        if let Some(bounds) = &mut mover.body {
            bounds.contents = 0x04000000;
        }
        // ActorCorpse stores the SP type. Authority models keep the runtime's
        // existing script-mover wire representation, including their DObj.

        mover.state.tr_time = now;
        mover.state.tr_delta = [0.; 3];
    }
    super::super::presence::settle_collision(world);
    Ok(())
}

fn eviction_rank(at: [f32; 3], observers: &[([f32; 3], [f32; 3])]) -> (u8, f32) {
    let mut behind = true;
    let mut sideways = true;
    let mut nearest = f32::MAX;
    for &(eye, forward) in observers {
        let horizontal = forward[0].hypot(forward[1]);
        let mut delta: [f32; 3] = std::array::from_fn(|i| at[i] - eye[i]);
        let direction = if horizontal > 0. {
            delta[2] = 0.;
            [forward[0] / horizontal, forward[1] / horizontal, 0.]
        } else {
            forward
        };
        let distance = delta.iter().map(|v| v * v).sum::<f32>();
        let projection = (0..3).map(|i| delta[i] * direction[i]).sum::<f32>();
        behind &= projection < 0.;
        sideways &= distance > 0. && projection * projection <= distance * 0.5;
        nearest = nearest.min(distance);
    }
    (if behind { 2 } else { u8::from(sideways) }, nearest)
}

fn reclaim_slot(world: &mut World) -> Result<usize, String> {
    let mut observers = Vec::new();
    crate::frame::FrameWorld::from_world(world).visit_players(|_, player| {
        let mut eye = player.origin;
        eye[2] += player.view_height_current;
        observers.push((eye, math_iw4::angles_to_axis(player.viewangles)[0]));
    });
    if observers.is_empty()
        || observers
            .iter()
            .any(|(eye, forward)| !eye.iter().chain(forward).all(|v| v.is_finite()))
    {
        return Err("corpse eviction requires a finite player view".into());
    }
    let entries = {
        let runtime = world.resource::<RoundScript>();
        runtime.actor_corpses.slots[..runtime.actor_corpses.count].to_vec()
    };
    let mut selected = None;
    for (slot, corpse) in entries.into_iter().enumerate() {
        let corpse = corpse.ok_or("corpse eviction slot became vacant")?;
        let Value::Vector(at) = super::super::players::entity_field(world, corpse.object, "origin")
        else {
            return Err("corpse eviction requires a position".into());
        };
        if !at.iter().all(|v| v.is_finite()) {
            return Err("corpse eviction requires a finite position".into());
        }
        let rank = eviction_rank(at, &observers);
        if selected
            .as_ref()
            .is_none_or(|&(_, _, previous)| rank > previous)
        {
            selected = Some((slot, corpse.object, rank));
        }
    }
    let (slot, object, (category, distance)) = selected.ok_or("actor corpse pool is disabled")?;
    let number = world.resource::<RoundScript>().entities[&object].number;
    perf::instant!("actor_corpse_evicted", "iw4l.sim", |ctx| {
        ctx.arg("entity", i64::from(number));
        ctx.arg("slot", slot as i64);
        ctx.arg("view_category", i64::from(category));
        ctx.arg("nearest_distance_squared", f64::from(distance));
    });
    world.resource_mut::<RoundScript>().delete_entity(object);
    Ok(slot)
}

pub(crate) fn advance(world: &mut World, now: i32) -> Result<(), String> {
    let _span = perf::span_handle!("actor_corpses", "iw4l.sim").enter();
    let requested = world
        .resource::<crate::script::MatchScript>()
        .dvars
        .get("ai_corpsecount")
        .and_then(|value| value.parse::<usize>().ok())
        .unwrap_or(MAX_CORPSES)
        .min(MAX_CORPSES);
    let (count, entries) = {
        let runtime = world.resource::<RoundScript>();
        (
            runtime.actor_corpses.count,
            runtime.actor_corpses.slots.clone(),
        )
    };
    if requested != count {
        let mut runtime = world.resource_mut::<RoundScript>();
        for corpse in entries
            .iter()
            .skip(requested)
            .take(count.saturating_sub(requested))
            .flatten()
        {
            runtime.delete_entity(corpse.object);
        }
        runtime.actor_corpses.count = requested;
    }
    for corpse in entries
        .into_iter()
        .take(requested)
        .flatten()
        .filter(|corpse| corpse.remove_at != 0 && corpse.remove_at <= now)
    {
        let Value::Vector(at) = world
            .resource_mut::<RoundScript>()
            .object_field(corpse.object, "origin")
        else {
            return Err("corpse origin is not a vector".into());
        };
        let players = {
            let frame = crate::frame::FrameWorld::from_world(world);
            let mut players = Vec::new();
            frame.visit_players(|client, player| {
                players.push((
                    client.0,
                    player.origin,
                    player.view_height_current,
                    player.viewangles,
                ));
            });
            players
        };
        let mut removable = !players.is_empty();
        for (client, origin, eye_height, angles) in players {
            let eye = [origin[0], origin[1], origin[2] + eye_height];
            let delta: [f32; 3] = std::array::from_fn(|i| at[i] - eye[i]);
            if delta.iter().map(|v| v * v).sum::<f32>() < 4096. {
                removable = false;
                break;
            }
            let forward = math_iw4::angles_to_axis(angles)[0];
            if forward[0] * delta[0] + forward[1] * delta[1] > 0. {
                let ignore = world
                    .resource::<RoundScript>()
                    .players
                    .get(&client)
                    .map(|slot| slot.object);
                let hit = super::super::trace::sight(world, eye, at, 0x801, ignore.as_slice());
                if hit.fraction == 1. && hit.startsolid == 0 {
                    removable = false;
                    break;
                }
            }
        }
        if removable {
            world
                .resource_mut::<RoundScript>()
                .delete_entity(corpse.object);
        } else {
            for entry in world
                .resource_mut::<RoundScript>()
                .actor_corpses
                .slots
                .iter_mut()
                .flatten()
            {
                if entry.object == corpse.object {
                    entry.remove_at = now + 500;
                }
            }
        }
    }
    Ok(())
}

pub(super) fn register(registry: &mut NativeRegistry) {
    registry.register(Namespace::Function, "clearallcorpses", |world, _, args| {
        if !args.is_empty() {
            return Err("clearallcorpses expects no arguments".into());
        }
        let mut runtime = world.resource_mut::<RoundScript>();
        let objects: Vec<_> = runtime
            .actor_corpses
            .slots
            .iter()
            .flatten()
            .map(|corpse| corpse.object)
            .collect();
        for object in objects {
            runtime.request_delete(object);
        }
        Ok(Value::Undefined)
    });
    registry.register(Namespace::Function, "getcorpsearray", |world, _, _| {
        let values = world
            .resource::<RoundScript>()
            .actor_corpses
            .slots
            .iter()
            .flatten()
            .map(|corpse| Value::Object(corpse.object))
            .collect();
        new_array(world, values)
    });
    registry.register(
        Namespace::Method,
        "setcorpseremovetimer",
        |world, receiver, args| {
            let id = super::super::natives::engine::entity_id(world, receiver)?;
            if !matches!(
                world.resource::<RoundScript>().entities[&id].kind,
                EntityKind::ActorCorpse { .. }
            ) {
                return Err("SetCorpseRemoveTimer must be called on an actor corpse".into());
            }
            let seconds = float(args, 0)?;
            if !seconds.is_finite() {
                return Err("corpse remove time must be finite".into());
            }
            let deadline = (super::super::players::now_ms(world) as i32)
                .wrapping_sub((seconds * -1000.).trunc() as i32);
            let mut runtime = world.resource_mut::<RoundScript>();
            let corpse = runtime
                .actor_corpses
                .slots
                .iter_mut()
                .flatten()
                .find(|corpse| corpse.object == id)
                .ok_or("actor corpse has no slot")?;
            corpse.remove_at = deadline;
            Ok(Value::Undefined)
        },
    );
}
