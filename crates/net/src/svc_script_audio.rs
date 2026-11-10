use crate::{ReliableEventHub, ReliableRow, WireError, WireReader, WireWriter};
use bevy::prelude::*;
use sim::{ClientId, ScriptAudioCommand};

#[derive(Message, Clone, Debug, PartialEq)]
pub struct SvcScriptAudio(pub ScriptAudioCommand);

pub(crate) fn fanout_script_audio(
    reliable: &mut ReliableEventHub,
    clients: impl IntoIterator<Item = ClientId>,
    commands: &[ScriptAudioCommand],
) {
    for client in clients {
        for command in commands {
            if command.target().is_some_and(|target| target != client) {
                continue;
            }
            reliable
                .queue_mut(client)
                .push(ReliableRow::ScriptAudio(command.clone()));
        }
    }
}

pub(crate) fn encode_script_audio(out: &mut WireWriter, command: &ScriptAudioCommand) {
    match command {
        ScriptAudioCommand::EntitySound {
            entity,
            alias_index,
            alias,
            origin,
            completion_client,
            master,
        } => {
            out.put_u8(11);
            out.put_u16(*entity);
            out.put_u16(*alias_index);
            out.put_u8(u8::from(*master));
            out.put_u32(completion_client.0);
            out.put_u16(alias.len() as u16);
            out.put_bytes(alias.as_bytes());
            for value in origin {
                out.put_f32(*value);
            }
        }
        ScriptAudioCommand::EqBands {
            client,
            channel,
            track,
            filters,
        } => {
            out.put_u8(10);
            out.put_u32(client.0);
            out.put_u8(*track);
            out.put_u8(channel.len() as u8);
            out.put_bytes(channel.as_bytes());
            out.put_u8(filters.len() as u8);
            for filter in filters {
                out.put_u8(filter.kind);
                out.put_f32(filter.gain_db);
                out.put_f32(filter.frequency);
                out.put_f32(filter.q);
            }
        }
        ScriptAudioCommand::Eq {
            client,
            channel,
            track,
            band,
            filter,
        } => {
            out.put_u8(7);
            out.put_u32(client.0);
            out.put_u8(*track);
            out.put_u8(*band);
            out.put_u8(channel.len() as u8);
            out.put_bytes(channel.as_bytes());
            out.put_u8(filter.kind);
            out.put_f32(filter.gain_db);
            out.put_f32(filter.frequency);
            out.put_f32(filter.q);
        }
        ScriptAudioCommand::DeactivateEq {
            client,
            track,
            channel,
            band,
        } => {
            out.put_u8(8);
            out.put_u32(client.0);
            out.put_u8(*track);
            out.put_u8(u8::from(channel.is_some()));
            if let Some(channel) = channel {
                out.put_u8(channel.len() as u8);
                out.put_bytes(channel.as_bytes());
            }
            out.put_u8(band.unwrap_or(255));
        }
        ScriptAudioCommand::EqLerp { client, fraction } => {
            out.put_u8(9);
            out.put_u32(client.0);
            out.put_f32(*fraction);
        }

        ScriptAudioCommand::Reverb {
            client,
            priority,
            room,
            dry,
            wet,
            fade_ms,
        } => {
            out.put_u8(5);
            out.put_u32(client.0);
            out.put_u8(*priority);
            out.put_u16(room.len() as u16);
            out.put_bytes(room.as_bytes());
            out.put_f32(*dry);
            out.put_f32(*wet);
            out.put_i32(*fade_ms);
        }
        ScriptAudioCommand::DeactivateReverb {
            client,
            priority,
            fade_ms,
        } => {
            out.put_u8(6);
            out.put_u32(client.0);
            out.put_u8(*priority);
            out.put_i32(*fade_ms);
        }
        ScriptAudioCommand::ChannelVolumes {
            client,
            priority,
            volumes,
            fade_ms,
        } => {
            out.put_u8(3);
            out.put_u32(client.0);
            out.put_u8(*priority);
            out.put_i32(*fade_ms);
            match volumes {
                None => out.put_u8(0),
                Some(volumes) => {
                    out.put_u8(volumes.len() as u8);
                    for (name, gain) in volumes {
                        out.put_u8(name.len() as u8);
                        out.put_bytes(name.as_bytes());
                        out.put_f32(*gain);
                    }
                }
            }
        }
        ScriptAudioCommand::DeactivateChannelVolumes {
            client,
            priority,
            fade_ms,
        } => {
            out.put_u8(4);
            out.put_u32(client.0);
            out.put_u8(*priority);
            out.put_i32(*fade_ms);
        }
        ScriptAudioCommand::MusicPlay(alias) => {
            out.put_u8(0);
            out.put_u16(alias.len() as u16);
            out.put_bytes(alias.as_bytes());
        }
        ScriptAudioCommand::SoundFade { volume, fade_ms } => {
            out.put_u8(2);
            out.put_f32(*volume);
            out.put_i32(*fade_ms);
        }
        ScriptAudioCommand::MusicStop { fade_ms } => {
            out.put_u8(1);
            out.put_i32(*fade_ms);
        }
    }
}

