use super::super::args::{byte_string, int, string};
use crate::frame::FrameWorld;
use crate::script::{RoundScript, Value};
use bevy_ecs::prelude::World;
use std::sync::Arc;

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct SoundWait {
    notify: Arc<str>,
    request_id: u64,
    alias: String,
    origin: [f32; 3],
    master: bool,
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
    let frame = FrameWorld::from_world(world);
    if frame.script_sound_is_looping(&alias)? {
        return Err("cannot play a looping alias as a one-shot sound".into());
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
        let alias_index = u16::from(FrameWorld::from_world(world).sound_alias_index(&alias));
        if alias_index == 0 {
            return Err("sound alias configstring table is full".into());
        }
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
    let request_id = world
        .resource::<RoundScript>()
        .restart
        .as_ref()
        .unwrap()
        .next_presentation_request()?;
    let command = crate::ScriptAudioCommand::EntitySound {
        entity,
        request_id,
        alias: alias.clone(),
        origin,
        completion_client,
        master,
    };
    if !command.valid() {
        return Err("invalid entity sound command".into());
    }
    // Refused replacement still publishes playback and raises the prior notification.
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
        request_id,
        alias,
        origin,
        master,
        replace_allowed,
        completed: false,
    });
    Ok(Value::Undefined)
}

pub(crate) fn restore_pending(
    round: &mut RoundScript,
) -> Result<Vec<crate::ScriptAudioCommand>, String> {
    let mut commands = Vec::new();
    for row in round.entities.values_mut() {
        let Some(wait) = row.sound_wait.as_mut().filter(|wait| !wait.completed) else {
            continue;
        };
        let plan = round
            .restart
            .as_ref()
            .ok_or("sound restore requires a map session")?;
        let completion_client = plan
            .campaign_player
            .ok_or("sound restore requires a campaign audio owner")?;
        let entity = u16::try_from(row.number)
            .ok()
            .filter(|number| *number < 2046)
            .ok_or("invalid restored sound entity number")?;
        wait.request_id = plan.next_presentation_request()?;
        commands.push(crate::ScriptAudioCommand::EntitySound {
            entity,
            request_id: wait.request_id,
            alias: wait.alias.clone(),
            origin: wait.origin,
            completion_client,
            master: wait.master,
        });
    }
    Ok(commands)
}

pub(crate) fn acknowledge(world: &mut World, client: crate::ClientId, request_id: u64) -> bool {
    if completion_client(world) != Some(client) || request_id == 0 {
        return false;
    }
    let mut runtime = world.resource_mut::<RoundScript>();
    let Some(wait) = runtime
        .entities
        .values_mut()
        .find(|row| {
            row.sound_wait
                .as_ref()
                .is_some_and(|wait| wait.request_id == request_id)
        })
        .and_then(|row| row.sound_wait.as_mut())
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
