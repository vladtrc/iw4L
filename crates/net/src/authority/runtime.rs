use bevy::app::{RunFixedMainLoop, RunFixedMainLoopSystems};
use bevy::ecs::system::SystemParam;
use bevy::prelude::*;
use frame::ScopeApp;
use std::collections::HashMap;

use crate::authority::inbox::{AuthorityClock, ClientActionInbox, ClientCommandInbox};
use crate::client::predict::CmdSeq;
use crate::client::presentation::presented::LocalPresentClient;
use crate::policy::seat::ActiveKillcams;
use crate::schedule::AuthoritySet;
use crate::transport::archive::FrameArchive;
use crate::transport::loopback_live::ListenLoopback;
use frame::{ExitLevelCalled, RuntimeRole, register_script_notify};
use sim::{ClientId, ClientLifecycle, DamageSource, SimEvent};

#[derive(Resource)]
pub struct AuthorityWorld(pub sim::SimWorld);

impl Default for AuthorityWorld {
    fn default() -> Self {
        Self(sim::SimWorld::new())
    }
}

#[derive(Resource, Debug, Default)]
pub struct ClientShotSamples(pub HashMap<(ClientId, i32), sim::ShotSampleProvenance>);

impl ClientShotSamples {
    pub fn note(&mut self, id: ClientId, command_time: i32, sample: sim::ShotSampleProvenance) {
        self.0.insert((id, command_time), sample);
    }
}

#[derive(Resource, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct AuthorityInputGate {
    pub local_cmds_enabled: bool,
}

#[derive(Resource, Default)]
pub struct PendingAuthorityInput(pub Option<sim::TickInput>);

#[derive(Resource, Default)]
pub struct PendingAcks(pub Vec<(ClientId, CmdSeq)>);

#[derive(Resource, Default)]
pub struct PendingStepResult(pub Option<ServerTickData>);

#[derive(Resource, Default)]
pub struct ServerTick(pub Option<ServerTickData>);

pub type ServerTickData = sim::TickResult;

#[derive(Resource, Default)]
pub struct NetDiagnostics {
    pub line: Option<String>,
}

#[derive(Resource, Default)]
struct FixedSpanOpen(bool);

fn stamp_authority_edge<const EDGE: u8>() {
    if EDGE > 0 {
        authority_toc_span(EDGE as usize - 1).end();
    }
    if (EDGE as usize) < 7 {
        authority_toc_span(EDGE as usize).begin();
    }
}

fn authority_toc_span(slot: usize) -> perf::Span {
    match slot {
        0 => perf::span!("Advance", "iw4l.sim", handle),
        1 => perf::span!("Ingress", "iw4l.sim", handle),
        2 => perf::span!("Gather", "iw4l.sim", handle),
        3 => perf::span!("Step", "iw4l.sim", handle),
        4 => perf::span!("Snapshot", "iw4l.sim", handle),
        5 => perf::span!("Fanout", "iw4l.sim", handle),
        6 => perf::span!("Bookkeeping", "iw4l.sim", handle),
        _ => unreachable!("authority phase slot {slot}"),
    }
}

fn register_authority_phase_seams(app: &mut App) {
    macro_rules! seams {
        ($($edge:literal),* $(,)?) => {$(
            app.add_systems(
                FixedUpdate,
                stamp_authority_edge::<$edge>
                    .in_set(frame::AuthorityEdge($edge))
                    .run_if(authority_should_tick),
            );
        )*};
    }
    seams!(0, 1, 2, 3, 4, 5, 6, 7);
}

#[derive(Resource, Debug, Clone, Default)]
pub struct ListenFanoutCensus {
    pub tick: Option<u32>,
    pub world_origin: Option<[f32; 3]>,
    pub snapshot_origin: Option<[f32; 3]>,
    pub sent_origin: Option<[f32; 3]>,
    pub seat_applied: Option<i32>,
    pub world_other_flags: Option<i32>,
    pub sent_other_flags: Option<i32>,
    pub sent_delta_time: Option<i32>,

    pub seat_archivetime_ms: Option<i32>,

    pub seat_focus_client: Option<i32>,

    pub seat_lookup_tick: Option<i32>,

    pub seat_attained_ms: Option<i32>,

    pub seat_rebase_ms: Option<i32>,

    pub seat_focus_live_origin: Option<[f32; 3]>,

    pub seat_focus_lifecycle: Option<&'static str>,

    pub seat_world_archived: Option<i32>,
}