pub(crate) fn decode_script_audio(
    input: &mut WireReader<'_>,
) -> Result<ScriptAudioCommand, WireError> {
    fn channel(input: &mut WireReader<'_>) -> Result<String, WireError> {
        let len = input.get_u8()? as usize;
        if len > 64 {
            return Err(WireError::Malformed("EQ channel exceeds limit"));
        }
        let mut bytes = vec![0; len];
        input.get_bytes(&mut bytes)?;
        String::from_utf8(bytes).map_err(|_| WireError::Malformed("EQ channel is not UTF-8"))
    }
    let command = match input.get_u8()? {
        11 => {
            let entity = input.get_u16()?;
            let alias_index = input.get_u16()?;
            let master = match input.get_u8()? {
                0 => false,
                1 => true,
                _ => return Err(WireError::Malformed("invalid entity sound master flag")),
            };
            let completion_client = ClientId(input.get_u32()?);
            let len = input.get_u16()? as usize;
            if len > 1023 {
                return Err(WireError::Malformed("entity sound alias exceeds limit"));
            }
            let mut bytes = vec![0; len];
            input.get_bytes(&mut bytes)?;
            let alias = String::from_utf8(bytes)
                .map_err(|_| WireError::Malformed("entity sound alias is not UTF-8"))?;
            let origin = [input.get_f32()?, input.get_f32()?, input.get_f32()?];
            ScriptAudioCommand::EntitySound {
                entity,
                alias_index,
                alias,
                origin,
                completion_client,
                master,
            }
        }
        10 => {
            let client = ClientId(input.get_u32()?);
            let track = input.get_u8()?;
            let channel = channel(input)?;
            let count = input.get_u8()?;
            if !(1..=3).contains(&count) {
                return Err(WireError::Malformed("invalid EQ band count"));
            }
            let mut filters = Vec::new();
            for _ in 0..count {
                filters.push(sim::ScriptEqBand {
                    kind: input.get_u8()?,
                    gain_db: input.get_f32()?,
                    frequency: input.get_f32()?,
                    q: input.get_f32()?,
                });
            }
            ScriptAudioCommand::EqBands {
                client,
                channel,
                track,
                filters,
            }
        }
        7 => ScriptAudioCommand::Eq {
            client: ClientId(input.get_u32()?),
            track: input.get_u8()?,
            band: input.get_u8()?,
            channel: channel(input)?,
            filter: sim::ScriptEqBand {
                kind: input.get_u8()?,
                gain_db: input.get_f32()?,
                frequency: input.get_f32()?,
                q: input.get_f32()?,
            },
        },
        8 => {
            let client = ClientId(input.get_u32()?);
            let track = input.get_u8()?;
            let channel = match input.get_u8()? {
                0 => None,
                1 => Some(channel(input)?),
                _ => return Err(WireError::Malformed("invalid EQ channel tag")),
            };
            let band = input.get_u8()?;
            ScriptAudioCommand::DeactivateEq {
                client,
                track,
                channel,
                band: (band != 255).then_some(band),
            }
        }
        9 => ScriptAudioCommand::EqLerp {
            client: ClientId(input.get_u32()?),
            fraction: input.get_f32()?,
        },
        5 => {
            let client = ClientId(input.get_u32()?);
            let priority = input.get_u8()?;
            let len = input.get_u16()? as usize;
            if len > 1023 {
                return Err(WireError::Malformed("room name exceeds limit"));
            }
            let mut bytes = vec![0; len];
            input.get_bytes(&mut bytes)?;
            let room = String::from_utf8(bytes)
                .map_err(|_| WireError::Malformed("room name is not UTF-8"))?;
            ScriptAudioCommand::Reverb {
                client,
                priority,
                room,
                dry: input.get_f32()?,
                wet: input.get_f32()?,
                fade_ms: input.get_i32()?,
            }
        }
        6 => ScriptAudioCommand::DeactivateReverb {
            client: ClientId(input.get_u32()?),
            priority: input.get_u8()?,
            fade_ms: input.get_i32()?,
        },
        3 => {
            let client = ClientId(input.get_u32()?);
            let priority = input.get_u8()?;
            let fade_ms = input.get_i32()?;
            let count = input.get_u8()?;
            if count > 64 {
                return Err(WireError::Malformed("too many channel gains"));
            }
            let mut volumes = std::collections::BTreeMap::new();
            for _ in 0..count {
                let len = input.get_u8()? as usize;
                if len > 64 {
                    return Err(WireError::Malformed("channel name exceeds limit"));
                }
                let mut bytes = vec![0; len];
                input.get_bytes(&mut bytes)?;
                let name = String::from_utf8(bytes)
                    .map_err(|_| WireError::Malformed("channel name is not UTF-8"))?;
                let gain = input.get_f32()?;
                if volumes.insert(name, gain).is_some() {
                    return Err(WireError::Malformed("duplicate channel gain"));
                }
            }
            ScriptAudioCommand::ChannelVolumes {
                client,
                priority,
                volumes: (count > 0).then_some(volumes),
                fade_ms,
            }
        }
        4 => ScriptAudioCommand::DeactivateChannelVolumes {
            client: ClientId(input.get_u32()?),
            priority: input.get_u8()?,
            fade_ms: input.get_i32()?,
        },
        0 => {
            let len = input.get_u16()? as usize;
            if len > 1023 {
                return Err(WireError::Malformed("music alias exceeds limit"));
            }
            let mut bytes = vec![0; len];
            input.get_bytes(&mut bytes)?;
            ScriptAudioCommand::MusicPlay(
                String::from_utf8(bytes)
                    .map_err(|_| WireError::Malformed("music alias is not UTF-8"))?,
            )
        }
        2 => ScriptAudioCommand::SoundFade {
            volume: input.get_f32()?,
            fade_ms: input.get_i32()?,
        },
        1 => ScriptAudioCommand::MusicStop {
            fade_ms: input.get_i32()?,
        },
        _ => return Err(WireError::Malformed("unknown script audio command")),
    };
    if !command.valid() {
        return Err(WireError::Malformed("invalid script audio command"));
    }
    Ok(command)
}
