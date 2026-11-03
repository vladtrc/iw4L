use bevy::prelude::*;
use frame::ScopeApp;
use frame::{RuntimeRole, SessionSwapApplied};
use net::{
    AUTHORITY_MS, AuthorityClock, AuthorityLoadHold, AuthoritySet, ClientClock, ClientSet,
    ReceivedTick, ReceivedTicks, ServerTick, ServerTime, authority_should_tick,
};

use crate::clip::ClipRing;
use crate::session::{Playback, Recording};

#[derive(Resource, Default)]
pub struct PendingReplayArm {
    pub playback: Option<Playback>,
    pub quit_on_end: bool,
    pub control: Option<PlaybackControl>,
    pub in_place: bool,
}

#[derive(Resource, Default)]
pub struct ReplaySession(pub Option<Recording>);

#[derive(Resource, Default)]
pub struct ReplayDiagnostics {
    pub line: Option<String>,
}

#[derive(SystemSet, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ReplayPump;

#[derive(Clone, Copy, Debug)]
pub struct PlaybackControl {
    pub paused: bool,
    pub rate: f32,
    pub steps: u32,
    pub stop_at: Option<u64>,
}

impl Default for PlaybackControl {
    fn default() -> Self {
        Self {
            paused: false,
            rate: 1.0,
            steps: 0,
            stop_at: None,
        }
    }
}

#[derive(Resource)]
pub struct ReplayPlayback {
    playback: Playback,
    pub control: PlaybackControl,
    acc_ms: f32,
    presented_once: bool,
    paused_last_pump: bool,
    initial_ticks: u64,
    pub ended: bool,
    pub retain_at_end: bool,

    pub quit_on_end: bool,
    pub exit_queued: bool,
}

impl ReplayPlayback {
    pub fn new(playback: Playback) -> Self {
        let initial_ticks = playback.ticks();
        Self {
            playback,
            control: PlaybackControl::default(),
            acc_ms: AUTHORITY_MS as f32,
            presented_once: false,
            paused_last_pump: false,
            initial_ticks,
            ended: false,
            retain_at_end: false,
            quit_on_end: true,
            exit_queued: false,
        }
    }

    pub fn with_quit_on_end(mut self, quit_on_end: bool) -> Self {
        self.quit_on_end = quit_on_end;
        self
    }

    pub fn step(&mut self) {
        self.control.paused = true;
        self.control.stop_at = None;
        self.control.steps = self.control.steps.saturating_add(1);
        self.quit_on_end = false;
    }

    pub fn ticks(&self) -> u64 {
        self.playback.ticks()
    }

    pub fn presented_frame(&self) -> f64 {
        let ticks = self.ticks();
        if self.control.paused || self.ended || ticks <= self.initial_ticks.saturating_add(1) {
            ticks.saturating_sub(1) as f64
        } else {
            ticks.saturating_sub(2) as f64
                + (self.acc_ms / AUTHORITY_MS as f32).clamp(0.0, 1.0) as f64
        }
    }

    pub fn acc_ms(&self) -> f32 {
        self.acc_ms
    }

    pub fn path(&self) -> &std::path::Path {
        self.playback.path()
    }
}

pub struct ReplayPlugin;

impl Plugin for ReplayPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<ReplaySession>()
            .init_resource::<ReplayDiagnostics>()
            .init_resource::<PendingReplayArm>()
            .scoped::<ClipRing>(frame::MatchScope::Live)
            .add_systems(OnEnter(frame::MatchScope::Absent), cancel_pending_playback);
        let role = app.world().get_resource::<RuntimeRole>().copied();
        if role.is_some_and(|role| role.runs_authority() || role == RuntimeRole::Replay) {
            app.add_systems(
                FixedUpdate,
                record_server_tick
                    .in_set(frame::InMatch)
                    .in_set(AuthoritySet::Fanout)
                    .run_if(authority_should_tick),
            );
        }
        app.add_systems(
            PreUpdate,
            (
                record_received_ticks
                    .in_set(frame::InMatch)
                    .after(ClientSet::Receive)
                    .before(ClientSet::Reconcile),
                sync_theater_occupancy
                    .in_set(frame::InMatch)
                    .in_set(ClientSet::Load)
                    .after(SessionSwapApplied),
                pump_playback
                    .in_set(ReplayPump)
                    .in_set(frame::InMatch)
                    .in_set(ClientSet::Load)
                    .run_if(resource_exists::<ReplayPlayback>)
                    .after(sync_theater_occupancy),
            ),
        );
    }
}

fn cancel_pending_playback(
    mut transitions: MessageReader<StateTransitionEvent<frame::MatchScope>>,
    mut pending: ResMut<PendingReplayArm>,
) {
    for transition in transitions.read() {
        if transition.exited.is_some() && transition.entered == Some(frame::MatchScope::Absent) {
            *pending = Default::default();
        }
    }
}

