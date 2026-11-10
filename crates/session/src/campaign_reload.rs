use bevy::prelude::*;
use frame::ScopeApp;
use std::time::Duration;

#[derive(Resource, Default)]
struct ReloadCountdown(Option<(sim::CampaignReloadRequest, Duration)>);

fn reload_campaign(
    role: Res<frame::RuntimeRole>,
    time: Res<Time<Real>>,
    installed: Res<frame::InstalledMatch>,
    authority: Option<Res<net::AuthorityWorld>>,
    bridge: Option<Res<net::MasterBridge>>,
    mut countdown: ResMut<ReloadCountdown>,
    mut swap: ResMut<crate::SessionSwapRequest>,
) {
    if *role != frame::RuntimeRole::Listen
        || bridge.is_some_and(|bridge| bridge.state().in_match())
        || swap.is_pending()
    {
        return;
    }
    let Some(authority) = authority else {
        return;
    };
    let Some(request) = authority.0.campaign_reload_request() else {
        countdown.0 = None;
        return;
    };
    let now = time.elapsed();
    let (started_request, started) = countdown.0.get_or_insert((request, now));
    if *started_request != request {
        *started_request = request;
        *started = now;
    }
    if now.saturating_sub(*started) <= Duration::from_millis(u64::from(request.delay_ms)) {
        return;
    }
    let checkpoint = authority.0.committed_campaign_checkpoint().is_some();
    let result = if checkpoint {
        authority
            .0
            .prepare_campaign_checkpoint_load()
            .and_then(|load| swap.request_campaign_checkpoint(installed.zone.clone(), load))
    } else {
        swap.request_zone(installed.zone.clone())
    };
    match result {
        Ok(()) => diag::info!(
            Sim,
            "campaign reload: {:?} after {} real ms, checkpoint={checkpoint}",
            request.cause,
            request.delay_ms
        ),
        Err(error) => diag::warn!(Sim, "campaign reload refused: {error}"),
    }
}

pub(crate) fn register(app: &mut App) {
    app.scoped::<ReloadCountdown>(frame::MatchScope::Live)
        .add_systems(
            PreUpdate,
            reload_campaign
                .in_set(frame::InMatch)
                .in_set(net::ClientSet::Load)
                .before(frame::SessionSwapApplied),
        );
}
