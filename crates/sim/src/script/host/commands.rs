use super::args::{byte_string, string};
use crate::script::{RoundScript, Value};
use bevy_ecs::prelude::World;
use std::sync::Arc;

#[derive(Clone, Debug)]
pub(crate) struct CommandNotify {
    client: Option<u32>,
    command: u32,
    notify: Arc<str>,
}

pub(crate) fn register(
    world: &mut World,
    client: Option<u32>,
    args: &[Value],
) -> Result<Value, String> {
    if args.len() != 2 {
        return Err("USAGE: notifyOnCommand(<notify>, <command>)".into());
    }
    let notify = byte_string(args, 0)?.symbol_key();
    let command = string(args, 1)?.to_ascii_lowercase();
    let Some(command) =
        input_iw4::command_id_from_name(&command).filter(|id| matches!(id, 1..=62 | 65..=77))
    else {
        return Ok(Value::Undefined);
    };
    let mut runtime = world.resource_mut::<RoundScript>();
    if runtime
        .command_notifies
        .iter()
        .any(|row| row.client == client && row.command == command && row.notify == notify)
    {
        return Ok(Value::Undefined);
    }
    if runtime.command_notifies.len() == 64 {
        return Err("too many command notifications (64)".into());
    }
    runtime.command_notifies.push(CommandNotify {
        client,
        command,
        notify,
    });
    Ok(Value::Undefined)
}

pub(crate) fn execute(world: &mut World, client: u32, command: u32) -> bool {
    if input_iw4::command_name(command).is_none() {
        return false;
    };
    let runtime = world.resource::<RoundScript>();
    let Some(player) = runtime.players.get(&client) else {
        return false;
    };
    let receiver = Value::Object(player.object);
    let notifies: Vec<_> = runtime
        .command_notifies
        .iter()
        .filter(|row| row.client.is_none_or(|owner| owner == client) && row.command == command)
        .map(|row| row.notify.clone())
        .collect();
    for notify in notifies {
        crate::script::runtime::raise(world, receiver.clone(), &notify, Vec::new());
    }
    true
}
