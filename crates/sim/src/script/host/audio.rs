pub(crate) mod waits;

use super::args::{float, int, optional, string};
use crate::script::{Namespace, NativeRegistry, RoundScript, Value};
use bevy_ecs::prelude::World;

fn change(world: &mut World, args: &[Value], stop: bool, ac130: bool) -> Result<Value, String> {
    let sp_ambient =
        !ac130 && world.resource::<RoundScript>().profile == asset_core::RuntimeProfile::Campaign;
    if if stop {
        args.len() > 1
    } else {
        !(1..=if sp_ambient { 3 } else { 2 }).contains(&args.len())
    } {
        return Err("wrong number of ambient arguments".into());
    }
    let alias = if stop {
        None
    } else {
        let name = string(args, 0)?;
        if name.is_empty() || name.len() > 1023 {
            return Err("ambient alias must be 1–1023 bytes".into());
        }
        Some(name)
    };
    let seconds = optional(args, usize::from(!stop), float)?.unwrap_or(0.0);
    let rate = if sp_ambient && !stop {
        optional(args, 2, float)?.unwrap_or(1.0)
    } else {
        1.0
    };
    if !rate.is_finite() {
        return Err("ambient playback rate must be finite".into());
    }
    let rate_percent = (rate * 100.0 + 0.5).floor().clamp(0.0, 100.0) as u8;
    let duration = (seconds * 1000.0 + 0.5).floor();
    if !duration.is_finite() || duration < 0.0 || f64::from(duration) > f64::from(i32::MAX) {
        return Err("ambient fade time must be finite, nonnegative and fit the level clock".into());
    }
    let now = world
        .get_resource::<crate::step::StepRequest>()
        .map_or(0, |request| crate::level_time_ms(request.tick));
    let end = now
        .checked_add(duration as i32)
        .ok_or("ambient fade exceeds the level clock")?;
    let plan = crate::ScriptAmbient {
        alias,
        start_ms: now,
        end_ms: end,
        rate_percent,
    };
    let mut runtime = world.resource_mut::<RoundScript>();
    if ac130 {
        runtime.engine.ac130_ambient = Some(plan);
    } else {
        runtime.engine.ambient = Some(plan);
    }
    Ok(Value::Undefined)
}

fn music_play(world: &mut World, args: &[Value]) -> Result<Value, String> {
    let alias = string(args, 0)?;
    if alias.len() > 1023 {
        return Err("music alias exceeds 1023 bytes".into());
    }
    crate::frame::FrameWorld::from_world(world)
        .push_script_audio(crate::ScriptAudioCommand::MusicPlay(alias));
    Ok(Value::Undefined)
}

fn music_stop(world: &mut World, args: &[Value]) -> Result<Value, String> {
    if args.len() > 1 {
        return Err("musicStop accepts an optional fade time".into());
    }
    let seconds = optional(args, 0, float)?.unwrap_or(0.0);
    let duration = (seconds * 1000.0 + 0.5).floor();
    if !duration.is_finite() || duration < 0.0 || f64::from(duration) > f64::from(i32::MAX) {
        return Err("music fade must be finite, nonnegative and fit the audio clock".into());
    }
    crate::frame::FrameWorld::from_world(world).push_script_audio(
        crate::ScriptAudioCommand::MusicStop {
            fade_ms: duration as i32,
        },
    );
    Ok(Value::Undefined)
}

fn sound_fade(world: &mut World, args: &[Value]) -> Result<Value, String> {
    let volume = float(args, 0)?;
    let seconds = optional(args, 1, float)?.unwrap_or(0.0);
    let duration = (seconds * 1000.0).trunc();
    if !volume.is_finite()
        || volume < 0.0
        || !duration.is_finite()
        || duration < 0.0
        || f64::from(duration) > f64::from(i32::MAX)
    {
        return Err("soundFade needs finite nonnegative volume and fade duration".into());
    }
    crate::frame::FrameWorld::from_world(world).push_script_audio(
        crate::ScriptAudioCommand::SoundFade {
            volume,
            fade_ms: duration as i32,
        },
    );
    Ok(Value::Undefined)
}

