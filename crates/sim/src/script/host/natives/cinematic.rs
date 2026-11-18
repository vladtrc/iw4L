use crate::ScriptCinematic;
use crate::script::host::args::string;
use crate::script::{Namespace, NativeRegistry, RoundScript, Value};
use bevy_ecs::prelude::World;

fn play(world: &mut World, args: &[Value], looped: bool) -> Result<Value, String> {
    if args.len() != 1 {
        return Err("cinematic expects one video name".into());
    }
    let name = string(args, 0)?.to_ascii_lowercase();
    if name.is_empty()
        || name.len() > 255
        || name == "."
        || name == ".."
        || name.contains(['/', '\\', ':'])
    {
        return Err("cinematic requires a video stem".into());
    }
    let fullscreen = world
        .resource::<crate::script::MatchScript>()
        .dvars
        .get("cg_cinematicfullscreen")
        .is_none_or(|v| v != "0");
    let mut runtime = world.resource_mut::<RoundScript>();
    let plan = runtime
        .restart
        .as_ref()
        .ok_or("cinematic requires an installed level")?;
    let completion_client = plan
        .campaign_player
        .ok_or("script cinematic requires a campaign presentation owner")?;
    runtime.engine.cinematic = Some(ScriptCinematic {
        request_id: plan.next_presentation_request()?,
        completion_client,
        name,
        looped,
        fullscreen,
    });
    Ok(Value::Undefined)
}

pub(crate) fn acknowledge(world: &mut World, client: crate::ClientId, request_id: u64) -> bool {
    let mut runtime = world.resource_mut::<RoundScript>();
    if request_id == 0
        || runtime
            .restart
            .as_ref()
            .and_then(|plan| plan.campaign_player)
            != Some(client)
        || !runtime
            .engine
            .cinematic
            .as_ref()
            .is_some_and(|request| request.request_id == request_id)
    {
        return false;
    }
    runtime.engine.cinematic = None;
    true
}

pub(crate) fn register(registry: &mut NativeRegistry) {
    use Namespace::Function;
    for name in ["cinematicingame", "cinematicingamesync"] {
        registry.register(Function, name, |world, _, args| play(world, args, false));
    }
    for name in ["cinematicingameloop", "cinematicingameloopresident"] {
        registry.register(Function, name, |world, _, args| play(world, args, true));
    }
    registry.register(Function, "stopcinematicingame", |world, _, args| {
        if !args.is_empty() {
            return Err("stopcinematicingame expects no arguments".into());
        }
        world.resource_mut::<RoundScript>().engine.cinematic = None;
        Ok(Value::Undefined)
    });
    registry.register(Function, "iscinematicplaying", |world, _, args| {
        if !args.is_empty() {
            return Err("iscinematicplaying expects no arguments".into());
        }
        Ok(Value::Int(
            world
                .resource::<RoundScript>()
                .engine
                .cinematic
                .is_some()
                .into(),
        ))
    });
}
