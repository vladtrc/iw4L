use crate::script::{MatchScript, RoundScript};
use bevy_ecs::prelude::{Resource, World};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CampaignReloadCause {
    Death,
    MissionFailed,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CampaignReloadRequest {
    pub cause: CampaignReloadCause,
    pub delay_ms: u32,
}

#[derive(Resource, Default)]
pub(crate) struct CampaignReload {
    pub(crate) request: Option<CampaignReloadRequest>,
}

pub(crate) fn reset(world: &mut World) {
    world.insert_resource(CampaignReload::default());
    world
        .resource_mut::<MatchScript>()
        .dvars
        .insert("g_reloading".into(), "0".into());
}

pub(crate) fn request(world: &mut World, cause: CampaignReloadCause) -> Result<(), String> {
    if world.resource::<RoundScript>().profile != asset_core::RuntimeProfile::Campaign {
        return Err("campaign reload requires campaign mode".into());
    }
    let dvars = &world.resource::<MatchScript>().dvars;
    let value = |name: &str| {
        dvars
            .get(name)
            .map_or(0, |v| crate::script::host::natives::iw4::atoi(v))
    };
    if value("g_reloading") != 0
        || value("g_specialops") != 0
        || value("g_arcademode") != 0 && value("g_arcademode_lives") < 0
    {
        return Ok(());
    }
    let delay_ms = value("g_deathdelay").max(0) as u32;
    let tick = world
        .resource::<RoundScript>()
        .last_tick
        .unwrap_or(crate::Tick(0));
    world.resource_mut::<CampaignReload>().request =
        Some(CampaignReloadRequest { cause, delay_ms });
    world
        .resource_mut::<MatchScript>()
        .dvars
        .insert("g_reloading".into(), "1".into());
    world.resource_mut::<RoundScript>().engine.slow_motion = Some(crate::ScriptSlowMotion {
        from: 1.0,
        to: 0.1,
        start_ms: crate::level_time_ms(tick),
        duration_ms: delay_ms as i32,
    });
    Ok(())
}

pub(crate) fn check_death(world: &mut World, tick: crate::Tick) {
    if !world.contains_resource::<CampaignReload>() {
        return;
    }
    let frame = crate::frame::FrameWorld::from_world(world);
    let expired = frame.client_meta(crate::ClientId(0)).is_some_and(|meta| {
        meta.lifecycle == crate::ClientLifecycle::Dead
            && meta.dead_since_tick.is_some_and(|died| {
                tick.0
                    .saturating_sub(died)
                    .saturating_mul(crate::MATCH_TICK_MS)
                    > 1700
            })
    });
    if expired {
        request(world, CampaignReloadCause::Death)
            .expect("campaign reload owner only exists in campaign");
    }
}