fn stop_entity_sounds(world: &mut World, receiver: &Value) -> Result<Value, String> {
    let id = super::natives::engine::entity_id(world, receiver)?;
    let number = world.resource::<RoundScript>().entities[&id].number;
    let origin = match super::players::entity_field(world, id, "origin") {
        Value::Vector(origin) => origin,
        _ => [0.0; 3],
    };
    let tick = world.resource::<crate::step::StepRequest>().tick;
    crate::frame::FrameWorld::from_world(world).push_entity_event(
        tick,
        crate::EventAudience::All,
        entity_iw4::EntityEventKind::STOPSOUNDS,
        crate::EntityEventPayload {
            number,
            origin,
            ..Default::default()
        },
    );
    Ok(Value::Undefined)
}

fn channel_volumes(
    world: &mut World,
    receiver: &Value,
    args: &[Value],
    deactivate: bool,
) -> Result<Value, String> {
    let client = crate::ClientId(super::natives::player::player(world, receiver)?);
    let count = if deactivate { 1..=2 } else { 2..=3 };
    if !count.contains(&args.len()) {
        return Err("wrong number of channel volume arguments".into());
    }
    let priority = match string(args, 0)?.as_str() {
        "snd_channelvolprio_holdbreath" => 1,
        "snd_channelvolprio_pain" => 2,
        "snd_channelvolprio_shellshock" => 3,
        _ => return Err("invalid channel volume priority".into()),
    };
    let seconds = optional(args, if deactivate { 1 } else { 2 }, float)?.unwrap_or(0.0);
    let duration = (seconds * 1000.0 + 0.5).floor().max(0.0);
    if !seconds.is_finite() || f64::from(duration) > f64::from(i32::MAX) {
        return Err("channel fade must be finite and fit the audio clock".into());
    }
    let command = if deactivate {
        crate::ScriptAudioCommand::DeactivateChannelVolumes {
            client,
            priority,
            fade_ms: duration as i32,
        }
    } else {
        let name = string(args, 1)?.to_ascii_lowercase();
        let precached = world
            .resource::<crate::script::MatchScript>()
            .precached
            .keys()
            .any(|(kind, item)| *kind == "shellshock" && item.eq_ignore_ascii_case(&name));
        let volumes = if precached {
            Some(
                crate::frame::FrameWorld::from_world(world)
                    .shock(&name)
                    .and_then(|shock| shock.sound.channel_volumes.clone())
                    .ok_or_else(|| format!("shellshock '{name}' has no channel volume profile"))?,
            )
        } else {
            None
        };
        crate::ScriptAudioCommand::ChannelVolumes {
            client,
            priority,
            volumes,
            fade_ms: duration as i32,
        }
    };
    if !command.valid() {
        return Err("invalid channel volume profile".into());
    }
    crate::frame::FrameWorld::from_world(world).push_script_audio(command);
    Ok(Value::Undefined)
}

fn reverb(
    world: &mut World,
    receiver: &Value,
    args: &[Value],
    deactivate: bool,
) -> Result<Value, String> {
    let client = crate::ClientId(super::natives::player::player(world, receiver)?);
    let count = if deactivate { 1..=2 } else { 2..=5 };
    if !count.contains(&args.len()) {
        return Err("wrong number of reverb arguments".into());
    }
    let priority = match string(args, 0)?.as_str() {
        "snd_enveffectsprio_level" => 1,
        "snd_enveffectsprio_shellshock" => 2,
        _ => return Err("invalid reverb priority".into()),
    };
    let seconds = optional(args, if deactivate { 1 } else { 4 }, float)?.unwrap_or(0.0);
    let duration = (seconds * 1000.0 + 0.5).floor().max(0.0);
    if !seconds.is_finite() || f64::from(duration) > f64::from(i32::MAX) {
        return Err("reverb fade must be finite and fit the audio clock".into());
    }
    let command = if deactivate {
        crate::ScriptAudioCommand::DeactivateReverb {
            client,
            priority,
            fade_ms: duration as i32,
        }
    } else {
        crate::ScriptAudioCommand::Reverb {
            client,
            priority,
            room: string(args, 1)?,
            dry: optional(args, 2, float)?.unwrap_or(1.0),
            wet: optional(args, 3, float)?.unwrap_or(0.5),
            fade_ms: duration as i32,
        }
    };
    if !command.valid() {
        return Err("reverb needs finite dry/wet levels and a bounded room name".into());
    }
    crate::frame::FrameWorld::from_world(world).push_script_audio(command);
    Ok(Value::Undefined)
}

