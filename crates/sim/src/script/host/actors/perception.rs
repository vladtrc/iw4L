use super::*;

mod memory;
mod sight;
pub(super) use sight::eye;
pub(super) use sight::{EyeBatch, can_see, can_see_batched};
pub(super) use sight::{FIELDS, Sight};

pub(super) fn register(registry: &mut NativeRegistry) {
    sight::register(registry);
    memory::register(registry);
}

#[derive(Clone, Debug, Default)]
pub(crate) struct SentientInfo {
    pub last_known_time: i32,
    pub last_known_position: Option<[f32; 3]>,
    pub visible: bool,
    pub visibility_updated_at: Option<i32>,
    pub last_visible_time: Option<i32>,
    pub attack_time: i32,
}

pub(crate) fn perfect_spawn_info(world: &mut World, actor: u64, now: i32) -> Result<(), String> {
    let owners: Vec<_> = world
        .resource::<RoundScript>()
        .sentients
        .keys()
        .copied()
        .collect();
    for owner in owners {
        let origin = super::super::players::entity_field(world, owner, "origin");
        let Value::Vector(origin) = origin else {
            return Err("sentient has no origin".into());
        };
        world
            .resource_mut::<RoundScript>()
            .actors
            .get_mut(&actor)
            .ok_or("spawned actor was deleted")?
            .sentient_info
            .insert(
                owner,
                SentientInfo {
                    last_known_time: now,
                    last_known_position: Some(origin),
                    ..Default::default()
                },
            );
    }
    Ok(())
}

pub(crate) fn share_spawn_info(world: &mut World, child: u64) {
    let runtime = world.resource::<RoundScript>();
    let Some(team) = runtime.sentients.get(&child).map(|sentient| sentient.team) else {
        return;
    };
    let mut sources: Vec<_> = runtime
        .actors
        .iter()
        .filter(|(id, _)| {
            **id != child
                && runtime.live(id)
                && runtime
                    .sentients
                    .get(id)
                    .is_some_and(|sentient| sentient.team == team)
        })
        .map(|(&id, actor)| (actor.slot, id))
        .collect();
    sources.sort_unstable();
    let species = runtime.actors[&child].species as u8;
    let mut copied = Vec::new();
    for (_, source) in sources {
        if runtime.actors[&source].talk_to_species & (1 << species) == 0 {
            continue;
        }
        copied.extend(
            runtime.actors[&source]
                .sentient_info
                .iter()
                .filter(|(owner, _)| runtime.sentients.contains_key(owner) && runtime.live(owner))
                .map(|(&owner, info)| (owner, info.clone())),
        );
    }
    let mut runtime = world.resource_mut::<RoundScript>();
    let target = &mut runtime.actors.get_mut(&child).unwrap().sentient_info;
    for (owner, incoming) in copied {
        let current = target.entry(owner).or_default();
        if incoming.last_known_position.is_some()
            && (current.last_known_position.is_none()
                || incoming.last_known_time > current.last_known_time)
        {
            current.last_known_time = incoming.last_known_time;
            current.last_known_position = incoming.last_known_position;
        }
    }
}
