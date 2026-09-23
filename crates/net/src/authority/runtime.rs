use bevy::app::{RunFixedMainLoop, RunFixedMainLoopSystems};
use bevy::ecs::system::SystemParam;
use bevy::prelude::*;
use std::collections::HashMap;
use std::sync::Arc;

use crate::authority::inbox::{AuthorityClock, ClientActionInbox, ClientCommandInbox};
use crate::client::predict::CmdSeq;
use crate::client::presented::LocalPresentClient;
use crate::gaps::{NetGapCause, NetIdentityGaps, ScriptNotify};
use crate::policy::seat::ActiveKillcams;
use crate::role::RuntimeRole;
use crate::schedule::{AuthoritySet, ClientSet};
use crate::transport::archive::FrameArchive;
use crate::transport::loopback_live::ListenLoopback;
use frame::{
    AbortKillcam, BeginKillcam, ExitLevelCalled, GameEnded, GameWin, GameWinner, GlassDestroyed,
    KillcamEnded, MatchEndingReason, MatchEndingSoon, MatchEndingVerySoon, MatchTornDown,
    PrematchDone, RoundSwitchKind, RoundSwitchNotify, RoundWin, SpawnedPlayer, SpawnedPlayerNotify,
    SpawningIntermission, register_script_notify,
};
use gamemode_iw4::end_game::EndGameTailOutput;
use gamemode_iw4::sound_emit::{MatchEndingReason as GscEndingReason, MatchSoundEmit};
use sim::{ClientAction, ClientId, ClientLifecycle, DamageSource, SimEvent};

use crate::policy::killcam::{
    ActiveKillcamSkips, PendingDeathTimelines, ScriptKillcamEmitStats,
    abort_killcam_on_use_copycat, emit_spawned_player, expire_with_notify,
    spawned_clients_from_journal, start_round_end_wait_from_journal, start_timelines_from_deaths,
    tick_death_timelines, tick_final_killcam, tick_phase_b_skips, tick_round_end_wait,
    watch_nothing_to_show,
};

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

#[derive(Clone, Debug)]
pub struct ServerTickData {
    pub input: sim::TickInput,
    pub snapshot: sim::Snapshot,

    pub respawn_delay_ticks: u32,

    pub weapon_script_names: Arc<[String]>,

    pub pending_final_kill: Option<(ClientId, ClientId)>,
}

#[derive(Resource, Default)]
pub struct NetDiagnostics {
    pub line: Option<String>,
}