fn eq_index(args: &[Value], index: usize, limit: i32) -> Result<u8, String> {
    let value = int(args, index)?;
    if !(0..limit).contains(&value) {
        return Err("EQ index out of range".into());
    }
    Ok(value as u8)
}

fn eq_band(args: &[Value], index: usize) -> Result<crate::ScriptEqBand, String> {
    let kind = match string(args, index)?.to_ascii_lowercase().as_str() {
        "lowpass" => 0,
        "highpass" => 1,
        "lowshelf" => 2,
        "highshelf" => 3,
        "bell" => 4,
        _ => return Err("unknown EQ filter type".into()),
    };
    let band = crate::ScriptEqBand {
        kind,
        gain_db: float(args, index + 1)?,
        frequency: float(args, index + 2)?,
        q: float(args, index + 3)?,
    };
    if !band.valid() {
        return Err("EQ needs finite gain, frequency in 0..20000 and positive Q".into());
    }
    Ok(band)
}

fn eq(world: &mut World, receiver: &Value, args: &[Value], operation: u8) -> Result<Value, String> {
    let client = crate::ClientId(super::natives::player::player(world, receiver)?);
    let mut commands = Vec::new();
    match operation {
        0 | 1 => {
            if (operation == 0 && args.len() != 7)
                || (operation == 1 && ![6, 10, 14].contains(&args.len()))
            {
                return Err("wrong number of EQ arguments".into());
            }
            let channel = string(args, 0)?;
            let track = eq_index(args, 1, 2)?;
            if operation == 0 {
                commands.push(crate::ScriptAudioCommand::Eq {
                    client,
                    channel,
                    track,
                    band: eq_index(args, 2, 3)?,
                    filter: eq_band(args, 3)?,
                });
            } else {
                let filters = (0..((args.len() - 2) / 4))
                    .map(|band| eq_band(args, 2 + band * 4))
                    .collect::<Result<Vec<_>, _>>()?;
                commands.push(crate::ScriptAudioCommand::EqBands {
                    client,
                    channel,
                    track,
                    filters,
                });
            }
        }
        2 => {
            if !(1..=3).contains(&args.len()) {
                return Err("DeactivateEq takes track, optional channel and band".into());
            }
            commands.push(crate::ScriptAudioCommand::DeactivateEq {
                client,
                track: eq_index(args, 0, 2)?,
                channel: optional(args, 1, string)?,
                band: if args.len() == 3 {
                    Some(eq_index(args, 2, 3)?)
                } else {
                    None
                },
            });
        }
        _ => {
            if !(1..=2).contains(&args.len()) {
                return Err("SetEqLerp takes fraction and optional track".into());
            }
            let fraction = float(args, 0)?;
            let track = if args.len() == 2 {
                eq_index(args, 1, 2)?
            } else {
                0
            };
            if !fraction.is_finite() || !(0.0..=1.0).contains(&fraction) {
                return Err("EQ fraction must be in 0..1".into());
            }
            commands.push(crate::ScriptAudioCommand::EqLerp {
                client,
                fraction: if track == 1 { 1.0 - fraction } else { fraction },
            });
        }
    }
    if commands.iter().any(|command| !command.valid()) {
        return Err("invalid EQ command".into());
    }
    let mut frame = crate::frame::FrameWorld::from_world(world);
    for command in commands {
        frame.push_script_audio(command);
    }
    Ok(Value::Undefined)
}