fn sync_theater_occupancy(
    mut commands: Commands,
    mut pending: ResMut<PendingReplayArm>,
    role: Res<RuntimeRole>,
    transition: Option<Res<session::SessionSwapRequest>>,
) {
    // Last still commits the old Live scope; a pending swap must not arm into it.
    if transition
        .as_ref()
        .is_some_and(|transition| transition.is_pending())
    {
        return;
    }
    if *role != RuntimeRole::Replay {
        return;
    }
    if pending.playback.is_none() {
        return;
    }
    // The new playback is armed only after the timeline restart has retired
    // the old ticks; arming in the same frame would feed its first snapshot
    // into state that is about to be dropped.
    if pending.in_place {
        pending.in_place = false;
        commands.queue(|world: &mut World| {
            world
                .resource_mut::<NextState<frame::Timeline>>()
                .set(frame::Timeline::Live);
        });
        return;
    }
    let Some(playback) = pending.playback.take() else {
        return;
    };
    pending.in_place = false;
    let quit_on_end = pending.quit_on_end;
    let mut playback = ReplayPlayback::new(playback).with_quit_on_end(quit_on_end);
    playback.control = pending
        .control
        .take()
        .unwrap_or_else(PlaybackControl::default);
    commands.queue(move |world: &mut World| {
        frame::scope::insert(world, playback, frame::MatchScope::Live);
    });
    perf::instant!("theater", "iw4l.sim", |ctx| {
        let (present, quit_on_end, runs_authority): (i64, Option<i64>, Option<i64>) = (
            1,
            Some(i64::from(quit_on_end)),
            Some(i64::from(role.runs_authority())),
        );
        ctx.arg("present", present);
        ctx.arg("quit_on_end", quit_on_end);
        ctx.arg("runs_authority", runs_authority);
    });
}

fn record_received_ticks(
    received: Res<ReceivedTicks>,
    role: Res<RuntimeRole>,
    mut ring: ResMut<ClipRing>,
    mut session: ResMut<ReplaySession>,
    mut diag: ResMut<ReplayDiagnostics>,
) {
    if *role == RuntimeRole::Dedicated {
        return;
    }
    for tick in &received.0 {
        if let Err(error) = ring.push_frame(&tick.snapshot, &tick.frame) {
            diag::warn!(Net, "clip: failed to retain tick: {error}");
        }
        if *role != RuntimeRole::Replay {
            record_snapshot(&mut session, &tick.snapshot, Some(&tick.frame), &mut diag);
        }
    }
}

fn record_server_tick(
    mut session: ResMut<ReplaySession>,
    mut ring: ResMut<ClipRing>,
    server_tick: Res<ServerTick>,
    role: Res<RuntimeRole>,
    mut diag: ResMut<ReplayDiagnostics>,
) {
    let Some(tick) = server_tick.0.as_ref() else {
        return;
    };
    if *role != RuntimeRole::Dedicated {
        return;
    }
    if let Err(error) = ring.push(&tick.snapshot) {
        diag::warn!(Net, "clip: failed to retain tick: {error}");
    }
    record_snapshot(&mut session, &tick.snapshot, None, &mut diag);
}

fn record_snapshot(
    session: &mut ReplaySession,
    snapshot: &sim::Snapshot,
    frame: Option<&net::Frame>,
    diag: &mut ReplayDiagnostics,
) {
    let Some(recording) = session.0.as_mut() else {
        return;
    };
    let result = match frame {
        Some(frame) => recording.record_frame(snapshot, frame),
        None => recording.record(snapshot),
    };
    if let Err(error) = result {
        let line = format!("record: write failed, recording stopped: {error}");
        diag::info!(Net, "{line}");
        diag.line = Some(line);
        session.0 = None;
    }
}

fn pump_playback(
    mut session: ResMut<ReplayPlayback>,
    mut received: ResMut<ReceivedTicks>,
    mut clock: Option<ResMut<AuthorityClock>>,
    mut client_clock: ResMut<ClientClock>,
    hold: Option<Res<AuthorityLoadHold>>,
    time: Res<Time>,
) {
    if session.ended {
        return;
    }
    if hold.is_none_or(|h| h.0) {
        return;
    }
    let slot = AUTHORITY_MS as f32;
    if session.paused_last_pump && !session.control.paused {
        session.acc_ms = slot;
    }
    let rate = session.control.rate;
    if !session.control.paused && rate.is_finite() && (0.05..=16.0).contains(&rate) {
        session.acc_ms += time.delta_secs() * 1000.0 * rate;
    }
    let mut budget = 256;
    while budget > 0
        && session
            .control
            .stop_at
            .is_none_or(|end| session.ticks() < end)
        && (!session.presented_once
            || session.control.steps > 0
            || (!session.control.paused && session.acc_ms >= slot))
    {
        budget -= 1;
        if !session.presented_once && session.control.paused && session.control.steps == 0 {
            session.acc_ms = 0.0;
        } else if session.control.steps > 0 {
            session.control.steps -= 1;
            session.acc_ms = 0.0;
        } else {
            session.acc_ms -= slot;
        }
        match session.playback.next_tick() {
            Ok(Some(played)) => {
                session.presented_once = true;
                if let Some(clock) = clock.as_mut() {
                    clock.tick = played.snapshot.tick.0;
                    clock.time_ms = ServerTime::from_tick(played.snapshot.tick).ms();
                }
                // What this frame is about to present. Two runs of the same
                // clip at different frame rates land on different frame
                // indices, so wall time cannot pair their frames and the demo
                // state can.
                perf::frames::set_replay_tick(u64::from(played.snapshot.tick.0));
                received.0.push_back(ReceivedTick {
                    snapshot: played.snapshot,
                    frame: played.frame,
                });
            }
            Ok(None) => {
                session.ended = true;
                diag::info!(
                    Net,
                    "play: ended after {} ticks ({})",
                    session.playback.ticks(),
                    session.playback.path().display()
                );
                break;
            }
            Err(error) => {
                session.ended = true;
                let line = format!("play: {error}");
                diag::warn!(Net, "{line}");
                break;
            }
        }
    }
    if session
        .control
        .stop_at
        .is_some_and(|end| session.ticks() >= end)
    {
        session.control.paused = true;
        session.acc_ms = 0.0;
    }
    session.paused_last_pump = session.control.paused;
    client_clock.accumulator_ms = if session.control.paused || session.ended {
        slot
    } else {
        session.acc_ms.min(slot)
    };
}