#[derive(Resource, Default, Debug, Clone)]
pub struct AuthorityPhaseTrace(pub Vec<&'static str>);

#[derive(Resource, Debug, Default, Clone)]
pub struct FixedUpdateCensus {
    pub steps: u32,
    accum_steps: u32,
    span_open: bool,
}

#[derive(Resource, Debug, Clone, Default)]
pub struct AuthorityPhaseCensus {
    pub n: [Option<u64>; frame::AUTHORITY_TOC.len()],

    pub bytes: [Option<u64>; frame::AUTHORITY_TOC.len()],
    accum_n: [Option<u64>; frame::AUTHORITY_TOC.len()],
    accum_bytes: [Option<u64>; frame::AUTHORITY_TOC.len()],
    seam_n: Option<u64>,
    seam_bytes: Option<u64>,
}

impl AuthorityPhaseCensus {
    pub fn alloc_n(&self) -> Option<String> {
        format_fixed_toc_u64(&self.n)
    }

    pub fn alloc_bytes(&self) -> Option<String> {
        format_fixed_toc_u64(&self.bytes)
    }

    pub fn alloc_sum_n(&self) -> Option<u64> {
        sum_named_fixed(&self.n)
    }

    pub fn alloc_sum_bytes(&self) -> Option<u64> {
        sum_named_fixed(&self.bytes)
    }
}

fn format_fixed_toc_u64(values: &[Option<u64>; frame::AUTHORITY_TOC.len()]) -> Option<String> {
    use std::fmt::Write as _;
    let mut out = String::new();
    for (index, phase) in frame::AUTHORITY_TOC.iter().enumerate() {
        let Some(n) = values[index] else { continue };
        if !out.is_empty() {
            out.push(',');
        }
        let _ = write!(out, "{}:{n}", frame::authority_set_name(phase));
    }
    (!out.is_empty()).then_some(out)
}

fn sum_named_fixed(values: &[Option<u64>; frame::AUTHORITY_TOC.len()]) -> Option<u64> {
    let mut sum = 0u64;
    let mut any = false;
    for slot in values {
        let Some(n) = slot else { continue };
        sum += *n;
        any = true;
    }
    any.then_some(sum)
}

fn stamp_authority_edge<const EDGE: u8>(mut census: ResMut<AuthorityPhaseCensus>) {
    let alloc = diag::process_allocations();
    if EDGE > 0 {
        let slot = EDGE as usize - 1;
        authority_toc_span(slot).end();
        if let Some(n0) = census.seam_n {
            let n = &mut census.accum_n[slot];
            *n = Some(n.unwrap_or(0) + alloc.process_allocations.saturating_sub(n0));
        }
        if let Some(b0) = census.seam_bytes {
            let b = &mut census.accum_bytes[slot];
            *b = Some(b.unwrap_or(0) + alloc.process_allocation_bytes.saturating_sub(b0));
        }
    }
    if (EDGE as usize) < frame::AUTHORITY_TOC.len() {
        authority_toc_span(EDGE as usize).begin();
    }
    census.seam_n = Some(alloc.process_allocations);
    census.seam_bytes = Some(alloc.process_allocation_bytes);
}

fn authority_toc_span(slot: usize) -> perf::Span {
    match slot {
        0 => perf::Span::FixedTocAdvance,
        1 => perf::Span::FixedTocIngress,
        2 => perf::Span::FixedTocGather,
        3 => perf::Span::FixedTocStep,
        4 => perf::Span::FixedTocSnapshot,
        5 => perf::Span::FixedTocFanout,
        6 => perf::Span::FixedTocBookkeeping,
        _ => unreachable!("AUTHORITY_TOC slot {slot}"),
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

fn publish_authority_phase_census(mut census: ResMut<AuthorityPhaseCensus>) {
    census.n = census.accum_n;
    census.bytes = census.accum_bytes;
    census.accum_n = Default::default();
    census.accum_bytes = Default::default();
    census.seam_n = None;
    census.seam_bytes = None;
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

#[derive(Resource, Debug, Clone, Default)]
pub struct DumpGiveLog {
    pub request_id: Option<u32>,
    pub weapon: Option<u32>,

    pub accepted: Option<u8>,

    pub reject_reason: Option<&'static str>,
}

impl DumpGiveLog {
    fn push_journal(&mut self, journal: &[sim::EventRecord]) {
        for rec in journal {
            match rec.event {
                SimEvent::GiveAccepted { request_id, weapon } => {
                    self.request_id = Some(request_id);
                    self.weapon = Some(weapon);
                    self.accepted = Some(1);
                    self.reject_reason = None;
                }
                SimEvent::GiveRejected {
                    request_id,
                    weapon,
                    reason,
                } => {
                    self.request_id = Some(request_id);
                    self.weapon = Some(weapon);
                    self.accepted = Some(0);
                    self.reject_reason = Some(reason.as_str());
                }
                _ => {}
            }
        }
    }
}

#[derive(Resource, Debug, Clone, Default)]
pub struct DumpConfigurationChangeLog {
    pub request_id: Option<u32>,
    pub from: Option<u32>,
    pub to: Option<u32>,
    pub accepted: Option<bool>,
    pub reject_reason: Option<&'static str>,
}

impl DumpConfigurationChangeLog {
    fn push_journal(&mut self, journal: &[sim::EventRecord]) {
        for rec in journal {
            match rec.event {
                SimEvent::ConfigurationChangeAccepted {
                    request_id,
                    from,
                    to,
                } => {
                    self.request_id = Some(request_id);
                    self.from = Some(from);
                    self.to = Some(to);
                    self.accepted = Some(true);
                    self.reject_reason = None;
                }
                SimEvent::ConfigurationChangeRejected {
                    request_id,
                    from,
                    to,
                    reason,
                } => {
                    self.request_id = Some(request_id);
                    self.from = Some(from);
                    self.to = Some(to);
                    self.accepted = Some(false);
                    self.reject_reason = Some(reason.as_str());
                }
                _ => {}
            }
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

fn push_phase(trace: Option<ResMut<AuthorityPhaseTrace>>, name: &'static str) {
    if let Some(mut t) = trace {
        t.0.push(name);
    }
}

fn begin_fixed_census(mut census: ResMut<FixedUpdateCensus>) {
    if !census.span_open {
        perf::Span::FramesFixedMs.begin();
        census.span_open = true;
    }
}

fn end_fixed_census(mut census: ResMut<FixedUpdateCensus>) {
    census.accum_steps = census.accum_steps.saturating_add(1);
}

fn publish_fixed_census(mut census: ResMut<FixedUpdateCensus>) {
    if census.span_open {
        perf::Span::FramesFixedMs.end();
        census.span_open = false;
    }
    census.steps = census.accum_steps;
    census.accum_steps = 0;
}

#[derive(Resource, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct AuthorityLoadHold(pub bool);

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
        && !hold.is_some_and(|h| h.0)
}

fn reset_authority_on_match_torn_down(
    mut torn: MessageReader<MatchTornDown>,
    mut clock: ResMut<AuthorityClock>,
    mut world: Option<ResMut<AuthorityWorld>>,
    mut loopback: Option<ResMut<ListenLoopback>>,
    mut archive: Option<ResMut<FrameArchive>>,
    mut end_game: ResMut<crate::policy::end_game::PendingEndGameTail>,
    mut inbox: ResMut<ClientCommandInbox>,
    mut transactions: ActionTransactionScope,
    mut samples: ResMut<ClientShotSamples>,
    mut input_gate: ResMut<AuthorityInputGate>,
    mut pending_input: ResMut<PendingAuthorityInput>,
    mut pending_acks: ResMut<PendingAcks>,
    mut pending_step: ResMut<PendingStepResult>,
    mut server_tick: ResMut<ServerTick>,
) {
    if torn.read().count() == 0 {
        return;
    }
    *clock = AuthorityClock::default();

    inbox.clear();
    transactions.open_new_scope();
    samples.0.clear();
    *input_gate = AuthorityInputGate::default();
    pending_input.0 = None;
    pending_acks.0.clear();
    pending_step.0 = None;
    server_tick.0 = None;

    *end_game = crate::policy::end_game::PendingEndGameTail::default();
    if let Some(world) = world.as_mut() {
        world.0.shutdown_game();
    }
    if let Some(loopback) = loopback.as_mut() {
        loopback.reset();
    }
    if let Some(archive) = archive.as_mut() {
        archive.clear();
    }
    perf::sim_hold(
        i64::from(world.as_ref().is_some_and(|world| world.0.is_running())),
        world
            .as_ref()
            .map_or(0, |world| world.0.script_mover_count() as i64),
        loopback.as_ref().map(|l| l.pending() as i64).unwrap_or(0),
    );
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
    ledger: ResMut<'w, crate::ClientActionLedger>,
    reliable: ResMut<'w, crate::ReliableEventHub>,
    request_ids: ResMut<'w, crate::ActionRequestIds>,
    roster: ResMut<'w, LastAuthorityRoster>,
}

impl ActionTransactionScope<'_> {
    fn open_new_scope(&mut self) {
        self.actions.clear();
        self.ledger.open_scope();
        self.reliable.clear();
        self.request_ids.reset();
        *self.roster = LastAuthorityRoster::default();
    }
}

#[derive(Resource, Debug, Default)]
pub struct LastAuthorityRoster(Vec<ClientId>);

fn retire_departed_peers(
    server_tick: Res<ServerTick>,
    mut last: ResMut<LastAuthorityRoster>,
    mut cmds: ResMut<ClientCommandInbox>,
    mut actions: ResMut<ClientActionInbox>,
    mut ledger: ResMut<crate::ClientActionLedger>,
    mut reliable: ResMut<crate::ReliableEventHub>,
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
    for client in last.0.iter().copied() {
        if roster.contains(&client) {
            continue;
        }
        if let Some(hub) = hub.as_mut() {
            hub.retire_client(client);
        }
        if let Some(input) = staged.0.as_mut() {
            input.cmds.retain(|(id, _)| *id != client);
            input.actions.retain(|(id, _)| *id != client);
        }
        acks.0.retain(|(id, _)| *id != client);
        cmds.retire_client(client);
        actions.retire_client(client);
        ledger.retire_client(client);
        reliable.retire(client);
        samples.0.retain(|(id, _), _| *id != client);
        diag::info!(Net, "retire: client {} dropped from the roster", client.0);
    }
    last.0 = roster;
}

fn advance_authority_clock(
    mut clock: ResMut<AuthorityClock>,
    mut server_tick: ResMut<ServerTick>,
    mut pending_step: ResMut<PendingStepResult>,
    mut trace: Option<ResMut<AuthorityPhaseTrace>>,
) {
    if let Some(t) = trace.as_deref_mut() {
        t.0.clear();
    }
    push_phase(trace, "Advance");
    server_tick.0 = None;
    pending_step.0 = None;
    clock.advance();
}

fn ingress_authority(
    hub: Option<ResMut<crate::transport::udp_session::UdpAuthorityHub>>,
    mut cmd_inbox: ResMut<ClientCommandInbox>,
    mut action_inbox: ResMut<ClientActionInbox>,
    mut samples: ResMut<ClientShotSamples>,
    mut reliable: ResMut<crate::ReliableEventHub>,
    loopback_ack: Option<Res<crate::ClientReliableAck>>,
    local: Option<Res<LocalPresentClient>>,
    trace: Option<ResMut<AuthorityPhaseTrace>>,
) {
    push_phase(trace, "Ingress");

    if let (Some(ack), Some(local)) = (loopback_ack, local) {
        reliable.ack(local.0, ack.0);
    }
    if let Some(mut hub) = hub {
        if let Err(e) = hub.ingress(
            &mut cmd_inbox,
            &mut action_inbox,
            Some(&mut samples),
            Some(&mut reliable),
        ) {
            diag::warn!(Net, "udp ingress: {e}");
        }
    }
}

fn gather_authority_input(
    clock: Res<AuthorityClock>,
    mut inbox: ResMut<ClientCommandInbox>,
    mut samples: ResMut<ClientShotSamples>,
    mut backlog: ResMut<PendingConnectionFaults>,
    mut action_inbox: ResMut<ClientActionInbox>,
    mut ledger: ResMut<crate::ClientActionLedger>,
    mut reliable: ResMut<crate::ReliableEventHub>,
    gate: Res<AuthorityInputGate>,
    local: Option<Res<LocalPresentClient>>,
    mut pending: ResMut<PendingAuthorityInput>,
    mut pending_acks: ResMut<PendingAcks>,
    trace: Option<ResMut<AuthorityPhaseTrace>>,
) {
    push_phase(trace, "Gather");
    backlog
        .0
        .extend(reliable.overflowed_clients().map(|client| {
            (
                client,
                "ReliableControlOverflow: outcome unknown".to_owned(),
            )
        }));
    backlog.0.extend(
        action_inbox
            .take_overflowed()
            .map(|client| (client, "ActionQueueOverflow: outcome unknown".to_owned())),
    );
    let mut actions = Vec::new();
    for (client, action) in action_inbox.gather() {
        let request_id = sim::action_request_id(&action);
        match ledger.admit(client, &action) {
            crate::ActionAdmission::Fresh => actions.push((client, action)),
            crate::ActionAdmission::Pending => {}
            crate::ActionAdmission::Repeat(verdict) => {
                reliable.push_outcome(client, request_id, verdict);
            }
            crate::ActionAdmission::PayloadMismatch => {
                diag::warn!(
                    Net,
                    "action request_id {request_id} re-used with a different payload by client {}                      — refused",
                    client.0
                );
                reliable.push_outcome(client, request_id, crate::ActionVerdict::PayloadMismatch);
            }
            crate::ActionAdmission::Expired => {
                reliable.push_outcome(client, request_id, crate::ActionVerdict::Expired);
            }
        }
    }
    // The gate is the local player's arming switch, not a licence to hold every
    // peer's input: a queue the authority never drains ages until
    // `ClientCommandInbox::backlog_fault` retires the peer for a stall the
    // authority itself caused. Drain on every tick the match is live, and drop
    // only what the local client is not yet allowed to contribute.
    let gathered = inbox.take_for_tick(clock.time_ms);

    samples.0.clear();
    for (client, command_time, sample) in gathered.samples {
        samples.note(client, command_time, sample);
    }

    for (client, fault) in gathered.backlog_faults {
        diag::warn!(Net, "client {}: {fault}", client.0);
        backlog.0.push((client, fault.to_string()));
    }
    let (mut cmds, mut acks) = (gathered.cmds, gathered.acks);
    if !gate.local_cmds_enabled
        && let Some(local) = local.as_ref()
    {
        cmds.retain(|(id, _)| *id != local.0);
        acks.retain(|(id, _)| *id != local.0);
        samples.0.retain(|(id, _), _| *id != local.0);
    }
    for (client, _) in &backlog.0 {
        cmds.retain(|(id, _)| id != client);
        acks.retain(|(id, _)| id != client);
        actions.retain(|(id, _)| id != client);
        samples.0.retain(|(id, _), _| id != client);
        inbox.retire_client(*client);
        action_inbox.retire_client(*client);
        ledger.retire_client(*client);
    }
    pending_acks.0 = acks;
    pending.0 = Some(sim::TickInput { cmds, actions });
}

fn step_authority(
    mut world: ResMut<AuthorityWorld>,
    clock: Res<AuthorityClock>,
    mut pending: ResMut<PendingAuthorityInput>,
    mut pending_step: ResMut<PendingStepResult>,
    mut pending_svc: ResMut<crate::PendingSvcSounds>,
    samples: Res<ClientShotSamples>,
    trace: Option<ResMut<AuthorityPhaseTrace>>,
) {
    push_phase(trace, "Step");
    pending_svc.occupy_cs(&mut world.0);
    let Some(input) = pending.0.take() else {
        return;
    };

    world
        .0
        .set_lagcomp_commands(samples.0.iter().map(|(key, sample)| (*key, *sample)));
    let snapshot = sim::step(
        &mut world.0,
        sim::Tick(clock.tick),
        &input,
        crate::AUTHORITY_MS,
        sim::StepReason::AuthorityFrame,
    );
    let respawn_delay_ticks = world.0.respawn_delay_ticks();
    let weapon_script_names = world.0.weapon_script_names();
    let pending_final_kill = world.0.take_pending_final_kill();
    pending_step.0 = Some(ServerTickData {
        input,
        snapshot,
        respawn_delay_ticks,
        weapon_script_names,
        pending_final_kill,
    });
}

fn publish_server_tick(
    mut pending_step: ResMut<PendingStepResult>,
    mut server_tick: ResMut<ServerTick>,
    mut ledger: ResMut<crate::ClientActionLedger>,
    mut reliable: ResMut<crate::ReliableEventHub>,
    mut actions: ResMut<ClientActionInbox>,
    trace: Option<ResMut<AuthorityPhaseTrace>>,
) {
    push_phase(trace, "Snapshot");
    if let Some(tick) = pending_step.0.as_ref() {
        record_action_outcomes(&mut ledger, &mut reliable, &mut actions, tick);
        queue_reliable_events(&mut reliable, tick);
    }

    server_tick.0 = pending_step.0.take();
}

fn record_action_outcomes(
    ledger: &mut crate::ClientActionLedger,
    reliable: &mut crate::ReliableEventHub,
    actions: &mut ClientActionInbox,
    tick: &ServerTickData,
) {
    for (client, action) in &tick.input.actions {
        let request_id = sim::action_request_id(action);
        let refused = tick
            .snapshot
            .meta
            .journal
            .iter()
            .any(|record| sim_event_refuses(&record.event, request_id));
        let verdict = if refused {
            crate::ActionVerdict::Refused
        } else {
            crate::ActionVerdict::Applied
        };
        ledger.record(*client, *action, verdict);
        reliable.push_outcome(*client, request_id, verdict);

        actions.retire(*client, request_id);
    }
}

fn sim_event_refuses(event: &sim::SimEvent, request_id: sim::ActionRequestId) -> bool {
    match event {
        sim::SimEvent::ClassRejected {
            request_id: rid, ..
        }
        | sim::SimEvent::GiveRejected {
            request_id: rid, ..
        }
        | sim::SimEvent::ConfigurationChangeRejected {
            request_id: rid, ..
        } => *rid == request_id,
        _ => false,
    }
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
    pending_svc: ResMut<'w, crate::PendingSvcSounds>,
    pending_playercard: ResMut<'w, crate::PendingPlayerCard>,
    pending_gamenotify: ResMut<'w, crate::PendingGameNotify>,
    pending_scores: ResMut<'w, crate::PendingScoreboard>,
    world: ResMut<'w, AuthorityWorld>,
    local: Option<Res<'w, LocalPresentClient>>,
    kb: Option<Res<'w, crate::ClientActionInput>>,
    fanout_census: ResMut<'w, ListenFanoutCensus>,
    deaths: ResMut<'w, DumpDeathLog>,
    gives: ResMut<'w, DumpGiveLog>,
    configuration_changes: ResMut<'w, DumpConfigurationChangeLog>,
    actions: ResMut<'w, ClientActionInbox>,
    entity_slots: Option<Res<'w, crate::CEntitySlots>>,
    entity_notify_gaps: ResMut<'w, NetIdentityGaps>,
    pending_notifies: Option<ResMut<'w, PendingScriptEntityNotifies>>,
    role: Res<'w, RuntimeRole>,
    end_game: ResMut<'w, crate::policy::end_game::PendingEndGameTail>,
    spawning_intermission: MessageWriter<'w, SpawningIntermission>,
    exit_level: MessageWriter<'w, ExitLevelCalled>,
}

fn fanout_loopback(
    mut commands: Commands,
    loopback: Option<ResMut<ListenLoopback>>,
    hub: Option<ResMut<crate::transport::udp_session::UdpAuthorityHub>>,
    server_tick: Res<ServerTick>,
    mut reliable: ResMut<crate::ReliableEventHub>,
    mut queues: FanoutQueues,
    mut diag: ResMut<NetDiagnostics>,
    archive: Res<FrameArchive>,
    mut seats: ResMut<ActiveKillcams>,
    mut pending_deaths: ResMut<PendingDeathTimelines>,
    mut skips: ResMut<ActiveKillcamSkips>,
    clock: Res<AuthorityClock>,
    mut kc_stats: ResMut<ScriptKillcamEmitStats>,
    trace: Option<ResMut<AuthorityPhaseTrace>>,
) {
    push_phase(trace, "Fanout");
    let Some(tick) = server_tick.0.as_ref() else {
        return;
    };
    queues
        .pending_playercard
        .adopt_from_world(&mut queues.world.0);
    let tus = crate::policy::killcam::time_until_spawn_seconds(
        tick.respawn_delay_ticks,
        crate::AUTHORITY_MS as u32,
    );
    let mut level =
        crate::policy::killcam::level_notifies_from_journal(&tick.snapshot.meta.journal);
    let spawned = spawned_clients_from_journal(&tick.snapshot.meta.journal);
    let mut spawned_notifies = Vec::new();
    let mut ended_notifies = Vec::new();
    let mut begin_notifies = Vec::new();
    let mut abort_notifies = Vec::new();
    emit_spawned_player(
        &mut seats,
        &mut skips,
        &tick.snapshot.meta.journal,
        &mut kc_stats,
        &mut spawned_notifies,
        &mut ended_notifies,
    );
    start_round_end_wait_from_journal(
        &mut pending_deaths,
        clock.time_ms,
        &tick.snapshot.meta.journal,
        &mut kc_stats,
    );

    level.extend(tick_round_end_wait(
        &mut pending_deaths,
        clock.time_ms,
        &mut kc_stats,
    ));

    let copycat_victims: std::collections::HashSet<ClientId> = tick
        .snapshot
        .meta
        .clients
        .iter()
        .filter_map(|(id, _)| {
            queues
                .world
                .0
                .client_meta(*id)
                .is_some_and(|m| m.copycat_stash_defined())
                .then_some(*id)
        })
        .collect();
    start_timelines_from_deaths(
        &mut pending_deaths,
        clock.time_ms,
        &tick.snapshot.meta.journal,
        tus,
        &tick.weapon_script_names,
        tick.pending_final_kill,
        &copycat_victims,
        &mut kc_stats,
    );
    queues.deaths.push_journal(&tick.snapshot.meta.journal);
    queues.gives.push_journal(&tick.snapshot.meta.journal);
    queues
        .configuration_changes
        .push_journal(&tick.snapshot.meta.journal);
    pending_deaths.queue_spawn_for_deaths_without_timeline(&tick.snapshot.meta.journal);
    tick_death_timelines(
        &mut pending_deaths,
        &mut seats,
        &mut skips,
        &archive,
        clock.time_ms,
        &tick.input.cmds,
        &level,
        &spawned,
        &mut kc_stats,
        &mut begin_notifies,
        &tick.snapshot,
    );

    let viewers: Vec<ClientId> = tick.snapshot.players.iter().map(|(id, _)| *id).collect();
    let mut killedby_cards = Vec::new();
    let showing_final_killcam = tick_final_killcam(
        &mut pending_deaths,
        &mut seats,
        &archive,
        clock.time_ms,
        &viewers,
        &level,
        &mut kc_stats,
        &mut begin_notifies,
        &mut killedby_cards,
    );
    for (viewer, attacker) in killedby_cards {
        queues
            .world
            .0
            .push_player_card_slot(viewer, attacker, hud_iw4::PLAYER_CARD_SLOT_KILLEDBY);
        queues
            .world
            .0
            .push_player_card_open(viewer, hud_iw4::SCRIPT_MENU_KILLEDBY_DISPLAY);
    }

    queues
        .end_game
        .start_from_journal(&tick.snapshot.meta.journal);
    let round_end_finished = level.contains(&killcam_iw4::NotifyKind::RoundEndFinished);
    if round_end_finished && showing_final_killcam {
        queues.world.0.reset_outcome();
    }
    match queues
        .end_game
        .advance(clock.time_ms, round_end_finished, showing_final_killcam)
    {
        Some(EndGameTailOutput::SpawningIntermission) => {
            queues.world.0.reset_outcome();
            queues.spawning_intermission.write(SpawningIntermission);
            let request_id = 20_000u32.saturating_add(queues.end_game.spawning_intermission);
            for (id, _) in tick.snapshot.meta.clients.iter() {
                if let Err(error) = queues
                    .actions
                    .push(*id, ClientAction::SpawnIntermission { request_id })
                {
                    diag::warn!(
                        Sim,
                        "endGame: spawnIntermission for client {} not queued: {error}",
                        id.0
                    );
                }
            }
            diag::info!(
                Sim,
                "endGame: level.intermission — spawnIntermission for {} clients",
                tick.snapshot.meta.clients.len()
            );
        }
        Some(EndGameTailOutput::ExitLevel) => {
            queues.exit_level.write(ExitLevelCalled);
            diag::info!(Sim, "endGame: exitLevel( false )");
        }
        None => {}
    }

    tick_phase_b_skips(
        &mut seats,
        &mut skips,
        &tick.input.cmds,
        &mut kc_stats,
        &mut abort_notifies,
    );
    abort_killcam_on_use_copycat(
        &mut seats,
        &mut skips,
        &tick.input.actions,
        &mut kc_stats,
        &mut abort_notifies,
    );

    watch_nothing_to_show(
        &mut seats,
        &mut skips,
        &archive,
        &mut kc_stats,
        &mut abort_notifies,
    );
    expire_with_notify(
        &mut seats,
        &mut skips,
        clock.time_ms,
        &mut kc_stats,
        &mut ended_notifies,
    );
    for id in ended_notifies.iter().chain(abort_notifies.iter()).copied() {
        pending_deaths.on_ordinary_seat_finished(id);
    }
    for id in pending_deaths.take_spawn_client() {
        let still_dead = tick
            .snapshot
            .meta
            .for_client(id)
            .is_some_and(|m| m.lifecycle == ClientLifecycle::Dead);
        if !still_dead {
            continue;
        }
        kc_stats.spawn_client = kc_stats.spawn_client.saturating_add(1);
        let request_id = 10_000u32.saturating_add(kc_stats.spawn_client);
        if let Err(error) = queues
            .actions
            .push(id, ClientAction::SpawnClient { request_id })
        {
            diag::warn!(
                Sim,
                "respawn: spawnClient for client {} not queued: {error}",
                id.0
            );
        }
    }
    if *queues.role == RuntimeRole::Listen {
        if let Some(pending) = queues.pending_notifies.as_mut() {
            pending.store(
                begin_notifies,
                spawned_notifies,
                abort_notifies,
                ended_notifies,
            );
        }
    } else if let Some(slots) = queues.entity_slots.as_ref() {
        trigger_script_entity_notifies(
            &mut commands,
            slots,
            &mut queues.entity_notify_gaps,
            begin_notifies,
            spawned_notifies,
            abort_notifies,
            ended_notifies,
        );
    }

    let mut listen_snapshot = tick.snapshot.clone();
    let mut seat_applied = 0i32;
    if let Some(local) = queues.local.as_ref() {
        let already_alive = tick
            .snapshot
            .meta
            .for_client(local.0)
            .is_some_and(|m| m.lifecycle == ClientLifecycle::Alive);

        let ordinary_seat = seats
            .get(local.0)
            .is_some_and(|session| !session.final_kill);
        if already_alive && ordinary_seat {
            seats.clear(local.0);
            skips.clear(local.0);
            kc_stats.seats_refused_already_alive =
                kc_stats.seats_refused_already_alive.saturating_add(1);
        } else {
            let (out, sample) = crate::policy::seat::snapshot_and_sample_for_viewer(
                &archive,
                &seats,
                &tick.snapshot,
                local.0,
                clock.time_ms,
            );
            listen_snapshot = out;
            if let Some(sample) = sample {
                seat_applied = 1;
                let session = seats.get(local.0);
                queues.fanout_census.seat_archivetime_ms = session.map(|s| s.archivetime_ms);
                queues.fanout_census.seat_focus_client = session.map(|s| s.focus_client.0 as i32);
                queues.fanout_census.seat_lookup_tick = sample.lookup.tick.map(|t| t.0 as i32);
                queues.fanout_census.seat_attained_ms = Some(sample.lookup.attained_ms);
                queues.fanout_census.seat_rebase_ms = Some(sample.rebase_ms);
                queues.fanout_census.seat_world_archived =
                    Some(i32::from(sample.lookup.tick.is_some()));
                let focus = session.map(|s| s.focus_client);
                queues.fanout_census.seat_focus_live_origin =
                    focus.and_then(|f| queues.world.0.player(f).map(|ps| ps.origin));
                queues.fanout_census.seat_focus_lifecycle = focus.and_then(|f| {
                    queues
                        .world
                        .0
                        .client_meta(f)
                        .map(|m| client_lifecycle_dump_label(m.lifecycle))
                });
            }
        }
    }
    let local_id = queues.local.as_ref().map(|id| id.0);
    let world_row = local_id.and_then(|id| {
        queues
            .world
            .0
            .player(id)
            .map(|ps| (ps.origin, ps.e_flags as i32, ps.other_flags as i32))
    });
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
    perf::feel(
        clock.time_ms,
        local_life,
        Some(seat_applied),
        queues.fanout_census.seat_lookup_tick,
        None,
        queues.fanout_census.sent_delta_time,
        None,
        None,
        None,
        None,
        None,
        None,
    );
    let acks = core::mem::take(&mut queues.pending_acks.0);
    let scores_down = queues
        .kb
        .as_ref()
        .is_some_and(|a| a.client.kb.scores.active);
    let svc_scores = queues
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
        for value in queues.pending_gamenotify.take_broadcast() {
            queue.push(crate::ReliableRow::Notify(value));
        }
        if scores_broadcast_due {
            queue.push(crate::ReliableRow::Scores(
                crate::format_scoreboard_from_snapshot(&tick.snapshot),
            ));
        }
    }
    queues.pending_gamenotify.clear_after_fanout();
    if let Some(mut loopback) = loopback {
        let svc = queues
            .local
            .as_ref()
            .map(|id| queues.pending_svc.take_for(id.0))
            .unwrap_or_default();
        let (svc_card_slots, svc_open_menus, svc_hud_splashes) = queues
            .local
            .as_ref()
            .map(|id| queues.pending_playercard.take_for(id.0))
            .unwrap_or_default();
        let svc_game_notifies = queues.pending_gamenotify.take_broadcast();
        if let Err(e) = loopback.send_tick(
            &tick.input,
            &listen_snapshot,
            acks.clone(),
            svc,
            svc_scores,
            svc_card_slots,
            svc_open_menus,
            svc_hud_splashes,
            svc_game_notifies,
            queues
                .local
                .as_ref()
                .map_or_else(crate::ReliablePayload::owes_nothing, |id| {
                    reliable.payload(id.0)
                }),
        ) {
            let line = format!("loopback: send_tick failed: {e}");
            diag::info!(Net, "{line}");
            diag.line = Some(line);
        }
    }
    if let Some(mut hub) = hub {
        if let Err(e) = hub.fanout_with_seats(
            &tick.input,
            &tick.snapshot,
            &acks,
            |peer, live| {
                crate::policy::seat::snapshot_for_viewer(
                    &archive,
                    &seats,
                    live,
                    peer,
                    clock.time_ms,
                )
            },
            Some(&mut queues.pending_svc),
            Some(&mut queues.pending_playercard),
            Some(&mut queues.pending_gamenotify),
            Some(&reliable),
            false,
        ) {
            let line = format!("udp fanout: {e}");
            diag::info!(Net, "{line}");
            diag.line = Some(line);
        }
    }
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

#[derive(Resource, Debug, Default)]
pub struct PendingScriptEntityNotifies {
    begins: Vec<ClientId>,
    spawned: Vec<ClientId>,
    aborts: Vec<ClientId>,
    ended: Vec<ClientId>,
}

impl PendingScriptEntityNotifies {
    fn store(
        &mut self,
        begins: Vec<ClientId>,
        spawned: Vec<ClientId>,
        aborts: Vec<ClientId>,
        ended: Vec<ClientId>,
    ) {
        self.begins = begins;
        self.spawned = spawned;
        self.aborts = aborts;
        self.ended = ended;
    }

    fn take(&mut self) -> Self {
        std::mem::take(self)
    }
}

pub(crate) fn flush_script_entity_notifies(
    mut commands: Commands,
    slots: Res<crate::CEntitySlots>,
    mut gaps: ResMut<NetIdentityGaps>,
    mut pending: ResMut<PendingScriptEntityNotifies>,
) {
    let pending = pending.take();
    trigger_script_entity_notifies(
        &mut commands,
        &slots,
        &mut gaps,
        pending.begins,
        pending.spawned,
        pending.aborts,
        pending.ended,
    );
}

fn trigger_script_entity_notifies(
    commands: &mut Commands,
    slots: &crate::CEntitySlots,
    gaps: &mut NetIdentityGaps,
    begins: Vec<ClientId>,
    spawned: Vec<ClientId>,
    aborts: Vec<ClientId>,
    ended: Vec<ClientId>,
) {
    let mut resolve = |client: ClientId, notify: ScriptNotify| match slots.entity_for_client(client)
    {
        Some(entity) => Some(entity),
        None => {
            gaps.raise(NetGapCause::NotifyClientHasNoEntity { client, notify });
            None
        }
    };
    for client in begins {
        if let Some(entity) = resolve(client, ScriptNotify::BeginKillcam) {
            commands.trigger(BeginKillcam { entity });
        }
    }
    for client in spawned {
        if let Some(entity) = resolve(client, ScriptNotify::SpawnedPlayer) {
            commands.trigger(SpawnedPlayer { entity });
        }
    }
    for client in aborts {
        if let Some(entity) = resolve(client, ScriptNotify::AbortKillcam) {
            commands.trigger(AbortKillcam { entity });
        }
    }
    for client in ended {
        if let Some(entity) = resolve(client, ScriptNotify::EndedKillcam) {
            commands.trigger(KillcamEnded { entity });
        }
    }
    gaps.report();
}

#[derive(Resource, Debug, Default, Clone)]
pub struct ScriptNotifyEmitStats {
    pub match_ending_soon: u32,
    pub match_ending_very_soon: u32,
    pub countdown_tick: u32,
    pub game_ended: u32,
    pub prematch_done: u32,
    pub game_win: u32,
    pub round_win: u32,
    pub round_switch: u32,
    pub spawn_music: u32,
    pub glass_destroyed: u32,
}

fn team_winner(winner: Option<gamemode_iw4::Team>) -> GameWinner {
    match winner {
        Some(gamemode_iw4::Team::Allies) => GameWinner::Allies,
        Some(gamemode_iw4::Team::Axis) => GameWinner::Axis,
        Some(gamemode_iw4::Team::Free) | None => GameWinner::Undefined,
    }
}

pub fn authority_bookkeeping(
    server_tick: Res<ServerTick>,
    mut archive: ResMut<FrameArchive>,
    mut world: ResMut<AuthorityWorld>,
    mut soon: MessageWriter<MatchEndingSoon>,
    mut very_soon: MessageWriter<MatchEndingVerySoon>,
    mut ended: MessageWriter<GameEnded>,
    mut prematch: MessageWriter<PrematchDone>,
    mut game_win: MessageWriter<GameWin>,
    mut round_win: MessageWriter<RoundWin>,
    mut round_switch: MessageWriter<RoundSwitchNotify>,
    mut spawned_music: MessageWriter<SpawnedPlayerNotify>,
    mut glass_destroyed: MessageWriter<GlassDestroyed>,
    mut stats: ResMut<ScriptNotifyEmitStats>,
    trace: Option<ResMut<AuthorityPhaseTrace>>,
) {
    push_phase(trace, "Bookkeeping");

    if let Some(tick) = server_tick.0.as_ref() {
        archive.push_snapshot(&tick.snapshot);
        if tick
            .snapshot
            .meta
            .journal
            .iter()
            .any(|r| matches!(r.event, SimEvent::MatchEnded { .. }))
        {
            ended.write(GameEnded);
            stats.game_ended = stats.game_ended.saturating_add(1);
        }
    }
    if world.0.take_prematch_done() {
        prematch.write(PrematchDone);
        stats.prematch_done = stats.prematch_done.saturating_add(1);
    }
    if let Some(winner) = world.0.take_game_win() {
        game_win.write(GameWin {
            winner: match winner {
                Some(id) => GameWinner::Player(id.0),
                None => GameWinner::Undefined,
            },
        });
        stats.game_win = stats.game_win.saturating_add(1);
    }
    if let Some(winner) = world.0.take_round_win() {
        round_win.write(RoundWin {
            winner: team_winner(winner),
        });
        stats.round_win = stats.round_win.saturating_add(1);
    }
    if let Some(halftime) = world.0.take_round_switch() {
        round_switch.write(RoundSwitchNotify {
            kind: if halftime {
                RoundSwitchKind::Halftime
            } else {
                RoundSwitchKind::Other
            },
        });
        stats.round_switch = stats.round_switch.saturating_add(1);
    }
    if let Some(winner) = world.0.take_team_game_win() {
        game_win.write(GameWin {
            winner: team_winner(winner),
        });
        stats.game_win = stats.game_win.saturating_add(1);
    }
    for id in world.0.take_spawn_music() {
        spawned_music.write(SpawnedPlayerNotify { client: id.0 });
        stats.spawn_music = stats.spawn_music.saturating_add(1);
    }
    for piece in world.0.take_glass_destroyed() {
        glass_destroyed.write(GlassDestroyed { piece });
        stats.glass_destroyed = stats.glass_destroyed.saturating_add(1);
    }
    let score_soon = world.0.take_score_limit_soon();
    let tick_emit = world.0.take_match_clock_emit();
    if score_soon.is_none() && tick_emit.is_none() {
        return;
    }
    if let Some(notify) = score_soon {
        match notify {
            gamemode_iw4::MatchSoundNotify::MatchEndingSoon(reason) => {
                soon.write(MatchEndingSoon {
                    reason: match reason {
                        GscEndingReason::Time => MatchEndingReason::Time,
                        GscEndingReason::Score => MatchEndingReason::Score,
                    },
                });
                stats.match_ending_soon = stats.match_ending_soon.saturating_add(1);
            }
            gamemode_iw4::MatchSoundNotify::MatchEndingVerySoon => {
                very_soon.write(MatchEndingVerySoon);
                stats.match_ending_very_soon = stats.match_ending_very_soon.saturating_add(1);
            }
        }
    }
    let Some(tick_emit) = tick_emit else {
        return;
    };
    let mut buf = [MatchSoundEmit::Notify(gamemode_iw4::MatchSoundNotify::MatchEndingVerySoon);
        gamemode_iw4::ClockTickEmit::MAX_EMITS];
    let n = tick_emit.emits(&mut buf);
    for emit in buf.iter().take(n) {
        match *emit {
            MatchSoundEmit::Notify(gamemode_iw4::MatchSoundNotify::MatchEndingSoon(reason)) => {
                soon.write(MatchEndingSoon {
                    reason: match reason {
                        GscEndingReason::Time => MatchEndingReason::Time,
                        GscEndingReason::Score => MatchEndingReason::Score,
                    },
                });
                stats.match_ending_soon = stats.match_ending_soon.saturating_add(1);
            }
            MatchSoundEmit::Notify(gamemode_iw4::MatchSoundNotify::MatchEndingVerySoon) => {
                very_soon.write(MatchEndingVerySoon);
                stats.match_ending_very_soon = stats.match_ending_very_soon.saturating_add(1);
            }
            MatchSoundEmit::EntitySound { .. } => {
                stats.countdown_tick = stats.countdown_tick.saturating_add(1);
            }
        }
    }
}

pub fn register_listen_runtime(app: &mut App) {
    register_script_notify(app);
    if *app.world().resource::<RuntimeRole>() != RuntimeRole::Client {
        app.init_resource::<AuthorityWorld>();
    }
    app.init_resource::<AuthorityInputGate>()
        .init_resource::<AuthorityLoadHold>()
        .init_resource::<AuthorityPhaseTrace>()
        .init_resource::<PendingAuthorityInput>()
        .init_resource::<PendingAcks>()
        .init_resource::<PendingStepResult>()
        .init_resource::<ServerTick>()
        .init_resource::<FrameArchive>()
        .init_resource::<ActiveKillcams>()
        .init_resource::<PendingDeathTimelines>()
        .init_resource::<ActiveKillcamSkips>()
        .init_resource::<crate::policy::end_game::PendingEndGameTail>()
        .init_resource::<NetDiagnostics>()
        .init_resource::<ClientShotSamples>()
        .init_resource::<ScriptNotifyEmitStats>()
        .init_resource::<ScriptKillcamEmitStats>()
        .init_resource::<NetIdentityGaps>()
        .init_resource::<crate::PendingSvcSounds>()
        .init_resource::<crate::PendingPlayerCard>()
        .init_resource::<crate::PendingGameNotify>()
        .init_resource::<crate::PendingScoreboard>()
        .init_resource::<FixedUpdateCensus>()
        .init_resource::<ListenFanoutCensus>()
        .init_resource::<DumpDeathLog>()
        .init_resource::<DumpGiveLog>()
        .init_resource::<DumpConfigurationChangeLog>()
        .init_resource::<LastAuthorityRoster>()
        .init_resource::<PendingConnectionFaults>();
    let role = *app.world().resource::<RuntimeRole>();
    if role != RuntimeRole::Dedicated {
        app.init_resource::<PendingScriptEntityNotifies>()
            .add_systems(
                FixedUpdate,
                (
                    begin_fixed_census
                        .before(AuthoritySet::Advance)
                        .before(frame::AuthorityEdge(0)),
                    advance_authority_clock.in_set(AuthoritySet::Advance),
                    ingress_authority.in_set(AuthoritySet::Ingress),
                    gather_authority_input.in_set(AuthoritySet::Gather),
                    step_authority.in_set(AuthoritySet::Step),
                    publish_server_tick.in_set(AuthoritySet::Snapshot),
                    fanout_loopback.in_set(AuthoritySet::Fanout),
                    apply_connection_faults
                        .in_set(AuthoritySet::Bookkeeping)
                        .before(retire_departed_peers),
                    retire_departed_peers.in_set(AuthoritySet::Bookkeeping),
                    authority_bookkeeping.in_set(frame::AuthorityBookkeeping),
                    end_fixed_census.after(AuthoritySet::Bookkeeping),
                )
                    .run_if(authority_should_tick),
            );
        app.add_systems(
            Update,
            flush_script_entity_notifies
                .in_set(ClientSet::Reconcile)
                .after(crate::client::entities::sync_client_entities),
        );
    } else {
        app.add_systems(
            FixedUpdate,
            (
                begin_fixed_census
                    .before(AuthoritySet::Advance)
                    .before(frame::AuthorityEdge(0)),
                advance_authority_clock.in_set(AuthoritySet::Advance),
                ingress_authority.in_set(AuthoritySet::Ingress),
                gather_authority_input.in_set(AuthoritySet::Gather),
                step_authority.in_set(AuthoritySet::Step),
                publish_server_tick.in_set(AuthoritySet::Snapshot),
                fanout_loopback
                    .in_set(AuthoritySet::Fanout)
                    .after(crate::client::entities::sync_authority_entities),
                apply_connection_faults
                    .in_set(AuthoritySet::Bookkeeping)
                    .before(retire_departed_peers),
                retire_departed_peers.in_set(AuthoritySet::Bookkeeping),
                authority_bookkeeping.in_set(frame::AuthorityBookkeeping),
                end_fixed_census.after(AuthoritySet::Bookkeeping),
            )
                .run_if(authority_should_tick),
        );
        crate::client::entities::register_authority_entities(app);
    }
    app.init_resource::<AuthorityPhaseCensus>().add_systems(
        RunFixedMainLoop,
        (publish_fixed_census, publish_authority_phase_census)
            .in_set(RunFixedMainLoopSystems::AfterFixedMainLoop),
    );
    app.add_systems(
        Update,
        reset_authority_on_match_torn_down
            .in_set(ClientSet::Load)
            .after(frame::SessionSwapApplied),
    );
    register_authority_phase_seams(app);
}