#[derive(Resource, Debug, Clone, Default)]
pub struct DumpDeathLog {
    pub rows: Vec<DumpDeathRow>,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DumpDeathRow {
    pub tick: u32,
    pub victim: u32,
    pub attacker: Option<u32>,
    pub weapon: u32,
    pub suicide: i32,
    pub source: &'static str,

    pub life_sequence: u32,

    pub attacker_life: Option<u32>,

    pub killcam_entity_start_time: i32,
}

impl DumpDeathLog {
    fn push_journal(&mut self, journal: &[sim::EventRecord]) {
        for rec in journal {
            let SimEvent::Died {
                victim,
                attacker,
                weapon,
                source,
                life_sequence,
                attacker_life,
                killcam_entity_start_time,
            } = &rec.event
            else {
                continue;
            };
            if self
                .rows
                .iter()
                .any(|row| row.tick == rec.tick.0 && row.victim == victim.0)
            {
                continue;
            }
            let suicide = match attacker {
                None => 1,
                Some(a) if a == victim => 1,
                Some(_) => 0,
            };
            let source_s = match source {
                Some(DamageSource::Shot(_)) => "shot",
                Some(DamageSource::Projectile(_)) => "projectile",
                Some(DamageSource::Radius(_)) => "radius",
                Some(DamageSource::Melee) => "melee",
                None => "none",
            };
            self.rows.push(DumpDeathRow {
                tick: rec.tick.0,
                victim: victim.0,
                attacker: attacker.map(|a| a.0),
                weapon: *weapon,
                suicide,
                source: source_s,
                life_sequence: life_sequence.0,
                attacker_life: attacker_life.map(|life| life.0),
                killcam_entity_start_time: *killcam_entity_start_time,
            });
        }
    }
}

fn player_origin_flags(
    players: &[(ClientId, playerstate_iw4::PlayerState)],
    id: ClientId,
) -> Option<([f32; 3], i32, i32, i32)> {
    players.iter().find(|(c, _)| *c == id).map(|(_, ps)| {
        (
            ps.origin,
            ps.e_flags as i32,
            ps.other_flags as i32,
            ps.delta_time,
        )
    })
}

fn client_lifecycle_dump_label(life: ClientLifecycle) -> &'static str {
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

fn begin_fixed_span(mut open: ResMut<FixedSpanOpen>) {
    if !open.0 {
        perf::span!("FixedUpdate", "iw4l.sim", begin);
        open.0 = true;
    }
}

fn end_fixed_span(mut open: ResMut<FixedSpanOpen>) {
    if open.0 {
        perf::span!("FixedUpdate", "iw4l.sim", end);
        open.0 = false;
    }
}

#[derive(Resource, Clone, Copy, Debug, PartialEq, Eq)]
pub struct AuthorityLoadHold(pub bool);

impl Default for AuthorityLoadHold {
    fn default() -> Self {
        Self(true)
    }
}

impl AuthorityLoadHold {
    pub fn get(self) -> bool {
        self.0
    }
}

pub fn authority_should_tick(
    role: Res<RuntimeRole>,
    world: Option<Res<AuthorityWorld>>,
    hold: Option<Res<AuthorityLoadHold>>,
) -> bool {
    role.runs_authority()
        && world.is_some_and(|world| world.0.clip_brush_count() > 0)
        && hold.is_some_and(|h| !h.0)
}

fn release_authority(
    mut world: Option<ResMut<AuthorityWorld>>,
    mut inbox: ResMut<ClientCommandInbox>,
    mut transactions: ActionTransactionScope,
    mut input_gate: ResMut<AuthorityInputGate>,
) {
    inbox.clear();
    transactions.open_new_scope();
    *input_gate = AuthorityInputGate::default();
    if let Some(world) = world.as_mut() {
        world.0.shutdown_game();
    }
    perf::instant!("sim_hold", "iw4l.sim", |ctx| {
        let (running, movers, loopback_pending): (i64, i64, i64) = (0, 0, 0);
        ctx.arg("running", running);
        ctx.arg("movers", movers);
        ctx.arg("loopback_pending", loopback_pending);
    });
}

#[derive(Resource, Debug, Default)]
pub struct PendingConnectionFaults(pub Vec<(ClientId, String)>);

fn apply_connection_faults(
    mut faults: ResMut<PendingConnectionFaults>,
    mut world: ResMut<AuthorityWorld>,
    hub: Option<ResMut<crate::transport::udp_session::UdpAuthorityHub>>,
) {
    if faults.0.is_empty() {
        return;
    }
    let mut hub = hub;
    for (client, reason) in faults.0.drain(..) {
        if let Some(hub) = hub.as_mut() {
            hub.retire_client_with_reason(client, &reason);
        }
        world.0.retire_client(client);
    }
}

#[derive(bevy::ecs::system::SystemParam)]
pub struct ActionTransactionScope<'w> {
    actions: ResMut<'w, ClientActionInbox>,
    reliable: ResMut<'w, crate::ReliableEventHub>,
    roster: ResMut<'w, LastAuthorityRoster>,
}

