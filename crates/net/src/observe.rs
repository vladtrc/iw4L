use bevy::prelude::*;

use crate::CgFrameClock;
use crate::ClientSet;
use crate::authority::inbox::ClientCommandInbox;
use crate::authority::runtime::{AuthorityWorld, ListenFanoutCensus};
use crate::client::centity_runtime::CEntityRuntime;
use crate::client::entities::CEntity;
use crate::client::presented::{LocalPresentClient, PresentLocalCensus, PresentedSnapshot};
use crate::client::runtime::{ClientClock, ClientPredictionState, LastAdoptedSnapshot};
use sim::ClientLifecycle;

#[derive(Resource, Default)]
struct IngressQueueCensus(Option<i64>);

pub(crate) fn register(app: &mut App) {
    app.init_resource::<IngressQueueCensus>().add_systems(
        Update,
        (
            latch_ingress_queue
                .after(ClientSet::Receive)
                .before(ClientSet::Input),
            (emit_feel, emit_remote).in_set(ClientSet::Diag),
        ),
    );
}

fn latch_ingress_queue(
    inbox: Option<Res<ClientCommandInbox>>,
    local: Res<LocalPresentClient>,
    mut census: ResMut<IngressQueueCensus>,
) {
    census.0 = inbox.map(|i| i.unique_queue_depth(local.0) as i64);
}

fn emit_feel(
    clock: Option<Res<CgFrameClock>>,
    presented: Res<PresentedSnapshot>,
    local: Res<LocalPresentClient>,
    client_clock: Option<Res<ClientClock>>,
    queue: Res<IngressQueueCensus>,
    last_adopted: Option<Res<LastAdoptedSnapshot>>,
    prediction: Option<Res<ClientPredictionState>>,
    authority_world: Option<Res<AuthorityWorld>>,
    fanout: Option<Res<ListenFanoutCensus>>,
    present_census: Option<Res<PresentLocalCensus>>,
) {
    let Some(clock) = clock else {
        return;
    };
    let auth_ps = authority_world.as_ref().and_then(|w| w.0.player(local.0));
    let adopted_ps = last_adopted.as_ref().and_then(|a| {
        a.next().and_then(|snap| {
            snap.players
                .iter()
                .find(|(id, _)| *id == local.0)
                .map(|(_, ps)| ps)
        })
    });
    let pred_ps = prediction.as_ref().and_then(|p| p.0.predicted_local());
    let lifecycle = presented
        .snapshot()
        .and_then(|s| s.meta.for_client(local.0))
        .map(|m| lifecycle_label(m.lifecycle));
    perf::feel(
        clock.time(),
        lifecycle,
        fanout.as_ref().and_then(|f| f.seat_applied),
        fanout.as_ref().and_then(|f| f.seat_lookup_tick),
        present_census.as_ref().and_then(|c| c.choice),
        present_census.as_ref().and_then(|c| c.snapshot_delta_time),
        auth_ps.map(|p| p.origin),
        adopted_ps.map(|p| p.origin),
        client_clock.as_ref().map(|c| c.debt_ms()),
        queue.0,
        auth_ps.map(|p| p.command_time),
        pred_ps.map(|p| p.command_time),
    );
}

fn emit_remote(
    clock: Option<Res<CgFrameClock>>,
    presented: Res<PresentedSnapshot>,
    local: Res<LocalPresentClient>,
    last_adopted: Option<Res<LastAdoptedSnapshot>>,
    identities: Query<(&CEntity, &CEntityRuntime)>,
) {
    let Some(clock) = clock else {
        return;
    };
    for (identity, runtime) in &identities {
        let Some(client) = identity.client() else {
            continue;
        };
        if client == local.0 {
            continue;
        }
        let outcome = presented
            .remote_sample_provenance(client)
            .map(|p| p.outcome.dump_kind());
        let snap_origin = last_adopted
            .as_ref()
            .and_then(|a| a.next())
            .or_else(|| presented.snapshot())
            .and_then(|snap| {
                snap.players
                    .iter()
                    .find(|(id, _)| *id == client)
                    .map(|(_, ps)| ps.origin)
            });
        perf::remote(
            client.0,
            clock.time(),
            i32::from(runtime.pose_e_type),
            runtime.origin,
            snap_origin,
            outcome,
        );
    }
}

fn lifecycle_label(life: ClientLifecycle) -> &'static str {
    match life {
        ClientLifecycle::Connecting => "Connecting",
        ClientLifecycle::ChoosingClass => "ChoosingClass",
        ClientLifecycle::SpawnPending => "SpawnPending",
        ClientLifecycle::Alive => "Alive",
        ClientLifecycle::Dead => "Dead",
        ClientLifecycle::RespawnPending => "RespawnPending",
        ClientLifecycle::Spectating => "Spectating",
        ClientLifecycle::Intermission => "Intermission",
    }
}
