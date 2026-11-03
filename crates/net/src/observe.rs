use bevy::prelude::*;

use crate::ClientSet;
use crate::FrameClock;
use crate::authority::inbox::ClientCommandInbox;
use crate::authority::runtime::{AuthorityWorld, ListenFanoutCensus};
use crate::client::presentation::centity_runtime::CEntityRuntime;
use crate::client::presentation::entities::CEntity;
use crate::client::presentation::presented::{
    LocalPresentClient, LocalPresentation, PresentLocalCensus, PresentedSnapshot,
};
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

#[derive(bevy::ecs::system::SystemParam)]
struct LocalPlayerStages<'w> {
    presentation: LocalPresentation<'w>,
    last_adopted: Option<Res<'w, LastAdoptedSnapshot>>,
    prediction: Option<Res<'w, ClientPredictionState>>,
    authority_world: Option<Res<'w, AuthorityWorld>>,
}

impl LocalPlayerStages<'_> {
    fn authority_player(&self) -> Option<&playerstate_iw4::PlayerState> {
        self.authority_world
            .as_ref()
            .and_then(|w| w.0.player(self.presentation.client_id()))
    }

    fn adopted_player(&self) -> Option<&playerstate_iw4::PlayerState> {
        self.last_adopted.as_ref().and_then(|a| {
            a.next().and_then(|snap| {
                snap.players
                    .iter()
                    .find(|(id, _)| *id == self.presentation.client_id())
                    .map(|(_, ps)| ps)
            })
        })
    }

    fn predicted_player(&self) -> Option<&playerstate_iw4::PlayerState> {
        self.prediction.as_ref().and_then(|p| p.0.predicted_local())
    }

    fn lifecycle(&self) -> Option<&'static str> {
        self.presentation
            .snapshot()
            .and_then(|s| s.meta.for_client(self.presentation.client_id()))
            .map(|m| lifecycle_label(m.lifecycle))
    }
}

fn emit_feel(
    clock: Option<Res<FrameClock>>,
    stages: LocalPlayerStages,
    client_clock: Option<Res<ClientClock>>,
    queue: Res<IngressQueueCensus>,
    fanout: Option<Res<ListenFanoutCensus>>,
    present_census: Option<Res<PresentLocalCensus>>,
) {
    if !perf::enabled() {
        return;
    }
    let Some(clock) = clock else {
        return;
    };
    let auth_ps = stages.authority_player();
    let adopted_ps = stages.adopted_player();
    let pred_ps = stages.predicted_player();
    let lifecycle = stages.lifecycle();
    perf::instant!("feel", "iw4l.sim", |ctx| {
        let (
            time_ms,
            fanout_seat_applied,
            seat_lookup_tick,
            present_choice,
            present_snapshot_delta_time,
            authority_origin,
            adopted_origin,
            client_clock_debt_ms,
            ingress_queue_depth,
            authority_command_time,
            predicted_command_time,
        ): (
            i32,
            Option<i32>,
            Option<i32>,
            Option<&str>,
            Option<i32>,
            Option<[f32; 3]>,
            Option<[f32; 3]>,
            Option<f32>,
            Option<i64>,
            Option<i32>,
            Option<i32>,
        ) = (
            clock.time(),
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
        ctx.arg("time_ms", i64::from(time_ms));
        ctx.arg("lifecycle", lifecycle);
        ctx.arg("fanout_seat_applied", fanout_seat_applied.map(i64::from));
        ctx.arg("seat_lookup_tick", seat_lookup_tick.map(i64::from));
        ctx.arg("present_choice", present_choice);
        ctx.arg(
            "present_snapshot_delta_time",
            present_snapshot_delta_time.map(i64::from),
        );
        if let Some(o) = authority_origin {
            ctx.arg("authority_origin_x", f64::from(o[0]));
            ctx.arg("authority_origin_y", f64::from(o[1]));
            ctx.arg("authority_origin_z", f64::from(o[2]));
        }
        if let Some(o) = adopted_origin {
            ctx.arg("adopted_origin_x", f64::from(o[0]));
            ctx.arg("adopted_origin_y", f64::from(o[1]));
            ctx.arg("adopted_origin_z", f64::from(o[2]));
        }
        ctx.arg("client_clock_debt_ms", client_clock_debt_ms.map(f64::from));
        ctx.arg("ingress_queue_depth", ingress_queue_depth);
        ctx.arg(
            "authority_command_time",
            authority_command_time.map(i64::from),
        );
        ctx.arg(
            "predicted_command_time",
            predicted_command_time.map(i64::from),
        );
    });
}

fn emit_remote(
    clock: Option<Res<FrameClock>>,
    presented: Res<PresentedSnapshot>,
    local: Res<LocalPresentClient>,
    last_adopted: Option<Res<LastAdoptedSnapshot>>,
    identities: Query<(&CEntity, &CEntityRuntime)>,
) {
    if !perf::enabled() {
        return;
    }
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
        perf::instant!("remote", "iw4l.render", |ctx| {
            let (client, time_ms, pose_e_type, origin, proxy_outcome): (
                u32,
                i32,
                i32,
                [f32; 3],
                Option<&str>,
            ) = (
                client.0,
                clock.time(),
                i32::from(runtime.pose_e_type),
                runtime.origin,
                outcome,
            );
            ctx.arg("client", i64::from(client));
            ctx.arg("time_ms", i64::from(time_ms));
            ctx.arg("pose_e_type", i64::from(pose_e_type));
            ctx.arg("origin_x", f64::from(origin[0]));
            ctx.arg("origin_y", f64::from(origin[1]));
            ctx.arg("origin_z", f64::from(origin[2]));
            if let Some(o) = snap_origin {
                ctx.arg("snap_origin_x", f64::from(o[0]));
                ctx.arg("snap_origin_y", f64::from(o[1]));
                ctx.arg("snap_origin_z", f64::from(o[2]));
            }
            ctx.arg("proxy_outcome", proxy_outcome);
        });
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