impl ActionTransactionScope<'_> {
    fn open_new_scope(&mut self) {
        self.actions.clear();
        self.reliable.clear();
        *self.roster = LastAuthorityRoster::default();
    }

    fn retire_departed(
        &mut self,
        roster: Vec<ClientId>,
        cmds: &mut ResMut<ClientCommandInbox>,
        samples: &mut ResMut<ClientShotSamples>,
        hub: &mut Option<ResMut<crate::UdpAuthorityHub>>,
        staged: &mut ResMut<PendingAuthorityInput>,
        acks: &mut ResMut<PendingAcks>,
    ) {
        for client in self.roster.0.iter().copied() {
            if roster.contains(&client) {
                continue;
            }
            if let Some(hub) = hub.as_mut() {
                hub.retire_client(client);
            }
            if let Some(input) = staged.0.as_mut() {
                input.cmds.retain(|command| command.client != client);
                input.actions.retain(|(id, _)| *id != client);
            }
            acks.0.retain(|(id, _)| *id != client);
            cmds.retire_client(client);
            self.actions.retire_client(client);
            self.reliable.retire(client);
            samples.0.retain(|(id, _), _| *id != client);
            diag::info!(Net, "retire: client {} dropped from the roster", client.0);
        }
        self.roster.0 = roster;
    }
}

#[derive(Resource, Debug, Default)]
pub struct LastAuthorityRoster(Vec<ClientId>);

fn retire_departed_peers(
    server_tick: Res<ServerTick>,
    mut transactions: ActionTransactionScope,
    mut cmds: ResMut<ClientCommandInbox>,
    mut samples: ResMut<ClientShotSamples>,
    mut hub: Option<ResMut<crate::UdpAuthorityHub>>,
    mut staged: ResMut<PendingAuthorityInput>,
    mut acks: ResMut<PendingAcks>,
) {
    let Some(tick) = server_tick.0.as_ref() else {
        return;
    };
    let roster: Vec<ClientId> = tick
        .snapshot
        .meta
        .clients
        .iter()
        .map(|(id, _)| *id)
        .collect();
    transactions.retire_departed(
        roster,
        &mut cmds,
        &mut samples,
        &mut hub,
        &mut staged,
        &mut acks,
    );
}

fn advance_authority_clock(
    mut clock: ResMut<AuthorityClock>,
    mut server_tick: ResMut<ServerTick>,
    mut pending_step: ResMut<PendingStepResult>,
) {
    server_tick.0 = None;
    pending_step.0 = None;
    clock.advance();
}

#[derive(SystemParam)]
struct AuthorityCommandReceiver<'w> {
    cmd_inbox: ResMut<'w, ClientCommandInbox>,
    action_inbox: ResMut<'w, ClientActionInbox>,
    reliable: ResMut<'w, crate::ReliableEventHub>,
}

impl AuthorityCommandReceiver<'_> {
    fn receive(
        &mut self,
        world: &mut ResMut<AuthorityWorld>,
        bridge: Option<Res<crate::MasterBridge>>,
        hub: Option<ResMut<crate::transport::udp_session::UdpAuthorityHub>>,
        loopback_ack: Option<Res<crate::ClientReliableAck>>,
        local: Option<Res<LocalPresentClient>>,
    ) {
        if let (Some(ack), Some(local)) = (loopback_ack, local) {
            self.reliable.ack(local.0, ack.0);
        }
        if let Some(mut hub) = hub
            && let Err(e) = hub.ingress(
                &mut self.cmd_inbox,
                &mut self.action_inbox,
                Some(&mut self.reliable),
                Some((
                    &mut world.0,
                    bridge
                        .as_ref()
                        .map(|bridge| bridge.state().identity().match_key())
                        .unwrap_or(frame::MatchKey::NONE),
                )),
            )
        {
            diag::warn!(Net, "udp ingress: {e}");
        }
    }
}

fn ingress_authority(
    mut world: ResMut<AuthorityWorld>,
    bridge: Option<Res<crate::MasterBridge>>,
    hub: Option<ResMut<crate::transport::udp_session::UdpAuthorityHub>>,
    mut receiver: AuthorityCommandReceiver,
    loopback_ack: Option<Res<crate::ClientReliableAck>>,
    local: Option<Res<LocalPresentClient>>,
) {
    receiver.receive(&mut world, bridge, hub, loopback_ack, local);
}

#[derive(SystemParam)]
struct AuthorityInputAssembler<'w> {
    samples: ResMut<'w, ClientShotSamples>,
    backlog: ResMut<'w, PendingConnectionFaults>,
    pending: ResMut<'w, PendingAuthorityInput>,
    pending_acks: ResMut<'w, PendingAcks>,
}