fn entity_eq(world: &mut World, receiver: &Value, disabled: bool) -> Result<Value, String> {
    let id = super::natives::engine::entity_id(world, receiver)?;
    let client = world.resource::<RoundScript>().player_client(id);
    if let Some(client) = client {
        let mut frame = crate::frame::FrameWorld::from_world(world);
        let ps = frame
            .player_mut(crate::ClientId(client))
            .ok_or("player has no physical state")?;
        if disabled {
            ps.e_flags |= playerstate_iw4::eflags::SP_EQ_DISABLED;
        } else {
            ps.e_flags &= !playerstate_iw4::eflags::SP_EQ_DISABLED;
        }
    } else {
        world
            .resource_mut::<RoundScript>()
            .entities
            .get_mut(&id)
            .unwrap()
            .eq_disabled = disabled;
    }
    Ok(Value::Undefined)
}

pub(crate) fn eq_disabled_entities(world: &World) -> Vec<u16> {
    let Some(runtime) = world.get_resource::<RoundScript>() else {
        return Vec::new();
    };
    runtime
        .entities
        .iter()
        .filter(|(id, entity)| {
            if let Some(client) = runtime.player_client(**id) {
                runtime.profile == asset_core::RuntimeProfile::Campaign
                    && crate::frame::player_ref(world, crate::ClientId(client))
                        .is_some_and(|ps| ps.e_flags & playerstate_iw4::eflags::SP_EQ_DISABLED != 0)
            } else {
                entity.eq_disabled
            }
        })
        .filter_map(|(_, entity)| {
            u16::try_from(entity.number)
                .ok()
                .filter(|number| *number < 2048)
        })
        .collect::<std::collections::BTreeSet<_>>()
        .into_iter()
        .collect()
}

pub(crate) fn register(registry: &mut NativeRegistry) {
    registry.register(
        Namespace::Method,
        "iswaitingonsound",
        |world, receiver, _| {
            let id = super::natives::engine::entity_id(world, receiver)?;
            Ok(Value::Int(i32::from(
                world.resource::<RoundScript>().entities[&id]
                    .sound_wait
                    .is_some(),
            )))
        },
    );
    registry.register(Namespace::Method, "eqoff", |world, receiver, _| {
        entity_eq(world, receiver, true)
    });
    registry.register(Namespace::Method, "eqon", |world, receiver, _| {
        entity_eq(world, receiver, false)
    });
    registry.register(Namespace::Method, "seteq", |world, receiver, args| {
        eq(world, receiver, args, 0)
    });
    registry.register(Namespace::Method, "seteqbands", |world, receiver, args| {
        eq(world, receiver, args, 1)
    });
    registry.register(
        Namespace::Method,
        "deactivateeq",
        |world, receiver, args| eq(world, receiver, args, 2),
    );
    registry.register(Namespace::Method, "seteqlerp", |world, receiver, args| {
        eq(world, receiver, args, 3)
    });
    registry.register(Namespace::Method, "setreverb", |world, receiver, args| {
        reverb(world, receiver, args, false)
    });
    registry.register(
        Namespace::Method,
        "deactivatereverb",
        |world, receiver, args| reverb(world, receiver, args, true),
    );
    registry.register(
        Namespace::Method,
        "setchannelvolumes",
        |world, receiver, args| channel_volumes(world, receiver, args, false),
    );
    registry.register(
        Namespace::Method,
        "deactivatechannelvolumes",
        |world, receiver, args| channel_volumes(world, receiver, args, true),
    );
    registry.register(Namespace::Method, "stopsounds", |world, receiver, _| {
        stop_entity_sounds(world, receiver)
    });
    registry.register(Namespace::Function, "soundfade", |world, _, args| {
        sound_fade(world, args)
    });
    registry.register(Namespace::Function, "musicplay", |world, _, args| {
        music_play(world, args)
    });
    registry.register(Namespace::Function, "musicstop", |world, _, args| {
        music_stop(world, args)
    });
    registry.register(Namespace::Function, "ambientplay", |world, _, args| {
        change(world, args, false, false)
    });
    registry.register(Namespace::Function, "ambientstop", |world, _, args| {
        change(world, args, true, false)
    });
    registry.register(Namespace::Function, "setac130ambience", |world, _, args| {
        change(world, args, false, true)
    });
}
