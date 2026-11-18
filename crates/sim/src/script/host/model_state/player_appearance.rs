use crate::bullet_collision::AuthorityDObjState;
use crate::frame::FrameWorld;
use crate::script::{RoundScript, Value};
use crate::{ClientId, LifeSequence};
use bevy_ecs::prelude::World;
use std::sync::Arc;

/// Authority state for an actual player incarnation, independent of script movers.
#[derive(Clone, Debug)]
pub(crate) struct PlayerAppearance {
    pub(crate) object: u64,
    pub(crate) life: LifeSequence,
    pub(crate) generation: u32,
    pub(crate) dobj: Option<AuthorityDObjState>,
}

pub(crate) fn sync_receiver(world: &mut World, object: u64) {
    if let Some(client) = world.resource::<RoundScript>().player_client(object) {
        sync_one(world, client);
    }
}

pub(crate) fn sync_all(world: &mut World) {
    let clients: Vec<_> = world
        .resource::<RoundScript>()
        .players
        .keys()
        .copied()
        .collect();
    for client in clients {
        sync_one(world, client);
    }
}

pub(crate) fn sync_one(world: &mut World, client: u32) {
    let (object, model, attachments) = {
        let mut runtime = world.resource_mut::<RoundScript>();
        let Some(slot) = runtime.players.get(&client) else {
            return;
        };
        let object = slot.object;
        if !runtime.can_receive_call(&object) {
            return;
        }
        let model = match runtime.object_field(object, "model") {
            Value::String(model) => Some(model),
            _ => None,
        };
        (object, model, runtime.entities[&object].attachments.clone())
    };
    let Some(model) = model else {
        world
            .resource_mut::<RoundScript>()
            .players
            .get_mut(&client)
            .unwrap()
            .appearance = None;
        return;
    };
    let prepared = {
        let frame = FrameWorld::from_world(world);
        frame.player(ClientId(client)).and_then(|ps| {
            let meta = frame.client_meta(ClientId(client))?;
            let generation = frame.entity_kernel().reserved_generation(client as i32)?;
            let kit = frame.collision_kit(ClientId(client), ps.origin);
            let root = frame
                .model_capability(&model.to_ascii_lowercase())
                .flatten();
            let use_kit = root.is_none() && model.starts_with("mp_body_") && kit.body.is_some();
            let (model, root) = if use_kit {
                (kit.body_key.as_str().into(), kit.body.clone())
            } else {
                (model, root)
            };
            let attached: Vec<_> = attachments
                .iter()
                .filter_map(|(model, tag)| {
                    if use_kit && tag.is_empty() && model.starts_with("head_") {
                        return kit.head.as_ref().map(|head| {
                            (
                                Arc::from(kit.head_key.as_str()),
                                tag.clone(),
                                Some(Arc::clone(head)),
                            )
                        });
                    }
                    Some((
                        model.clone(),
                        tag.clone(),
                        frame
                            .model_capability(&model.to_ascii_lowercase())
                            .flatten(),
                    ))
                })
                .collect();
            Some((
                model,
                meta.life_sequence,
                generation,
                ps.origin,
                [0.0, ps.viewangles[1], 0.0],
                frame.player_model_request(ClientId(client), ps),
                root,
                attached,
            ))
        })
    };
    let Some((model, life, generation, origin, angles, request, root, attached)) = prepared else {
        world
            .resource_mut::<RoundScript>()
            .players
            .get_mut(&client)
            .unwrap()
            .appearance = None;
        return;
    };
    let mut runtime = world.resource_mut::<RoundScript>();
    let slot = runtime.players.get_mut(&client).unwrap();
    if slot
        .appearance
        .as_ref()
        .is_none_or(|old| old.object != object || old.life != life || old.generation != generation)
    {
        slot.appearance = Some(PlayerAppearance {
            object,
            life,
            generation,
            dobj: None,
        });
    }
    let appearance = slot.appearance.as_mut().unwrap();
    if model.is_empty() {
        appearance.dobj = None;
        return;
    }
    let dobj = appearance
        .dobj
        .get_or_insert_with(|| AuthorityDObjState::at_pose(&model, root.clone(), origin, angles));
    let same_capability = match (&dobj.capability, &root) {
        (Some(old), Some(new)) => std::sync::Arc::ptr_eq(old, new),
        (None, None) => true,
        _ => false,
    };
    if dobj.current_model != model.as_ref() || !same_capability {
        dobj.replace_model(&model, root);
    }
    dobj.set_attachments(
        &attached
            .iter()
            .map(|(model, tag, cap)| (model.as_ref(), tag.as_ref(), cap.clone()))
            .collect::<Vec<_>>(),
    );
    dobj.set_world_pose(origin, angles);
    match &request.tree {
        Some(tree) => dobj.set_script_animation_tree(tree),
        None => dobj.clear_script_animation_tree(),
    }
    dobj.set_player_controller(request.player_controller);
    debug_assert_eq!(runtime.players[&client].object, object);
}

pub(crate) fn edit_model<T>(
    world: &mut World,
    object: u64,
    change: impl FnOnce(&mut AuthorityDObjState) -> Result<T, String>,
) -> Result<T, String> {
    let client = world
        .resource::<RoundScript>()
        .player_client(object)
        .ok_or("receiver is not a player")?;
    sync_one(world, client);
    let mut runtime = world.resource_mut::<RoundScript>();
    let slot = runtime
        .players
        .get_mut(&client)
        .filter(|slot| slot.object == object)
        .ok_or("player identity is no longer live")?;
    let dobj = slot
        .appearance
        .as_mut()
        .and_then(|appearance| appearance.dobj.as_mut())
        .ok_or("entity has no model")?;
    change(dobj)
}

pub(crate) fn publish(world: &World, snapshot: &mut crate::Snapshot) {
    snapshot.meta.player_dobjs.clear();
    let Some(runtime) = world.get_resource::<RoundScript>() else {
        return;
    };
    for (&client, slot) in &runtime.players {
        let Some(appearance) = &slot.appearance else {
            continue;
        };
        if appearance.object != slot.object
            || !runtime.can_receive_call(&slot.object)
            || !snapshot.players.iter().any(|(id, _)| id.0 == client)
            || snapshot
                .meta
                .for_client(ClientId(client))
                .is_none_or(|meta| meta.life_sequence != appearance.life)
            || snapshot.meta.generations.get(client as usize) != Some(&appearance.generation)
        {
            continue;
        }
        snapshot.meta.player_dobjs.push(crate::PlayerDObjSnapshot {
            client: ClientId(client),
            generation: appearance.generation,
            life_sequence: appearance.life,
            semantic: appearance
                .dobj
                .as_ref()
                .map(|dobj| dobj.semantic_state.clone()),
        });
    }
}

// Compare the authoritative identity and semantic pose, not materialization caches.
impl PartialEq for PlayerAppearance {
    fn eq(&self, other: &Self) -> bool {
        self.object == other.object
            && self.life == other.life
            && self.generation == other.generation
            && self
                .dobj
                .as_ref()
                .map(|dobj| (&dobj.semantic_state, dobj.world_from_model))
                == other
                    .dobj
                    .as_ref()
                    .map(|dobj| (&dobj.semantic_state, dobj.world_from_model))
    }
}