impl AuthorityInputAssembler<'_> {
    fn assemble(
        &mut self,
        clock: &AuthorityClock,
        inbox: &mut ResMut<ClientCommandInbox>,
        action_inbox: &mut ResMut<ClientActionInbox>,
        reliable: &crate::ReliableEventHub,
        gate: &AuthorityInputGate,
        local: Option<&LocalPresentClient>,
    ) {
        self.backlog
            .0
            .extend(reliable.overflowed_clients().map(|client| {
                (
                    client,
                    "ReliableControlOverflow: outcome unknown".to_owned(),
                )
            }));
        self.backlog.0.extend(
            action_inbox
                .take_overflowed()
                .map(|client| (client, "ActionQueueOverflow: outcome unknown".to_owned())),
        );
        let mut actions = action_inbox.gather();
        // The gate is the local player's arming switch, not a licence to hold every
        // peer's input: a queue the authority never drains ages until
        // `ClientCommandInbox::backlog_fault` retires the peer for a stall the
        // authority itself caused. Drain on every tick the match is live, and drop
        // only what the local client is not yet allowed to contribute.
        let gathered = inbox.take_for_tick(clock.time_ms);

        self.samples.0.clear();
        for (client, command_time, sample) in gathered.samples {
            self.samples.note(client, command_time, sample);
        }

        for (client, fault) in gathered.backlog_faults {
            diag::warn!(Net, "client {}: {fault}", client.0);
            self.backlog.0.push((client, fault.to_string()));
        }
        let (mut cmds, mut acks) = (gathered.cmds, gathered.acks);
        if !gate.local_cmds_enabled
            && let Some(local) = local.as_ref()
        {
            cmds.retain(|command| command.client != local.0);
            acks.retain(|(id, _)| *id != local.0);
            self.samples.0.retain(|(id, _), _| *id != local.0);
        }
        for (client, _) in &self.backlog.0 {
            cmds.retain(|command| command.client != *client);
            acks.retain(|(id, _)| id != client);
            actions.retain(|(id, _)| id != client);
            self.samples.0.retain(|(id, _), _| id != client);
            inbox.retire_client(*client);
            action_inbox.retire_client(*client);
        }
        self.pending_acks.0 = acks;
        self.pending.0 = Some(sim::TickInput {
            cmds,
            actions,
            shot_samples: self
                .samples
                .0
                .iter()
                .map(|(key, sample)| (*key, *sample))
                .collect(),
        });
    }
}

fn gather_authority_input(
    clock: Res<AuthorityClock>,
    mut inbox: ResMut<ClientCommandInbox>,
    mut action_inbox: ResMut<ClientActionInbox>,
    reliable: Res<crate::ReliableEventHub>,
    gate: Res<AuthorityInputGate>,
    local: Option<Res<LocalPresentClient>>,
    mut assembler: AuthorityInputAssembler,
) {
    assembler.assemble(
        &clock,
        &mut inbox,
        &mut action_inbox,
        &reliable,
        &gate,
        local.as_deref(),
    );
}

fn step_authority(
    mut world: ResMut<AuthorityWorld>,
    clock: Res<AuthorityClock>,
    mut pending: ResMut<PendingAuthorityInput>,
    mut pending_step: ResMut<PendingStepResult>,
    mut pending_svc: ResMut<crate::PendingSvcSounds>,
    mut exit_level: MessageWriter<ExitLevelCalled>,
) {
    pending_svc.occupy_cs(&mut world.0);
    let Some(input) = pending.0.take() else {
        return;
    };

    let stepped = sim::try_step(
        &mut world.0,
        sim::Tick(clock.tick),
        &input,
        crate::AUTHORITY_MS,
    );
    // A terminal script error drops the match to the lobby;
    // the world stays frozen until the swap replaces it.
    let Ok(result) = stepped else {
        if let Some(fault) = world.0.take_script_fault() {
            diag::error!(Sim, "GSC execution failed: {fault}");
            diag::script_boundary(
                "fault",
                &format!(" fault=\"{}\"", fault.message.replace('"', "'")),
            );
            exit_level.write(ExitLevelCalled);
        }
        return;
    };
    pending_step.0 = Some(result);
}

fn publish_server_tick(
    mut pending_step: ResMut<PendingStepResult>,
    mut server_tick: ResMut<ServerTick>,
    mut reliable: ResMut<crate::ReliableEventHub>,
) {
    if let Some(tick) = pending_step.0.as_ref() {
        queue_reliable_events(&mut reliable, tick);
    }

    server_tick.0 = pending_step.0.take();
}

