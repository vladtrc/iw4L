use super::args::{float, optional, string};
use crate::script::{Namespace, NativeRegistry, RoundScript, Value};
use bevy_ecs::prelude::World;

fn change(world: &mut World, args: &[Value], stop: bool, ac130: bool) -> Result<Value, String> {
    if if stop {
        args.len() > 1
    } else {
        !(1..=3).contains(&args.len())
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
    let duration = (seconds * 1000.0 + 0.5).floor();
    if !duration.is_finite() || duration < 0.0 || f64::from(duration) > f64::from(i32::MAX) {
        return Err("ambient fade time must be finite, nonnegative and fit the level clock".into());
    }
    let now = crate::level_time_ms(world.resource::<crate::step::StepRequest>().tick);
    let end = now
        .checked_add(duration as i32)
        .ok_or("ambient fade exceeds the level clock")?;
    let plan = crate::ScriptAmbient {
        alias,
        start_ms: now,
        end_ms: end,
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

fn stop_entity_sound_channel(
    world: &mut World,
    receiver: &Value,
    args: &[Value],
) -> Result<Value, String> {
    let _channel = optional(args, 0, string)?;
    stop_entity_sounds(world, receiver)
}

fn set_reverb(world: &mut World, receiver: &Value, args: &[Value]) -> Result<Value, String> {
    let _entity = super::natives::engine::entity_id(world, receiver)?;
    let _priority = string(args, 0)?;
    let _roomtype = string(args, 1)?;
    let _drylevel = float(args, 2)?;
    let _wetlevel = float(args, 3)?;
    let _fadetime = optional(args, 4, float)?.unwrap_or(0.0);
    Ok(Value::Undefined)
}

fn deactivate_reverb(world: &mut World, receiver: &Value, args: &[Value]) -> Result<Value, String> {
    let _entity = super::natives::engine::entity_id(world, receiver)?;
    let _priority = string(args, 0)?;
    let _fadetime = optional(args, 1, float)?.unwrap_or(0.0);
    Ok(Value::Undefined)
}

fn set_eq(world: &mut World, receiver: &Value, args: &[Value]) -> Result<Value, String> {
    let _entity = super::natives::engine::entity_id(world, receiver)?;
    if args.is_empty() {
        return Err("seteq expects arguments".into());
    }
    Ok(Value::Undefined)
}

fn set_eq_bands(world: &mut World, receiver: &Value, args: &[Value]) -> Result<Value, String> {
    let _entity = super::natives::engine::entity_id(world, receiver)?;
    if args.is_empty() {
        return Err("seteqbands expects arguments".into());
    }
    Ok(Value::Undefined)
}

fn deactivate_eq(world: &mut World, receiver: &Value, args: &[Value]) -> Result<Value, String> {
    let _entity = super::natives::engine::entity_id(world, receiver)?;
    if args.is_empty() {
        return Err("deactivateeq expects arguments".into());
    }
    Ok(Value::Undefined)
}

fn set_eq_lerp(world: &mut World, receiver: &Value, args: &[Value]) -> Result<Value, String> {
    let _entity = super::natives::engine::entity_id(world, receiver)?;
    let _progress = float(args, 0)?;
    Ok(Value::Undefined)
}

fn eq_toggle(world: &mut World, receiver: &Value, on: bool) -> Result<Value, String> {
    let id = super::natives::engine::entity_id(world, receiver)?;
    world
        .resource_mut::<RoundScript>()
        .set_object_field(id, "eq_active", Value::Int(on.into()));
    Ok(Value::Undefined)
}

pub(crate) fn register(registry: &mut NativeRegistry) {
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
    registry.register(
        Namespace::Method,
        "stopsoundchannel",
        |world, receiver, args| stop_entity_sound_channel(world, receiver, args),
    );
    registry.register(Namespace::Method, "setreverb", set_reverb);
    registry.register(Namespace::Method, "deactivatereverb", deactivate_reverb);
    registry.register(Namespace::Method, "seteq", set_eq);
    registry.register(Namespace::Method, "seteqbands", set_eq_bands);
    registry.register(Namespace::Method, "deactivateeq", deactivate_eq);
    registry.register(Namespace::Method, "seteqlerp", set_eq_lerp);
    registry.register(Namespace::Method, "eqon", |world, receiver, _| {
        eq_toggle(world, receiver, true)
    });
    registry.register(Namespace::Method, "eqoff", |world, receiver, _| {
        eq_toggle(world, receiver, false)
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
