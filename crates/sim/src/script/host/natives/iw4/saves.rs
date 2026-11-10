use super::*;
use crate::campaign_checkpoint::SaveMemory;
use crate::script::host::args::int;

fn campaign(world: &World) -> Result<(), String> {
    if world.resource::<RoundScript>().profile != asset_core::RuntimeProfile::Campaign {
        return Err("savegame natives require campaign mode".into());
    }
    Ok(())
}

pub(super) fn register(registry: &mut NativeRegistry) {
    use Namespace::Function;
    registry.register(Function, "missionfailed", |world, _, _| {
        crate::campaign_reload::request(world, crate::CampaignReloadCause::MissionFailed)?;
        Ok(Value::Undefined)
    });
    registry.register(Function, "issavesuccessful", |world, _, _| {
        campaign(world)?;
        Ok(Value::Int(
            world.resource::<SaveMemory>().successful().into(),
        ))
    });
    registry.register(Function, "issaverecentlyloaded", |world, _, _| {
        campaign(world)?;
        let now = world
            .resource::<RoundScript>()
            .last_tick
            .unwrap_or(crate::Tick(0));
        Ok(Value::Int(
            world.resource::<SaveMemory>().recently_loaded(now).into(),
        ))
    });
    registry.register(Function, "savegame", |world, _, args| {
        request(world, args, true)?;
        Ok(Value::Undefined)
    });
    registry.register(Function, "savegamenocommit", |world, _, args| {
        request(world, args, false).map(Value::Int)
    });
    registry.register(Function, "commitwouldbevalid", |world, _, args| {
        campaign(world)?;
        let id = int(args, 0)?;
        Ok(Value::Int(
            world
                .resource::<SaveMemory>()
                .commit_would_be_valid(id)
                .into(),
        ))
    });
    registry.register(Function, "commitsave", |world, _, args| {
        campaign(world)?;
        let id = int(args, 0)?;
        world.resource_mut::<SaveMemory>().commit(id)?;
        Ok(Value::Undefined)
    });
}

fn request(world: &mut World, args: &[Value], commit: bool) -> Result<i32, String> {
    campaign(world)?;
    let label = string(args, 0)?;
    let label = if label.is_empty() {
        "auto".into()
    } else {
        label
    };
    let description = match args.get(1) {
        None => String::new(),
        Some(Value::LocalizedString(text)) => text.to_string(),
        _ => return Err("parameter 2 is not a localized string".into()),
    };
    let image = if args.len() > 2 {
        string(args, 2)?
    } else {
        "$default".into()
    };
    let suppress_print = args.len() > 3 && int(args, 3)? != 0;
    if world
        .resource::<RoundScript>()
        .last_tick
        .is_none_or(|tick| tick.0 == 0)
    {
        return Err("cannot save before level time has advanced".into());
    }
    if world
        .resource::<crate::script::MatchScript>()
        .dvars
        .get("g_reloading")
        .is_some_and(|value| atoi(value) != 0)
    {
        return Err("cannot save while reloading the level".into());
    }
    world.resource_mut::<SaveMemory>().request(
        crate::CampaignSaveInfo {
            id: 0,
            label,
            description,
            image,
            suppress_print,
        },
        commit,
    )
}