fn queue_reliable_events(reliable: &mut crate::ReliableEventHub, tick: &ServerTickData) {
    let recipients: Vec<ClientId> = tick
        .snapshot
        .meta
        .clients
        .iter()
        .map(|(id, _)| *id)
        .collect();
    for record in &tick.snapshot.meta.journal {
        if !sim::sim_event_is_reliable(&record.event) {
            continue;
        }
        for client in &recipients {
            if record.audience.projects_to(*client) {
                reliable.push_event(*client, record.event);
            }
        }
    }
}

#[derive(SystemParam)]
struct FanoutQueues<'w> {
    pending_acks: ResMut<'w, PendingAcks>,
    faults: ResMut<'w, PendingConnectionFaults>,
    pending_svc: ResMut<'w, crate::PendingSvcSounds>,
    pending_playercard: ResMut<'w, crate::PendingPlayerCard>,
    pending_gamenotify: ResMut<'w, crate::PendingGameNotify>,
    pending_scores: ResMut<'w, crate::PendingScoreboard>,
    local: Option<Res<'w, LocalPresentClient>>,
    kb: Option<Res<'w, crate::ClientActionInput>>,
    fanout_census: ResMut<'w, ListenFanoutCensus>,
    deaths: ResMut<'w, DumpDeathLog>,
    exit_level: MessageWriter<'w, ExitLevelCalled>,
}

#[derive(SystemParam)]
struct FanoutSeats<'w> {
    archive: Res<'w, FrameArchive>,
    seats: ResMut<'w, ActiveKillcams>,
}

impl FanoutSeats<'_> {
    fn apply_scripted(
        &mut self,
        world: &mut ResMut<'_, AuthorityWorld>,
        scripted: &[(ClientId, sim::ScriptSeat)],
        now_ms: i32,
    ) {
        crate::policy::killcam::play_script_seats(&mut self.seats, &self.archive, now_ms, scripted);
        for (viewer, seat) in scripted {
            if seat.archive_ms > 0
                && let Some(session) = self.seats.get(*viewer)
                && session.archivetime_ms != seat.archive_ms
            {
                world
                    .0
                    .set_script_archive_time(*viewer, seat.archive_ms, session.archivetime_ms);
            }
        }
    }

    fn sample(
        &self,
        live: &sim::Snapshot,
        viewer: ClientId,
        now_ms: i32,
    ) -> (sim::Snapshot, Option<crate::policy::seat::SeatSample>) {
        crate::policy::seat::snapshot_and_sample_for_viewer(
            &self.archive,
            &self.seats,
            live,
            viewer,
            now_ms,
        )
    }

    fn session(&self, viewer: ClientId) -> Option<&crate::policy::seat::KillcamSession> {
        self.seats.get(viewer)
    }

    fn override_snapshot(
        &self,
        live: &sim::Snapshot,
        viewer: ClientId,
        now_ms: i32,
    ) -> Option<sim::Snapshot> {
        crate::policy::seat::snapshot_override_for_viewer(
            &self.archive,
            &self.seats,
            live,
            viewer,
            now_ms,
        )
    }
}

#[derive(SystemParam)]
struct FanoutTransports<'w> {
    loopback: Option<ResMut<'w, ListenLoopback>>,
    hub: Option<ResMut<'w, crate::transport::udp_session::UdpAuthorityHub>>,
    diag: ResMut<'w, NetDiagnostics>,
}

impl FanoutTransports<'_> {
    fn send_local(
        &mut self,
        snapshot: &sim::Snapshot,
        acks: &[(ClientId, CmdSeq)],
        queues: &mut FanoutQueues<'_>,
        svc_scores: &mut Option<String>,
        reliable: &crate::ReliableEventHub,
    ) {
        if let Some(loopback) = self.loopback.as_mut() {
            let svc = queues
                .local
                .as_ref()
                .map(|id| queues.pending_svc.take_for(id.0))
                .unwrap_or(Vec::new());
            let (svc_card_slots, svc_open_menus, svc_hud_splashes) = queues
                .local
                .as_ref()
                .map(|id| queues.pending_playercard.take_for(id.0))
                .unwrap_or((Vec::new(), Vec::new(), Vec::new()));
            let svc_game_notifies = queues
                .local
                .as_ref()
                .map(|id| queues.pending_gamenotify.take_for(id.0))
                .unwrap_or(Vec::new());
            if let Err(e) = loopback.send_tick(
                snapshot,
                acks.to_vec(),
                crate::transport::frame::FrameTail {
                    svc_sounds: svc,
                    svc_scores: svc_scores.take(),
                    svc_card_slots,
                    svc_open_menus,
                    svc_hud_splashes,
                    svc_game_notifies,
                    reliable: queues
                        .local
                        .as_ref()
                        .map_or_else(crate::ReliablePayload::owes_nothing, |id| {
                            reliable.payload(id.0)
                        }),
                },
            ) {
                let line = format!("loopback: send_tick failed: {e}");
                diag::info!(Net, "{line}");
                self.diag.line = Some(line);
            }
        }
    }

    fn send_peers(
        &mut self,
        snapshot: &sim::Snapshot,
        acks: &[(ClientId, CmdSeq)],
        for_peer: impl FnMut(ClientId, &sim::Snapshot) -> Option<sim::Snapshot>,
        queues: &mut FanoutQueues<'_>,
        reliable: &crate::ReliableEventHub,
    ) {
        if let Some(hub) = self.hub.as_mut()
            && let Err(e) = hub.fanout_with_seats(
                snapshot,
                acks,
                for_peer,
                crate::transport::udp_session::FrameServiceQueues {
                    sounds: Some(&mut queues.pending_svc),
                    playercard: Some(&mut queues.pending_playercard),
                    notifications: Some(&mut queues.pending_gamenotify),
                },
                Some(reliable),
                false,
            )
        {
            let line = format!("udp fanout: {e}");
            diag::info!(Net, "{line}");
            self.diag.line = Some(line);
        }
    }
}

