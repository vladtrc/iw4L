use super::args::{float, optional, string};
use super::natives::player::player;
use crate::frame::FrameWorld;
use crate::script::Namespace::{Function, Method};
use crate::script::{NativeRegistry, Runtime, Value};
use crate::world::ClientId;
use bevy_ecs::prelude::World;

pub(crate) fn register(registry: &mut NativeRegistry) {
    registry.register(Method, "playlocalsound", |world, receiver, args| {
        local_sound(world, receiver, args, false)
    });
    registry.register(Method, "stoplocalsound", |world, receiver, args| {
        local_sound(world, receiver, args, true)
    });

    registry.register(Method, "setdepthoffield", |world, receiver, args| {
        let client = player(world, receiver)?;
        let dof = depth_of_field(args)?;
        edit_view(world, client, |view| view.depth_of_field = dof);
        Ok(Value::Undefined)
    });

    macro_rules! vision_channel {
        ($global:literal, $player:literal, $field:ident) => {
            registry.register(Function, $global, |world, _, args| {
                let vision = vision_change(world, args)?;
                let mut frame = FrameWorld::from_world(world);
                for client in frame.client_ids_sorted() {
                    frame.client_meta_mut(client).view_effects.$field = None;
                }
                world.resource_mut::<Runtime>().engine.$field = Some(vision);
                Ok(Value::Undefined)
            });
            registry.register(Method, $player, |world, receiver, args| {
                let client = player(world, receiver)?;
                let vision = vision_change(world, args)?;
                edit_view(world, client, |view| view.$field = Some(vision));
                Ok(Value::Undefined)
            });
        };
    }
    vision_channel!("visionsetnaked", "visionsetnakedforplayer", naked_vision);
    // Singleplayer scripts set a player's own vision set without the suffix.
    registry.register(Method, "visionsetnaked", |world, receiver, args| {
        let client = player(world, receiver)?;
        let vision = vision_change(world, args)?;
        edit_view(world, client, |view| view.naked_vision = Some(vision));
        Ok(Value::Undefined)
    });
    // The player's own vision set wins; before any is set it is the map's.
    registry.register(Method, "getvisionsetnaked", |world, receiver, _| {
        let client = player(world, receiver)?;
        let own = FrameWorld::from_world(world)
            .client_meta(ClientId(client))
            .and_then(|meta| {
                meta.view_effects
                    .naked_vision
                    .as_ref()
                    .map(|v| v.name.clone())
            });
        let mut runtime = world.resource_mut::<Runtime>();
        let name = own
            .or_else(|| runtime.engine.naked_vision.as_ref().map(|v| v.name.clone()))
            .map(|name| Value::string(&name))
            .unwrap_or_else(|| runtime.object_field(0, "script"));
        Ok(name)
    });
    vision_channel!(
        "visionsetthermal",
        "visionsetthermalforplayer",
        thermal_vision
    );
    vision_channel!(
        "visionsetmissilecam",
        "visionsetmissilecamforplayer",
        missile_vision
    );
    vision_channel!("visionsetnight", "visionsetnightforplayer", night_vision);
    vision_channel!("visionsetpain", "visionsetpainforplayer", pain_vision);
    registry.register(Method, "setblurforplayer", |world, receiver, args| {
        let client = player(world, receiver)?;
        let to = float(args, 0)?;
        let seconds = optional(args, 1, float)?.unwrap_or(0.0);
        if args.len() > 2 || !to.is_finite() || to < 0.0 || !seconds.is_finite() || seconds < 0.0 {
            return Err("setBlurForPlayer requires a nonnegative blur and transition time".into());
        }
        let now = crate::level_time_ms(world.resource::<crate::step::StepRequest>().tick);
        edit_view(world, client, |view| {
            view.blur = Some(crate::ScriptBlur {
                from: view.blur.map_or(0.0, |blur| blur.sample(now)),
                to,
                set_ms: now,
                duration_ms: (seconds * 1000.0).round() as i32,
            });
        });
        Ok(Value::Undefined)
    });
    super::rumble::register(registry);
}

fn local_sound(
    world: &mut World,
    receiver: &Value,
    args: &[Value],
    stop: bool,
) -> Result<Value, String> {
    let client = player(world, receiver)?;
    let alias = string(args, 0)?;
    let mut frame = FrameWorld::from_world(world);
    let alias_index = frame.sound_alias_index(&alias);
    frame.push_local_sound(crate::PendingLocalSound {
        recipient: ClientId(client),
        stop,
        alias_index,
    });
    Ok(Value::Undefined)
}

fn vision_change(world: &World, args: &[Value]) -> Result<crate::VisionChange, String> {
    if args.len() > 2 {
        return Err("USAGE: VisionSetNaked( <visionset name>, <transition time> )".into());
    }
    let name = string(args, 0)?;
    let seconds = optional(args, 1, float)?.unwrap_or(1.0);
    Ok(crate::VisionChange {
        name,
        duration_ms: (seconds * 1000.0).round() as i32,
        set_ms: crate::level_time_ms(world.resource::<crate::step::StepRequest>().tick),
    })
}

fn depth_of_field(args: &[Value]) -> Result<crate::ScriptDepthOfField, String> {
    if args.len() != 6 {
        return Err("Incorrect number of parameters".into());
    }
    let [
        near_start,
        near_end,
        far_start,
        far_end,
        near_blur,
        far_blur,
    ] = [0, 1, 2, 3, 4, 5].map(|i| float(args, i));
    let (near_start, near_end, far_start, far_end) = (near_start?, near_end?, far_start?, far_end?);
    let (near_blur, far_blur) = (near_blur?, far_blur?);
    for (value, what) in [
        (near_start, "near start"),
        (near_end, "near end"),
        (far_start, "far start"),
        (far_end, "far end"),
    ] {
        if value < 0.0 {
            return Err(format!("{what} must be >= 0"));
        }
    }
    if !(4.0..=10.0).contains(&near_blur) {
        return Err("near blur should be between 4 and 10".into());
    }
    if far_blur < 0.0 || near_blur < far_blur {
        return Err("far blur should be >= 0 and <= near blur".into());
    }
    let (near_start, near_end) = if near_end <= near_start {
        (0.0, 0.0)
    } else {
        (near_start, near_end)
    };
    let (far_start, far_end) = if far_end <= far_start || far_blur == 0.0 {
        (0.0, 0.0)
    } else {
        (far_start, far_end)
    };
    Ok(crate::ScriptDepthOfField {
        near_start,
        near_end,
        far_start,
        far_end,
        near_blur,
        far_blur,
    })
}

fn edit_view(world: &mut World, client: u32, edit: impl FnOnce(&mut crate::ViewEffects)) {
    let mut frame = FrameWorld::from_world(world);
    if frame.client_meta(ClientId(client)).is_some() {
        edit(&mut frame.client_meta_mut(ClientId(client)).view_effects);
    }
}
