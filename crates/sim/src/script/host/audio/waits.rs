use super::super::args::{byte_string, int, string};
use crate::frame::FrameWorld;
use crate::script::{RoundScript, Value};
use bevy_ecs::prelude::World;
use std::sync::Arc;

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct SoundWait {
    notify: Arc<str>,
    alias_index: u16,
    replace_allowed: bool,
    completed: bool,
}

fn completion_client(world: &World) -> Option<crate::ClientId> {
    world
        .get_resource::<RoundScript>()?
        .restart
        .as_ref()?
        .campaign_player
}

pub(crate) fn play(
    world: &mut World,
    receiver: &Value,
    args: &[Value],
    master: bool,
) -> Result<Value, String> {
    let id = super::super::natives::engine::entity_id(world, receiver)?;
    let alias = string(args, 0)?;
    let mut frame = FrameWorld::from_world(world);
    if frame.script_sound_is_looping(&alias)? {
        return Err("cannot play a looping alias as a one-shot sound".into());
    }
    let alias_index = u16::from(frame.sound_alias_index(&alias));
    if alias_index == 0 {
        return Err("sound alias configstring table is full".into());
    }
    if !(1..=3).contains(&args.len()) {
        return Err("expected alias, optional notify and replacement flag".into());
    }
    let entity = u16::try_from(world.resource::<RoundScript>().entities[&id].number)
        .ok()
        .filter(|number| *number < 2046)
        .ok_or("invalid sound entity number")?;
    let origin = super::super::natives::engine::origin_of(world, receiver)?;
    if args.len() == 1 {
        let tick = world.resource::<crate::step::StepRequest>().tick;
        FrameWorld::from_world(world).push_entity_event(
            tick,
            crate::EventAudience::All,
            if master {
                entity_iw4::EntityEventKind::SOUND_ALIAS_AS_MASTER
            } else {
                entity_iw4::EntityEventKind::SOUND_ALIAS
            },
            crate::EntityEventPayload {
                number: i32::from(entity),
                event_parm: i32::from(alias_index),
                origin,
                ..Default::default()
            },
        );
        return Ok(Value::Undefined);
    }
    let replace_allowed = if args.len() == 3 {
        int(args, 2)? != 0
    } else {
        false
    };
    let notify = byte_string(args, 1)?.symbol_key();
    let completion_client =
        completion_client(world).ok_or("tracked entity sound requires a campaign audio owner")?;
    let command = crate::ScriptAudioCommand::EntitySound {
        entity,
        alias_index,
        alias,
        origin,
        completion_client,
        master,
    };
    if !command.valid() {
        return Err("invalid entity sound command".into());
    }
    // SP sends playback before registering the wait. Replacement failure must
    // keep that playback and the old notification's partial effects.
    FrameWorld::from_world(world).push_script_audio(command);
    let old = world.resource::<RoundScript>().entities[&id]
        .sound_wait
        .clone();
    if let Some(old) = old {
        crate::script::runtime::raise(world, receiver.clone(), &old.notify, Vec::new());
        if !old.replace_allowed || !replace_allowed {
            return Err(
                "entity is already waiting on a sound; both sounds must permit replacement".into(),
            );
        }
    }
    world
        .resource_mut::<RoundScript>()
        .entities
        .get_mut(&id)
        .unwrap()
        .sound_wait = Some(SoundWait {
        notify,
        alias_index,
        replace_allowed,
        completed: false,
    });
    Ok(Value::Undefined)
}

pub(crate) fn acknowledge(
    world: &mut World,
    client: crate::ClientId,
    entity: u16,
    alias_index: u16,
) -> bool {
    if completion_client(world) != Some(client)
        || entity >= 2046
        || !(1..=255).contains(&alias_index)
    {
        return false;
    }
    let mut runtime = world.resource_mut::<RoundScript>();
    let Some(wait) = runtime
        .entities
        .values_mut()
        .find(|row| row.number == i32::from(entity))
        .and_then(|row| row.sound_wait.as_mut())
        .filter(|wait| wait.alias_index == alias_index)
    else {
        return false;
    };
    wait.completed = true;
    true
}

pub(crate) fn advance(world: &mut World) {
    let completed: Vec<_> = {
        let Some(mut runtime) = world.get_resource_mut::<RoundScript>() else {
            return;
        };
        runtime
            .entities
            .iter_mut()
            .filter_map(|(id, entity)| {
                if entity.sound_wait.as_ref()?.completed {
                    Some((*id, entity.sound_wait.take().unwrap().notify))
                } else {
                    None
                }
            })
            .collect()
    };
    if completed.is_empty() {
        return;
    }
    for (id, notify) in completed {
        crate::script::runtime::raise(world, Value::Object(id), &notify, Vec::new());
    }
    let now = crate::level_time_ms(world.resource::<crate::step::StepRequest>().tick);
    crate::script::runtime::deliver_external(world, i64::from(now));
}