fn fanout_loopback(
    mut world: ResMut<AuthorityWorld>,
    server_tick: Res<ServerTick>,
    mut reliable: ResMut<crate::ReliableEventHub>,
    mut queues: FanoutQueues,
    mut seats: FanoutSeats,
    clock: Res<AuthorityClock>,
    mut delivery: FanoutTransports,
) {
    let Some(tick) = server_tick.0.as_ref() else {
        return;
    };
    for (client, reason) in tick.effects.kicks.iter().cloned() {
        reliable
            .queue_mut(client)
            .push(crate::ReliableRow::Failure(reason.clone()));
        queues.faults.0.push((client, reason));
    }
    queues
        .pending_playercard
        .adopt_events(&tick.effects.player_cards);
    queues.pending_gamenotify.adopt_prints(&tick.effects.prints);
    queues.pending_svc.adopt_sounds(&tick.effects.local_sounds);
    crate::svc_script_audio::fanout_script_audio(
        &mut reliable,
        tick.snapshot.meta.clients.iter().map(|(id, _)| *id),
        &tick.effects.script_audio,
    );
    seats.apply_scripted(&mut world, &tick.script_seats, clock.time_ms);
    if tick.script_exit_level {
        queues.exit_level.write(ExitLevelCalled);
        diag::info!(Sim, "exitLevel: called by the match scripts");
    }
    queues.deaths.push_journal(&tick.snapshot.meta.journal);

    let mut listen_snapshot = tick.snapshot.clone();
    let mut seat_applied = 0i32;
    if let Some(local) = queues.local.as_ref() {
        let (out, sample) = seats.sample(&tick.snapshot, local.0, clock.time_ms);
        listen_snapshot = out;
        if let Some(sample) = sample {
            seat_applied = 1;
            let session = seats.session(local.0);
            queues.fanout_census.seat_archivetime_ms = session.map(|s| s.archivetime_ms);
            queues.fanout_census.seat_focus_client = session.map(|s| s.focus_client.0 as i32);
            queues.fanout_census.seat_lookup_tick = sample.lookup.tick.map(|t| t.0 as i32);
            queues.fanout_census.seat_attained_ms = Some(sample.lookup.attained_ms);
            queues.fanout_census.seat_rebase_ms = Some(sample.rebase_ms);
            queues.fanout_census.seat_world_archived =
                Some(i32::from(sample.lookup.tick.is_some()));
            let focus = session.map(|s| s.focus_client);
            queues.fanout_census.seat_focus_live_origin =
                focus.and_then(|f| player_origin_flags(&tick.snapshot.players, f).map(|row| row.0));
            queues.fanout_census.seat_focus_lifecycle = focus.and_then(|f| {
                tick.snapshot
                    .meta
                    .for_client(f)
                    .map(|m| client_lifecycle_dump_label(m.lifecycle))
            });
        }
    }
    let local_id = queues.local.as_ref().map(|id| id.0);
    let world_row = local_id.and_then(|id| player_origin_flags(&tick.snapshot.players, id));
    let snap_row = local_id.and_then(|id| player_origin_flags(&tick.snapshot.players, id));
    let sent_row = local_id.and_then(|id| player_origin_flags(&listen_snapshot.players, id));
    queues.fanout_census.tick = Some(tick.snapshot.tick.0);
    queues.fanout_census.world_origin = world_row.map(|r| r.0);
    queues.fanout_census.snapshot_origin = snap_row.map(|r| r.0);
    queues.fanout_census.sent_origin = sent_row.map(|r| r.0);
    queues.fanout_census.seat_applied = Some(seat_applied);
    queues.fanout_census.world_other_flags = world_row.map(|r| r.2);
    queues.fanout_census.sent_other_flags = sent_row.map(|r| r.2);
    queues.fanout_census.sent_delta_time = sent_row.map(|r| r.3);
    if seat_applied == 0 {
        queues.fanout_census.seat_archivetime_ms = None;
        queues.fanout_census.seat_focus_client = None;
        queues.fanout_census.seat_lookup_tick = None;
        queues.fanout_census.seat_attained_ms = None;
        queues.fanout_census.seat_rebase_ms = None;
        queues.fanout_census.seat_focus_live_origin = None;
        queues.fanout_census.seat_focus_lifecycle = None;
        queues.fanout_census.seat_world_archived = None;
    }
    let local_life = queues.local.as_ref().and_then(|local| {
        tick.snapshot
            .meta
            .for_client(local.0)
            .map(|m| client_lifecycle_dump_label(m.lifecycle))
    });
    perf::instant!("feel", "iw4l.sim", |ctx| {
        let (
            time_ms,
            lifecycle,
            fanout_seat_applied,
            seat_lookup_tick,
            present_snapshot_delta_time,
        ): (i32, Option<&str>, Option<i32>, Option<i32>, Option<i32>) = (
            clock.time_ms,
            local_life,
            Some(seat_applied),
            queues.fanout_census.seat_lookup_tick,
            queues.fanout_census.sent_delta_time,
        );
        ctx.arg("time_ms", i64::from(time_ms));
        ctx.arg("lifecycle", lifecycle);
        ctx.arg("fanout_seat_applied", fanout_seat_applied.map(i64::from));
        ctx.arg("seat_lookup_tick", seat_lookup_tick.map(i64::from));

        ctx.arg(
            "present_snapshot_delta_time",
            present_snapshot_delta_time.map(i64::from),
        );
    });
    let acks = core::mem::take(&mut queues.pending_acks.0);
    let scores_down = queues
        .kb
        .as_ref()
        .is_some_and(|a| a.client.kb.scores.active);
    let mut svc_scores = queues
        .pending_scores
        .take_if_due(scores_down, clock.time_ms)
        .then(|| crate::format_scoreboard_from_snapshot(&listen_snapshot));
    let scores_broadcast_due = queues.pending_scores.broadcast_due(clock.time_ms);
    for (client, _) in &tick.snapshot.meta.clients {
        let queue = reliable.queue_mut(*client);
        for value in queues.pending_svc.take_for(*client) {
            queue.push(crate::ReliableRow::Sound(value));
        }
        let (cards, menus, splashes) = queues.pending_playercard.take_for(*client);
        for value in cards {
            queue.push(crate::ReliableRow::Card(value));
        }
        for value in menus {
            queue.push(crate::ReliableRow::Menu(value));
        }
        for value in splashes {
            queue.push(crate::ReliableRow::Splash(value));
        }
        for value in queues.pending_gamenotify.take_for(*client) {
            queue.push(crate::ReliableRow::Notify(value));
        }
        if scores_broadcast_due {
            queue.push(crate::ReliableRow::Scores(
                crate::format_scoreboard_from_snapshot(&tick.snapshot),
            ));
        }
    }
    queues.pending_gamenotify.clear_after_fanout();
    delivery.send_local(
        &listen_snapshot,
        &acks,
        &mut queues,
        &mut svc_scores,
        &reliable,
    );
    delivery.send_peers(
        &tick.snapshot,
        &acks,
        |peer, live| seats.override_snapshot(live, peer, clock.time_ms),
        &mut queues,
        &reliable,
    );
    let stranded = queues.pending_svc.drain_stranded();
    if !stranded.is_empty() {
        diag::info!(
            Net,
            "svc p/h: {} command(s) had no recipient Frame this Fanout (stranded {})",
            stranded.len(),
            queues.pending_svc.stranded
        );
    }
    let pc_stranded = queues.pending_playercard.drain_stranded();
    if pc_stranded > 0 {
        diag::info!(
            Net,
            "svc K/u: {pc_stranded} command(s) had no recipient Frame this Fanout (stranded {})",
            queues.pending_playercard.stranded
        );
    }
    queues.pending_gamenotify.clear_after_fanout();
}

pub fn authority_bookkeeping(
    server_tick: Res<ServerTick>,
    mut archive: ResMut<FrameArchive>,
    seats: Res<ActiveKillcams>,
    budget: Option<Res<crate::transport::archive::ArchiveBudget>>,
) {
    let budget = budget.map_or_else(Default::default, |budget| *budget).0;
    if archive.budget() != budget {
        archive.set_budget(budget);
    }
    if let Some(tick) = server_tick.0.as_ref() {
        archive.push_snapshot(&tick.snapshot, &seats.viewers());
    }
}
pub fn register_listen_runtime(app: &mut App) {
    register_script_notify(app);
    app.init_resource::<AuthorityInputGate>()
        .init_resource::<AuthorityLoadHold>()
        .scoped::<PendingAuthorityInput>(frame::MatchScope::Live)
        .scoped::<PendingAcks>(frame::MatchScope::Live)
        .scoped::<PendingStepResult>(frame::MatchScope::Live)
        .scoped::<ServerTick>(frame::MatchScope::Live)
        .scoped::<FrameArchive>(frame::MatchScope::Live)
        .scoped::<ActiveKillcams>(frame::MatchScope::Live)
        .init_resource::<NetDiagnostics>()
        .scoped::<ClientShotSamples>(frame::MatchScope::Live)
        .scoped::<crate::PendingSvcSounds>(frame::MatchScope::Live)
        .scoped::<crate::PendingPlayerCard>(frame::MatchScope::Live)
        .scoped::<crate::PendingGameNotify>(frame::MatchScope::Live)
        .scoped::<crate::PendingScoreboard>(frame::MatchScope::Live)
        .init_resource::<FixedSpanOpen>()
        .init_resource::<ListenFanoutCensus>()
        .init_resource::<DumpDeathLog>()
        .scoped::<LastAuthorityRoster>(frame::MatchScope::Live)
        .init_resource::<PendingConnectionFaults>();
    let role = *app.world().resource::<RuntimeRole>();
    if role != RuntimeRole::Dedicated {
        app.add_systems(
            FixedUpdate,
            (
                begin_fixed_span
                    .before(AuthoritySet::Advance)
                    .before(frame::AuthorityEdge(0)),
                advance_authority_clock
                    .in_set(frame::InMatch)
                    .in_set(AuthoritySet::Advance),
                ingress_authority
                    .in_set(frame::InMatch)
                    .in_set(AuthoritySet::Ingress),
                gather_authority_input
                    .in_set(frame::InMatch)
                    .in_set(AuthoritySet::Gather),
                step_authority
                    .in_set(frame::InMatch)
                    .in_set(AuthoritySet::Step),
                publish_server_tick
                    .in_set(frame::InMatch)
                    .in_set(AuthoritySet::Snapshot),
                fanout_loopback
                    .in_set(frame::InMatch)
                    .in_set(AuthoritySet::Fanout),
                apply_connection_faults
                    .in_set(frame::InMatch)
                    .in_set(AuthoritySet::Bookkeeping)
                    .before(retire_departed_peers),
                retire_departed_peers
                    .in_set(frame::InMatch)
                    .in_set(AuthoritySet::Bookkeeping),
                authority_bookkeeping
                    .in_set(frame::InMatch)
                    .in_set(frame::AuthorityBookkeeping),
            )
                .run_if(authority_should_tick),
        );
    } else {
        app.add_systems(
            FixedUpdate,
            (
                begin_fixed_span
                    .before(AuthoritySet::Advance)
                    .before(frame::AuthorityEdge(0)),
                advance_authority_clock
                    .in_set(frame::InMatch)
                    .in_set(AuthoritySet::Advance),
                ingress_authority
                    .in_set(frame::InMatch)
                    .in_set(AuthoritySet::Ingress),
                gather_authority_input
                    .in_set(frame::InMatch)
                    .in_set(AuthoritySet::Gather),
                step_authority
                    .in_set(frame::InMatch)
                    .in_set(AuthoritySet::Step),
                publish_server_tick
                    .in_set(frame::InMatch)
                    .in_set(AuthoritySet::Snapshot),
                fanout_loopback
                    .in_set(frame::InMatch)
                    .in_set(AuthoritySet::Fanout)
                    .after(crate::client::presentation::entities::sync_authority_entities),
                apply_connection_faults
                    .in_set(frame::InMatch)
                    .in_set(AuthoritySet::Bookkeeping)
                    .before(retire_departed_peers),
                retire_departed_peers
                    .in_set(frame::InMatch)
                    .in_set(AuthoritySet::Bookkeeping),
                authority_bookkeeping
                    .in_set(frame::InMatch)
                    .in_set(frame::AuthorityBookkeeping),
            )
                .run_if(authority_should_tick),
        );
        crate::client::presentation::entities::register_authority_entities(app);
    }
    app.add_systems(
        RunFixedMainLoop,
        end_fixed_span.in_set(RunFixedMainLoopSystems::AfterFixedMainLoop),
    );
    app.add_systems(
        OnExit(frame::MatchScope::Live),
        release_authority.in_set(frame::ScopeSet::Release),
    );
    register_authority_phase_seams(app);
}
