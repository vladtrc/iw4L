use frame::ScopeApp;
use input_iw4::{AdjustAnglesInput, CL_ANGLESPEEDKEY_DEFAULT, CL_PITCHSPEED_DEFAULT};
use std::collections::{HashMap, VecDeque};
use std::sync::Arc;

use bevy::prelude::*;
use entity_iw4::EntityEventKind;
use frame::RuntimeRole;
use playerstate_iw4::UserCmd;

use crate::ServerTime;
use crate::authority::inbox::{
    AUTHORITY_MS, ClientActionInbox, ClientCommandInbox, MAX_REDUNDANT_CMDS,
};
use crate::authority::runtime::{AuthorityInputGate, AuthorityWorld, ClientShotSamples};
use crate::client::frame_clock::{FrameClock, GameActive, GameJoinCensus};
use crate::client::input::{
    ClientActionInput, LookState, build_usercmd, frame_time_msec, key_frame_msec,
    remote_control_axes,
};
use crate::client::predict::{ClientPrediction, CmdSeq};
use crate::client::presentation::presented::{
    LocalPresentClient, PresentLocalCensus, PresentedSnapshot, interpolate_player_state,
    keep_interpolated_player_clear,
};
use crate::client::proxy::{ProxySample, RemoteProxy};
use crate::client::realtime::ClientRealtime;
use crate::schedule::ClientSet;
use crate::transport::loopback_live::{ListenLoopback, ReceivedTick};
use sim::Snapshot;

#[derive(Resource)]
pub struct ClientPredictionState(pub ClientPrediction);

use crate::{ClientCollision, ClientLevel};

#[derive(Resource, Default)]
pub struct PendingPresentedEntityEvents {
    pub live: Vec<sim::EntityEventRecord>,

    pub archived: Vec<sim::EntityEventRecord>,

    pub in_killcam: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct ImpactOccurrence {
    pub tick: sim::Tick,
    pub cause: Option<sim::FireCause>,
    pub attacker: i32,
    pub correlation: u32,
    pub pellet: u16,
    pub segment: u16,
}

#[derive(Message, Clone, Copy, Debug)]
pub struct AcceptedImpact {
    pub occurrence: ImpactOccurrence,
    pub record: sim::PelletFxRecord,
}

#[derive(Message, Clone, Copy, Debug)]
pub struct ImpactReconstruction;

#[derive(Resource, Default)]
pub struct PendingAcceptedImpacts(Vec<(sim::Tick, sim::PelletFxRecord)>);

#[derive(Resource, Default)]
pub(crate) struct ImpactCursor {
    replay_tick: Option<sim::Tick>,
    seen: std::collections::HashSet<ImpactOccurrence>,
    order: VecDeque<ImpactOccurrence>,
}

pub(crate) fn publish_accepted_impacts(
    mut pending: ResMut<PendingAcceptedImpacts>,
    mut cursor: ResMut<ImpactCursor>,
    mut impacts: MessageWriter<AcceptedImpact>,
    presented: Res<PresentedSnapshot>,
    mut visuals: ResMut<PendingPelletFx>,
    proxy: Res<RemoteProxyState>,
    mut reconstruction: MessageWriter<ImpactReconstruction>,
) {
    let mut accepted = std::mem::take(&mut pending.0);
    if presented.is_archived_replay()
        && let Some(snapshot) = presented.snapshot()
    {
        let (gap, records) = proxy.0.impact_window(cursor.replay_tick, snapshot.tick);
        if gap.is_some() {
            cursor.seen.clear();
            cursor.order.clear();
            reconstruction.write(ImpactReconstruction);
        }
        if records.is_empty() && cursor.replay_tick != Some(snapshot.tick) {
            accepted.extend(
                snapshot
                    .meta
                    .pellet_fx
                    .iter()
                    .copied()
                    .map(|record| (snapshot.tick, record)),
            );
        } else {
            accepted.extend(records);
        }
        cursor.replay_tick = Some(snapshot.tick);
    }
    for (tick, record) in accepted {
        if record.normal == [0.0; 3] {
            continue;
        }
        let occurrence = ImpactOccurrence {
            tick: if record.fire_cause.is_some() {
                sim::Tick(0)
            } else {
                tick
            },
            cause: record.fire_cause,
            attacker: record.attacker,
            correlation: record.correlation,
            pellet: record.pellet,
            segment: record.segment,
        };
        if !cursor.seen.insert(occurrence) {
            continue;
        }
        cursor.order.push_back(occurrence);
        while cursor.order.len() > 4096 {
            if let Some(old) = cursor.order.pop_front() {
                cursor.seen.remove(&old);
            }
        }
        if presented.is_archived_replay() {
            visuals.0.push(record);
        }
        impacts.write(AcceptedImpact { occurrence, record });
    }
    let excess = visuals.0.len().saturating_sub(8192);
    visuals.0.drain(..excess);
}

#[derive(Resource, Default)]
pub struct PendingPelletFx(pub(crate) Vec<sim::PelletFxRecord>);

impl PendingPelletFx {
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
    pub fn take(&mut self) -> Vec<sim::PelletFxRecord> {
        core::mem::take(&mut self.0)
    }
}

fn collect_received_entity_events(
    last_tick: Option<sim::Tick>,
    local: sim::ClientId,
    snapshot: &mut Snapshot,
    pending: &mut PendingPresentedEntityEvents,
) -> bool {
    if last_tick.is_some_and(|last| snapshot.tick.0 <= last.0) {
        return false;
    }
    pending.in_killcam = snapshot
        .meta
        .for_client(local)
        .is_some_and(|m| m.killcam_hud.is_some());
    if pending.in_killcam {
        pending.archived.append(&mut snapshot.meta.entity_events);
    } else {
        pending.live.append(&mut snapshot.meta.entity_events);
    }
    true
}

impl Default for ClientPredictionState {
    fn default() -> Self {
        Self(ClientPrediction::new(sim::ClientId(0)))
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ClockTick {
    pub released: u32,

    pub dropped: u32,
}

#[derive(Resource, Clone, Copy, Debug, Default)]
pub struct ClientClock {
    pub accumulator_ms: f32,

    pub dropped_slots: u32,
}

impl ClientClock {
    pub fn tick(&mut self, delta_ms: f32) -> ClockTick {
        let before = self.accumulator_ms;
        self.accumulator_ms += delta_ms;
        if self.accumulator_ms < AUTHORITY_MS as f32 {
            return ClockTick {
                released: 0,
                dropped: 0,
            };
        }
        let slots = (self.accumulator_ms / AUTHORITY_MS as f32).floor() as u32;
        let dropped = slots.saturating_sub(1);
        self.dropped_slots = self.dropped_slots.wrapping_add(dropped);
        self.accumulator_ms %= AUTHORITY_MS as f32;

        if self.accumulator_ms >= AUTHORITY_MS as f32 - 0.0005 {
            self.accumulator_ms = 0.0;
        }
        debug_assert!(
            self.accumulator_ms >= 0.0 && self.accumulator_ms < AUTHORITY_MS as f32,
            "clock debt after tick: before={before} dt={delta_ms} after={}",
            self.accumulator_ms
        );
        ClockTick {
            released: 1,
            dropped,
        }
    }

    pub fn debt_ms(&self) -> f32 {
        self.accumulator_ms
    }
}

#[derive(Resource, Default)]
pub struct ReceivedTicks(pub VecDeque<ReceivedTick>);

#[derive(Resource, Default, Debug)]
pub struct LastAdoptedSnapshot {
    pub snap: Option<Arc<Snapshot>>,
    pub next_snap: Option<Arc<Snapshot>>,

    pub applied_this_frame: bool,
    pub(crate) prediction_ack: Option<CmdSeq>,

    pub adopted_at: Option<std::time::Instant>,
}

impl LastAdoptedSnapshot {
    pub fn since_adopted(&self) -> Option<std::time::Duration> {
        self.adopted_at.map(|at| at.elapsed())
    }

    pub fn next(&self) -> Option<&Snapshot> {
        self.next_snap.as_deref()
    }

    pub fn sound_alias_name(&self, index: u8) -> Option<&str> {
        for snap in [self.next_snap.as_ref(), self.snap.as_ref()]
            .into_iter()
            .flatten()
        {
            if let Some((_, name)) = snap.meta.sound_aliases.iter().find(|(i, _)| *i == index) {
                return Some(name.as_str());
            }
        }
        None
    }

    pub fn effect_name(&self, index: u8) -> Option<&str> {
        self.next().and_then(|snap| {
            snap.meta
                .effect_names
                .iter()
                .find(|(i, _)| *i == index)
                .map(|(_, name)| name.as_str())
        })
    }
}

#[derive(Resource, Default, Debug, Clone)]
pub struct PendingClientSends {
    cmds: VecDeque<(CmdSeq, UserCmd, sim::ShotSampleProvenance)>,

    evicted_unacked: u32,

    acknowledged: Option<CmdSeq>,
}

impl PendingClientSends {
    pub fn len(&self) -> usize {
        self.cmds.len()
    }

    pub fn is_empty(&self) -> bool {
        self.cmds.is_empty()
    }

    pub fn push(&mut self, seq: CmdSeq, cmd: UserCmd, sample: sim::ShotSampleProvenance) {
        if self.cmds.len() >= MAX_REDUNDANT_CMDS {
            self.evicted_unacked = self.evicted_unacked.saturating_add(1);
            return;
        }
        self.cmds.push_back((seq, cmd, sample));
    }

    pub fn evicted_unacked(&self) -> u32 {
        self.evicted_unacked
    }

    pub fn clear_acknowledged(&mut self, acked: CmdSeq) {
        self.acknowledged = Some(self.acknowledged.map_or(acked, |last| last.max(acked)));
        while self
            .cmds
            .front()
            .is_some_and(|(seq, _, _)| seq.0 <= acked.0)
        {
            self.cmds.pop_front();
        }
    }

    pub fn iter(&self) -> impl Iterator<Item = &(CmdSeq, UserCmd, sim::ShotSampleProvenance)> {
        self.cmds.iter()
    }

    pub fn has_command_capacity(&self, server_time: i32) -> bool {
        if self.len() >= MAX_REDUNDANT_CMDS {
            return false;
        }
        // Reserve one full step for the gap from the authority's last consumed
        // command. Bound unacknowledged work before allocating a sequence: after
        // a delayed ACK we can resume without dropping commands or leaving holes.
        let step = crate::authority::inbox::COMMAND_TIME_STEP_MS;
        let mut work = step;
        let mut previous: Option<i32> = None;
        for time in self
            .cmds
            .iter()
            .map(|(_, cmd, _)| cmd.server_time)
            .chain(std::iter::once(server_time))
        {
            if let Some(previous) = previous {
                work = work.saturating_add(time.saturating_sub(previous).clamp(0, step));
            }
            previous = Some(time);
        }
        work <= crate::authority::inbox::MAX_QUEUED_COMMAND_MS
    }

    pub fn oldest_seq(&self) -> Option<CmdSeq> {
        self.cmds.front().map(|(seq, _, _)| *seq)
    }

    pub fn newest_seq(&self) -> Option<CmdSeq> {
        self.cmds.back().map(|(seq, _, _)| *seq)
    }
}

#[derive(Resource, Default)]
pub struct ClientCmdTemplate {
    pub cmd: UserCmd,
    pub ready: bool,
}

#[derive(Resource, Default, Debug, Clone, Copy, PartialEq, Eq)]
pub struct WeaponSelect {
    pub index: u32,

    pub time: i32,

    pub life_sequence: u32,

    pub held_index: u32,

    pub last_primary: u32,
    pub mapped_index: u32,
}

pub fn follow_held_weapon_select(
    select: &mut WeaponSelect,
    ps_weapon: u32,
    life_sequence: u32,
    cg_time: i32,
) {
    if ps_weapon == 0 {
        return;
    }
    if select.index == 0
        || select.life_sequence != life_sequence
        || (select.index == select.held_index && ps_weapon != select.held_index)
    {
        select.index = ps_weapon;
        select.mapped_index = ps_weapon;
        select.last_primary = 0;
        select.life_sequence = life_sequence;
        select.time = cg_time;
    }
    select.held_index = ps_weapon;
}

pub fn cycle_weapon_select(
    select: &mut WeaponSelect,
    input: &mut input_iw4::ClientInput,
    ps: &playerstate_iw4::PlayerState,
    world: &sim::SimContent,
    time: i32,
    next: bool,
) {
    use input_iw4::weapon_select::{cycle_weapon, weapon_cycle_allowed};
    if !weapon_cycle_allowed(ps, time, select.time, 0, 0) {
        return;
    }

    if (0x10..0x13).contains(&ps.weaponstate_primary) {
        let offhand = ps.off_hand_index.max(0) as u32;
        let cancelable = world
            .weapon_combat_row(offhand)
            .unwrap_or_else(weapon_iw4::WeaponCombatFacts::none)
            .offhand_hold_is_cancelable
            .unwrap_or(false);
        if cancelable {
            input.offhand_hold_cancel = true;
            return;
        }
    }
    select.time = time;
    if world.weapon_combat_len() < 2 {
        return;
    }
    let facts = |weapon| {
        world
            .weapon_combat_row(weapon)
            .expect("owned weapon must have a captured catalog row")
    };
    let Some(target) = cycle_weapon(
        &ps.weapons,
        select.index,
        select.mapped_index,
        select.last_primary,
        next,
        |weapon| facts(weapon).inventory_type,
    ) else {
        return;
    };
    let target_facts = facts(target);
    let requires_ammo = target_facts.select_requires_ammo.unwrap_or(false);
    if requires_ammo {
        let ammo_key = weapon_iw4::ammo_table_key(target_facts.ammo_index, target);
        let clip_key = weapon_iw4::clip_table_key(target_facts.clip_index, target);
        if weapon_iw4::get_ammo_player_both_clips(ps, target, ammo_key, clip_key) == 0 {
            return;
        }
    }
    if select.index != target {
        if select.index != 0 && facts(select.index).inventory_type == 0 {
            select.last_primary = select.index;
        }
        select.index = target;
        select.mapped_index = target;
        input_iw4::set_ads(input, false);
    }
}

pub fn listen_prediction_needs_content(
    pred: &ClientPrediction,
    level: &ClientLevel,
    collision: &ClientCollision,
    authority: &sim::SimWorld,
) -> bool {
    if !authority.has_world_clip() {
        return false;
    }
    if !pred.is_armed() || !collision.0.has_world_clip() {
        return true;
    }
    if level.content_digest != authority.content_digest() {
        return true;
    }
    authority.pen_table_loaded() && !level.content.pen_table_loaded()
}

pub fn arm_listen_prediction(
    role: Res<RuntimeRole>,
    authority: Option<Res<AuthorityWorld>>,
    mut prediction: ResMut<ClientPredictionState>,
    mut level: ResMut<ClientLevel>,
    mut collision: ResMut<ClientCollision>,
) {
    if !matches!(*role, RuntimeRole::Listen | RuntimeRole::Client) {
        return;
    }
    let Some(authority) = authority else {
        return;
    };
    if authority.0.input_program_owns(prediction.0.local()) {
        if prediction.0.is_armed() {
            prediction.0.disarm();
        }
        return;
    }
    if !listen_prediction_needs_content(&prediction.0, &level, &collision, &authority.0) {
        return;
    }
    prediction.0.arm();
    *level = ClientLevel::from_world(&authority.0);
    collision.0 = authority.0.collision_scene();
}

pub fn advance_cls_realtime(mut cls: ResMut<ClientRealtime>, time: Res<Time<Real>>) {
    cls.advance_listen(time.delta_secs());
}

#[derive(bevy::ecs::system::SystemParam)]
pub struct ClientFrameTiming<'w> {
    clock: ResMut<'w, FrameClock>,
    prediction: ResMut<'w, ClientPredictionState>,
    join: ResMut<'w, GameJoinCensus>,
    cgame_active: Res<'w, GameActive>,
}

impl ClientFrameTiming<'_> {
    fn advance(
        mut self,
        client_clock: Res<ClientClock>,
        time: Res<Time>,
        authority: Option<Res<crate::AuthorityClock>>,
        role: Res<RuntimeRole>,
        fixed: Res<Time<Fixed>>,
        adopted: Option<Res<LastAdoptedSnapshot>>,
    ) {
        let was_started = self.clock.started();
        let adopted_tick = adopted.as_ref().and_then(|a| a.next().map(|s| s.tick));

        let local_authority = authority.as_ref().filter(|_| role.runs_authority());
        let server_time = local_authority
            .map(|clock| ServerTime::from_ms(clock.time_ms))
            .or_else(|| adopted_tick.map(ServerTime::from_tick));
        if *role == RuntimeRole::Listen && self.cgame_active.0 && was_started {
            let server = authority.as_ref().expect("listen authority clock").time_ms;
            self.clock
                .assign_server_time(server.wrapping_add(fixed.overstep().as_millis() as i32));
        } else if *role == RuntimeRole::Replay {
            if self.cgame_active.0
                && let Some(server) = server_time
            {
                self.clock.assign_server_time(
                    server
                        .ms()
                        .wrapping_add(AUTHORITY_MS)
                        .wrapping_add(client_clock.accumulator_ms as i32),
                );
            }
        } else if *role == RuntimeRole::Client {
            self.clock
                .advance_remote(time.delta_secs(), self.cgame_active.0, server_time);
        } else {
            self.clock
                .advance_listen(time.delta_secs(), self.cgame_active.0, server_time);
        }
        if self.clock.started() && !was_started {
            self.join.latch(
                server_time.map(ServerTime::ms).unwrap_or(0),
                local_authority
                    .map(|c| c.tick)
                    .or_else(|| adopted_tick.map(|t| t.0))
                    .unwrap_or(0),
                (time.elapsed_secs() * 1000.0) as i32,
            );
        }
        self.prediction
            .0
            .predicted_error_mut()
            .sync_frame(&self.clock);
    }
}

pub fn advance_cg_frame_clock(
    timing: ClientFrameTiming,
    client_clock: Res<ClientClock>,
    time: Res<Time>,
    authority: Option<Res<crate::AuthorityClock>>,
    role: Res<RuntimeRole>,
    fixed: Res<Time<Fixed>>,
    adopted: Option<Res<LastAdoptedSnapshot>>,
) {
    timing.advance(client_clock, time, authority, role, fixed, adopted);
}

#[derive(Default)]
pub struct JoinLinkWatch {
    connected: bool,
}

fn select_timeline(
    received: Res<ReceivedTicks>,
    adopted: Res<LastAdoptedSnapshot>,
    local: Res<LocalPresentClient>,
    current: Res<State<frame::Timeline>>,
    mut next: ResMut<NextState<frame::Timeline>>,
) {
    let Some(tick) = received.0.iter().rev().find(|tick| {
        adopted
            .next()
            .is_none_or(|old| tick.snapshot.tick.0 > old.tick.0)
    }) else {
        return;
    };
    let snapshot = &tick.snapshot;
    let selected = if snapshot
        .meta
        .for_client(local.0)
        .is_some_and(|meta| meta.killcam_hud.is_some())
    {
        frame::Timeline::Killcam
    } else {
        frame::Timeline::Live
    };
    let rewind = adopted.next().is_some_and(|old| {
        snapshot.view_tick(local.0).0 < old.view_tick(local.0).0
            || snapshot.meta.world_objects.map_round_epoch != old.meta.world_objects.map_round_epoch
    });
    if *current.get() != selected || rewind {
        perf::instant!("timeline_transition", "iw4l.sim", |ctx| {
            ctx.arg("from", *current.get() as i64);
            ctx.arg("to", selected as i64);
        });
        next.set(selected);
    }
}

#[derive(bevy::ecs::system::SystemParam)]
pub struct ClientAccountSync<'w> {
    account: Option<ResMut<'w, crate::LocalAccount>>,
    receipt: Option<Res<'w, crate::AccountSaveReceipt>>,
    level: Res<'w, ClientLevel>,
}

impl ClientAccountSync<'_> {
    fn poll(
        &mut self,
        link: &mut crate::transport::udp_session::UdpClientLink,
        identity: Option<crate::SessionIdentity>,
    ) -> Result<(), String> {
        link.poll_accounts(
            identity,
            self.account.as_deref_mut(),
            &self.level.schemas,
            self.receipt.as_deref(),
        )
    }
}

#[derive(bevy::ecs::system::SystemParam)]
pub struct ClientLinkIdentity<'w, 's> {
    local: ResMut<'w, LocalPresentClient>,
    prediction: ResMut<'w, ClientPredictionState>,
    watch: Local<'s, JoinLinkWatch>,
}

impl ClientLinkIdentity<'_, '_> {
    fn adopt(&mut self, link: &crate::transport::udp_session::UdpClientLink) {
        if link.connection.is_some() && !self.watch.connected {
            self.watch.connected = true;
            diag::info!(
                Net,
                "udp client accepted as client {}",
                link.assigned_client.map(|c| c.0).unwrap_or(0)
            );
        }

        if let Some(id) = link.assigned_client {
            if self.local.0 != id {
                self.local.0 = id;
                self.prediction.0.set_local(id);
            }
        }
    }
}

pub fn receive_ticks(
    mut accounts: ClientAccountSync,
    mut link: Option<ResMut<crate::transport::udp_session::UdpClientLink>>,
    loopback: Option<ResMut<ListenLoopback>>,
    mut received: ResMut<ReceivedTicks>,
    adopted: Res<LastAdoptedSnapshot>,
    mut identity: ClientLinkIdentity,
    mut reliable: ReliableInbound,
) {
    if let Some(link) = link.as_mut() {
        match link.recv_ticks() {
            Ok(ticks) => {
                for tick in ticks {
                    received.0.push_back(tick);
                }
            }
            Err(e) => {
                if link.handshake_reject().is_some() {
                    diag::error!(Net, "udp recv: {e}");
                } else {
                    diag::warn!(Net, "udp recv: {e}");
                }
            }
        }
        if let Err(error) = accounts.poll(
            link,
            reliable
                .bridge
                .as_ref()
                .map(|bridge| bridge.state().identity()),
        ) {
            reliable.fail(&error);
        }
        for payload in link.take_controls() {
            reliable.apply(&payload, |index| adopted.sound_alias_name(index).is_some());
        }
        identity.adopt(link);
        return;
    }
    if let Some(mut loopback) = loopback {
        let mut drained = Vec::new();
        let outcome = loopback.recv_all(&mut drained);
        for tick in drained {
            received.0.push_back(tick);
        }
        if let Err(e) = outcome {
            diag::warn!(Net, "client_runtime: recv_all failed: {e}");
        }
    }
}

#[derive(Resource, Default)]
pub struct RemoteProxyState(pub RemoteProxy);

#[derive(bevy::ecs::system::SystemParam)]
pub struct SnapshotEvents<'w> {
    entity_events: ResMut<'w, PendingPresentedEntityEvents>,
    pellet_fx: ResMut<'w, PendingPelletFx>,
    accepted: ResMut<'w, PendingAcceptedImpacts>,
}

impl SnapshotEvents<'_> {
    fn collect_entities(
        &mut self,
        last_tick: Option<sim::Tick>,
        local: sim::ClientId,
        snapshot: &mut Snapshot,
    ) -> bool {
        collect_received_entity_events(last_tick, local, snapshot, &mut self.entity_events)
    }

    fn publish_impacts(&mut self, snapshot: &Snapshot, role: RuntimeRole, local: sim::ClientId) {
        self.accepted.0.extend(
            snapshot
                .meta
                .pellet_fx
                .iter()
                .copied()
                .map(|record| (snapshot.tick, record)),
        );
        self.pellet_fx.0.extend(
            snapshot
                .meta
                .pellet_fx
                .iter()
                .filter(|record| {
                    role == RuntimeRole::Replay
                        || record.attacker != local.0 as i32
                        || u32::from(record.surf_type) == weapon_iw4::SURF_TYPE_FLESH
                })
                .copied(),
        );
    }
}

#[derive(bevy::ecs::system::SystemParam)]
pub struct ClientSnapshotAdoption<'w> {
    received: ResMut<'w, ReceivedTicks>,
    last_adopted: ResMut<'w, LastAdoptedSnapshot>,
    proxy: ResMut<'w, RemoteProxyState>,
    pending: ResMut<'w, PendingClientSends>,
    collision: ResMut<'w, ClientCollision>,
}

impl ClientSnapshotAdoption<'_> {
    fn reconcile(
        mut self,
        mut prediction: ResMut<ClientPredictionState>,
        local: Res<LocalPresentClient>,
        mut reliable: ReliableInbound,
        mut events: SnapshotEvents,
        role: Res<RuntimeRole>,
        cg_clock: Res<FrameClock>,
    ) {
        let render_time_ms = (*role != RuntimeRole::Replay && cg_clock.started())
            .then(|| cg_clock.time().saturating_add(cg_clock.frametime()));
        self.last_adopted.applied_this_frame = false;
        while let Some(mut tick) = self.received.0.pop_front() {
            if !events.collect_entities(
                self.last_adopted.next().map(|last| last.tick),
                local.0,
                &mut tick.snapshot,
            ) {
                continue;
            }
            perf::instant!("net_leg", "iw4l.sim", |ctx| {
                let (leg, key, bytes): (&str, u32, u32) = ("snap_adopt", tick.snapshot.tick.0, 0);
                ctx.arg("leg", leg);
                ctx.arg("key", i64::from(key));
                ctx.arg("bytes", i64::from(bytes));
            });
            let match_key = crate::signon::live_match_key(reliable.bridge.as_deref());
            let same_match = if match_key.is_none() {
                matches!(*role, RuntimeRole::Listen | RuntimeRole::Replay)
            } else {
                tick.snapshot.meta.world_objects.map_round_epoch == match_key.match_epoch
            };
            if same_match {
                events.publish_impacts(&tick.snapshot, *role, local.0);
            }
            reliable.apply(&tick.frame.reliable, |index| {
                sim::name_in_occupied(&tick.snapshot.meta.sound_aliases, index).is_some()
            });
            let ack = tick.ack_for(local.0);
            if let Some(acked) = ack {
                self.pending.clear_acknowledged(acked);
            }
            self.last_adopted.applied_this_frame = true;
            self.last_adopted.adopted_at = Some(std::time::Instant::now());
            if let Some(old_next) = self.last_adopted.next_snap.take() {
                self.last_adopted.snap = Some(old_next);
            }
            let snap = Arc::new(tick.snapshot);
            self.proxy.0.push_arc(Arc::clone(&snap), render_time_ms);
            perf::instant!("net_leg", "iw4l.sim", |ctx| {
                let (leg, key, bytes): (&str, u32, u32) =
                    ("proxy_delay", snap.tick.0, self.proxy.0.delay_ms() as u32);
                ctx.arg("leg", leg);
                ctx.arg("key", i64::from(key));
                ctx.arg("bytes", i64::from(bytes));
            });
            self.last_adopted.next_snap = Some(snap);
            let newest = !self.received.0.iter().any(|pending| {
                pending.snapshot.tick.0 > self.last_adopted.next().expect("next snapshot").tick.0
            });
            if newest {
                self.last_adopted.prediction_ack = ack;
                self.collision
                    .0
                    .adopt_snapshot(self.last_adopted.next().expect("next_snap just stored"));
            } else {
                prediction.0.retire_acks(ack);
            }
            for cmd in tick.frame.svc_sounds {
                reliable.svc.sound.write(crate::SvcLocalSound {
                    stop: cmd.stop,
                    index: cmd.index,
                });
            }
            for cmd in tick.frame.svc_card_slots {
                reliable.svc.card.write(crate::SvcCardSlotCmd {
                    client: cmd.client,
                    slot: cmd.slot,
                });
            }
            for cmd in tick.frame.svc_hud_splashes {
                reliable.svc.splash.write(cmd);
            }
            for cmd in tick.frame.svc_game_notifies {
                reliable.svc.notify.write(cmd);
            }
            for cmd in tick.frame.svc_open_menus {
                reliable.svc.menu.write(crate::SvcOpenMenuCmd {
                    cs_index: cmd.cs_index,
                });
            }
            if let Some(cmd) = tick.frame.svc_scores {
                reliable.scores.parsed = crate::parse_scoreboard_cmd(&cmd);
                reliable.scores.cmd = Some(cmd);
            }
        }
    }
}

pub fn reconcile_prediction(
    adoption: ClientSnapshotAdoption,
    prediction: ResMut<ClientPredictionState>,
    local: Res<LocalPresentClient>,
    reliable: ReliableInbound,
    events: SnapshotEvents,
    role: Res<RuntimeRole>,
    cg_clock: Res<FrameClock>,
) {
    adoption.reconcile(prediction, local, reliable, events, role, cg_clock);
}

#[derive(bevy::prelude::Message, Clone, Copy, Debug)]
pub struct ReliableControlEvent(pub sim::SimEvent);

#[derive(bevy::ecs::system::SystemParam)]
pub struct SvcFrameWriters<'w> {
    sound: MessageWriter<'w, crate::SvcLocalSound>,
    audio: MessageWriter<'w, crate::SvcScriptAudio>,
    card: MessageWriter<'w, crate::SvcCardSlotCmd>,
    menu: MessageWriter<'w, crate::SvcOpenMenuCmd>,
    splash: MessageWriter<'w, crate::SvcHudSplash>,
    notify: MessageWriter<'w, crate::SvcGameNotify>,
}

#[derive(bevy::ecs::system::SystemParam)]
pub struct ReliableInbound<'w> {
    actions: ResMut<'w, ClientActionInbox>,
    ack: ResMut<'w, ClientReliableAck>,
    events: MessageWriter<'w, ReliableControlEvent>,

    svc: SvcFrameWriters<'w>,
    scores: ResMut<'w, crate::Scoreboard>,
    signon: ResMut<'w, crate::SignonState>,
    bridge: Option<Res<'w, crate::MasterBridge>>,
}

impl ReliableInbound<'_> {
    fn fail(&mut self, reason: &str) {
        self.actions.clear();
        let match_key = crate::signon::live_match_key(self.bridge.as_deref());
        if let Some(bridge) = &self.bridge {
            bridge.fail(reason);
        }
        self.signon.set_phase(crate::SignonPhase::Failed(
            crate::SignonFailReason::Transport {
                source: reason.to_owned(),
                stage: crate::FailStage::Transport,
                match_key,
            },
        ));
    }

    fn apply(&mut self, payload: &crate::ReliablePayload, sound_ready: impl Fn(u8) -> bool) {
        if payload.dropped_oldest != 0 {
            self.fail("ReliableControlOverflow: outcome unknown");
            return;
        }
        if let Some(reason) = payload.rows.iter().find_map(|(_, row)| {
            if let crate::ReliableRow::Failure(reason) = row {
                Some(reason)
            } else {
                None
            }
        }) {
            self.fail(reason);
            return;
        }
        let mut rows: Vec<&(u16, crate::ReliableRow)> = payload.rows.iter().collect();
        rows.sort_by_key(|(seq, _)| seq.wrapping_sub(self.ack.0));
        for (seq, row) in rows {
            if !reliable_seq_after(*seq, self.ack.0) {
                continue;
            }
            match row {
                crate::ReliableRow::Failure(_) => unreachable!("terminal handled before sequence"),
                crate::ReliableRow::ScriptAudio(cmd) => {
                    self.svc.audio.write(crate::SvcScriptAudio(cmd.clone()));
                }
                crate::ReliableRow::Sound(cmd) => {
                    // Reliable controls can overtake the snapshot that publishes
                    // this configuration string. Keep the row unacknowledged so
                    // retransmission preserves it until its dependency is ready.
                    if !sound_ready(cmd.index) {
                        break;
                    }
                    self.svc.sound.write(crate::SvcLocalSound {
                        stop: cmd.stop,
                        index: cmd.index,
                    });
                }
                crate::ReliableRow::Card(cmd) => {
                    self.svc.card.write(crate::SvcCardSlotCmd {
                        client: cmd.client,
                        slot: cmd.slot,
                    });
                }
                crate::ReliableRow::Menu(cmd) => {
                    self.svc.menu.write(crate::SvcOpenMenuCmd {
                        cs_index: cmd.cs_index,
                    });
                }
                crate::ReliableRow::Splash(cmd) => {
                    self.svc.splash.write(cmd.clone());
                }
                crate::ReliableRow::Notify(cmd) => {
                    self.svc.notify.write(cmd.clone());
                }
                crate::ReliableRow::Scores(cmd) => {
                    self.scores.parsed = crate::parse_scoreboard_cmd(cmd);
                    self.scores.cmd = Some(cmd.clone());
                }
                crate::ReliableRow::Event(event) => {
                    self.events.write(ReliableControlEvent(*event));
                }
            }
            self.ack.0 = *seq;
        }
    }
}

fn reliable_seq_after(a: u16, b: u16) -> bool {
    a != b && a.wrapping_sub(b) < 0x8000
}

fn apply_weapon_switch_requests(
    mut events: MessageReader<ReliableControlEvent>,
    mut select: ResMut<WeaponSelect>,
    clock: Res<FrameClock>,
) {
    for event in events.read() {
        if let sim::SimEvent::WeaponSwitchRequested { weapon } = event.0 {
            select.index = weapon;
            select.mapped_index = weapon;
            select.time = clock.time();
        }
    }
}

fn use_reload_as_reload(
    actions: &mut ClientActionInput,
    cmd: &mut playerstate_iw4::UserCmd,
    ps: Option<&playerstate_iw4::PlayerState>,
) {
    use playerstate_iw4::buttons::{RELOAD, USE_RELOAD};
    let down = cmd.buttons & USE_RELOAD != 0;
    let now = actions.now_msec;
    let item = ps
        .filter(|ps| ps.cursor_hint != 0)
        .map(|ps| ps.cursor_hint_ent_index);
    if down {
        if !actions.use_reload_down {
            actions.use_reload_over_item = item.map(|ent| (now, ent));
        }
        if actions.use_reload_over_item.is_none() && item.is_none() {
            cmd.buttons |= RELOAD;
        }
    } else if let Some((since, ent)) = actions.use_reload_over_item.take()
        && now.wrapping_sub(since) < sim::ITEM_USE_HOLD_MS
        && item == Some(ent)
    {
        cmd.buttons |= RELOAD;
    }
    actions.use_reload_down = down;
}

#[derive(bevy::ecs::system::SystemParam)]
pub struct LocalCommandSource<'w> {
    presentation: crate::client::presentation::presented::LocalPresentation<'w>,
    prediction: Res<'w, ClientPredictionState>,
    gate: Res<'w, AuthorityInputGate>,
    authority: Option<Res<'w, AuthorityWorld>>,
}

impl LocalCommandSource<'_> {
    fn enabled(&self) -> bool {
        self.gate.local_cmds_enabled
            && !self
                .authority
                .as_ref()
                .is_some_and(|a| a.0.input_program_owns(self.client_id()))
    }

    fn client_id(&self) -> sim::ClientId {
        self.presentation.client_id()
    }

    fn player(&self) -> Option<&playerstate_iw4::PlayerState> {
        self.prediction
            .0
            .predicted_local()
            .or_else(|| self.presentation.player())
    }

    fn snapshot(&self) -> Option<&Snapshot> {
        self.presentation.snapshot()
    }

    fn presented_player(&self, id: sim::ClientId) -> Option<&playerstate_iw4::PlayerState> {
        self.presentation.presented.player(id)
    }

    fn shellshock(&self) -> Option<&hud_iw4::ShockParams> {
        self.presentation.shellshock()
    }
}

struct InputFrameTimes {
    key_frame_msec: u32,
    key_now_msec: i32,
    client_frame_secs: f32,
    game_msec: i32,
}

#[derive(bevy::ecs::system::SystemParam)]
pub struct ClientInputClocks<'w> {
    real: Res<'w, Time<Real>>,
    client: Res<'w, ClientRealtime>,
    game: Res<'w, FrameClock>,
}

impl ClientInputClocks<'_> {
    fn capture(&self) -> InputFrameTimes {
        InputFrameTimes {
            key_frame_msec: key_frame_msec(self.real.delta_secs()),
            key_now_msec: frame_time_msec(self.real.elapsed_secs()),
            client_frame_secs: self.client.frametime_secs(),
            game_msec: self.game.time(),
        }
    }
}

#[derive(bevy::ecs::system::SystemParam)]
pub struct ClientCommandSampler<'w> {
    actions: ResMut<'w, ClientActionInput>,
    look: ResMut<'w, LookState>,
    template: ResMut<'w, ClientCmdTemplate>,
    select: ResMut<'w, WeaponSelect>,
    action_inbox: Option<ResMut<'w, ClientActionInbox>>,
    cursor: ResMut<'w, LocationCursor>,
}

impl ClientCommandSampler<'_> {
    fn sample(
        mut self,
        source: LocalCommandSource,
        clocks: ClientInputClocks,
        view: Option<Res<frame::ViewSubject>>,
        aim: Res<crate::ViewweaponAim>,
        level: Res<ClientLevel>,
        collision: Res<ClientCollision>,
    ) {
        if !source.enabled() {
            self.template.ready = false;
            self.actions.client.weapon_cycles.clear();
            self.actions.client.action_slots.clear();
            return;
        }
        let times = clocks.capture();
        self.actions.frame_msec = times.key_frame_msec;
        self.actions.now_msec = times.key_now_msec;
        let ps = source.player();

        let frozen = ps.is_some_and(|ps| (ps.pm_flags & 0x800) != 0)
            || source
                .snapshot()
                .and_then(|snapshot| snapshot.meta.for_client(source.client_id()))
                .is_none_or(|meta| meta.controls.frozen);

        if frozen {
            self.actions.mouse_x = 0.0;
            self.actions.mouse_y = 0.0;
            self.actions.pad_move = [0.0; 2];
            self.actions.pad_look = [0.0; 2];
        }
        if let Some(ps) = ps {
            let world = &collision.0;
            let eye = [
                ps.origin[0],
                ps.origin[1],
                ps.origin[2] + ps.view_height_current,
            ];
            let visible = |point: [f32; 3]| {
                let hit =
                    world.trace_world(eye, point, [0.0; 3], [0.0; 3], hud_iw4::OVERHEAD_TRACE_MASK);
                hit.fraction >= 1.0 && hit.startsolid == 0
            };
            let angles = [
                self.look.angles[0] as f32 / input_iw4::ANGLE2SHORT,
                self.look.angles[1] as f32 / input_iw4::ANGLE2SHORT,
            ];
            let (sp, cp) = angles[0].to_radians().sin_cos();
            let (sy, cy) = angles[1].to_radians().sin_cos();
            let forward = [cp * cy, cp * sy, -sp];
            let in_front = |origin: [f32; 3], radius: f32| {
                let d = [
                    origin[0] - ps.origin[0],
                    origin[1] - ps.origin[1],
                    origin[2] - ps.origin[2],
                ];
                d[0] * forward[0] + d[1] * forward[1] + d[2] * forward[2] + radius >= 0.0
            };
            let mut targets: Vec<crate::client::pad_aim::AimTarget> = Vec::new();
            if self.actions.pad_aim_assist > 0 {
                const RADIUS: f32 = 10.0;
                let snapshot = source.snapshot();
                let team = |id: sim::ClientId| {
                    snapshot
                        .and_then(|s| s.meta.for_client(id))
                        .map(|m| m.client_state_team)
                };
                let teams = snapshot.is_some_and(|s| s.meta.kind.is_team());
                let mine = team(source.client_id());
                for id in snapshot
                    .into_iter()
                    .flat_map(|s| s.players.iter().map(|(id, _)| *id))
                {
                    if id == source.client_id() || (teams && team(id) == mine) {
                        continue;
                    }
                    let Some(other) = source.presented_player(id).filter(|o| o.pm_type == 0) else {
                        continue;
                    };
                    let o = other.origin;
                    let head = [o[0], o[1], o[2] + other.view_height_current];
                    if !in_front(o, RADIUS) || !visible(head) {
                        continue;
                    }
                    let top = other.view_height_current + 8.0;
                    targets.push(crate::client::pad_aim::AimTarget {
                        key: u64::from(id.0),
                        mins: [o[0] - RADIUS, o[1] - RADIUS, o[2]],
                        maxs: [o[0] + RADIUS, o[1] + RADIUS, o[2] + top],
                        aim: [o[0], o[1], o[2] + top * 0.75],
                        velocity: other.velocity,
                    });
                }
            }
            let ranges = level
                .content
                .weapon_combat_row(playerstate_iw4::get_viewmodel_weapon_index(ps))
                .map_or(weapon_iw4::AimAssistRanges::NONE, |facts| facts.aim_assist);
            let view = crate::client::pad_aim::AimView {
                eye,
                angles,
                velocity: ps.velocity,
                ads_lerp: ps.f_weapon_pos_frac,
                fov_scale: self.actions.fov_scale.max(0.01),
                ranges,
                dt: times.client_frame_secs,
            };
            let ads = self.actions.client.using_ads || self.actions.client.kb.speed.active;
            crate::client::pad_aim::pad_look_frame(&mut self.actions, &view, &targets, ads);
        } else {
            self.actions.pad_look_delta = [0.0; 2];
        }
        if frozen {
            self.actions.pad_look_delta = [0.0; 2];
        }
        let choose_direction = source
            .snapshot()
            .and_then(|snapshot| snapshot.meta.for_client(source.client_id()))
            .and_then(|meta| meta.location_selection.as_ref())
            .map(|selection| selection.choose_direction);
        let location_input = choose_direction.is_some().then(|| {
            let mouse = (self.actions.mouse_x, self.actions.mouse_y);
            let pad = LocationPad::capture(&self.actions);
            self.actions.mouse_x = 0.0;
            self.actions.mouse_y = 0.0;
            self.actions.pad_look_delta = [0.0; 2];
            self.actions.pad_move = [0.0; 2];
            self.actions.pad_turn_rate = [0.0; 2];
            self.actions.pad_lockon = None;
            self.actions.pad_autoaim = None;
            (mouse, pad)
        });
        let remote_mouse = source
            .snapshot()
            .and_then(|snapshot| snapshot.meta.for_client(source.client_id()))
            .and_then(|meta| meta.remote_missile)
            .filter(|link| link.unlink_at_ms.is_none())
            .map(|_| {
                let mouse = (self.actions.mouse_x, self.actions.mouse_y);
                self.actions.mouse_x = 0.0;
                self.actions.mouse_y = 0.0;
                self.actions.pad_look_delta = [0.0; 2];
                mouse
            });
        let look_state = ps
            .zip(source.shellshock())
            .map(|(ps, shock)| {
                hud_iw4::update_shellshock_look_control(
                    times.game_msec,
                    ps.shellshock_time,
                    ps.shellshock_duration,
                    shock.look,
                )
            })
            .unwrap_or(hud_iw4::ShellshockLookState {
                sensitivity: 1.0,
                max_pitch_speed: 0.0,
                max_yaw_speed: 0.0,
            });
        self.actions.shellshock_look_scale = look_state.sensitivity;
        self.actions.cgame_max_pitch_speed = look_state.max_pitch_speed;
        self.actions.cgame_max_yaw_speed = look_state.max_yaw_speed;
        let now_msec = self.actions.now_msec;
        let frame_msec = self.actions.frame_msec;
        let cl_yawspeed = self.actions.cl_yawspeed;
        let max_pitch = self.actions.cgame_max_pitch_speed;
        let max_yaw = self.actions.cgame_max_yaw_speed;
        self.look.accumulate_keyboard(
            &mut self.actions.client.kb,
            AdjustAnglesInput {
                dt: times.client_frame_secs,
                now_msec,
                frame_msec,
                cl_yawspeed,
                cl_pitchspeed: CL_PITCHSPEED_DEFAULT,
                cl_anglespeedkey: CL_ANGLESPEEDKEY_DEFAULT,
                cgame_max_yaw_speed: max_yaw,
                cgame_max_pitch_speed: max_pitch,
                frozen,
            },
        );
        if std::mem::take(&mut self.actions.client.center_view) {
            if let Some(ps) = ps {
                self.look.angles[0] = (-ps.delta_angles[0] * input_iw4::ANGLE2SHORT) as i32;
            }
        }
        let ps_weapon = ps.map(|ps| ps.weapon).unwrap_or(0);
        let life_sequence = source
            .snapshot()
            .and_then(|snapshot| snapshot.meta.for_client(source.client_id()))
            .map(|meta| meta.life_sequence.0)
            .unwrap_or(0);
        if self.actions.stance_life != Some(life_sequence) {
            self.actions.client.stance_latch = 0;
            self.actions.stance_life = Some(life_sequence);
            self.actions.stance_event_sequence = ps.map_or(0, |ps| ps.event_sequence);
        } else if let Some(ps) = ps {
            let pending = ps
                .event_sequence
                .wrapping_sub(self.actions.stance_event_sequence);
            if pending > 0 {
                let events = [ps.events_0, ps.events_1, ps.events_2, ps.events_3];
                for age in (0..pending.min(4)).rev() {
                    let seq = ps.event_sequence.wrapping_sub(1 + age);
                    match EntityEventKind(events[(seq & 3) as usize]) {
                        EntityEventKind::STANCE_FORCE_STAND => self.actions.client.stance_latch = 0,
                        EntityEventKind::STANCE_FORCE_CROUCH => {
                            self.actions.client.stance_latch =
                                playerstate_iw4::buttons::CROUCH as i32;
                        }
                        EntityEventKind::STANCE_FORCE_PRONE => {
                            self.actions.client.stance_latch =
                                playerstate_iw4::buttons::PRONE as i32;
                        }
                        _ => {}
                    }
                }
                self.actions.stance_event_sequence = ps.event_sequence;
            }
        }
        follow_held_weapon_select(&mut self.select, ps_weapon, life_sequence, times.game_msec);
        if let Some(ps) = ps {
            if self.select.index == ps.weapon {
                self.select.mapped_index = if level
                    .content
                    .weapon_combat_row(ps.weapon)
                    .is_some_and(|facts| facts.inventory_type == 3)
                {
                    ps.weapon_primary
                } else {
                    ps.weapon
                };
            }
        }
        let slots = std::mem::take(&mut self.actions.client.action_slots);
        if let Some(inbox) = self.action_inbox.as_mut() {
            for &slot in &slots {
                let action = sim::ClientAction::ActionSlot { slot: slot as u8 };
                if let Err(error) = inbox.push(source.client_id(), action) {
                    diag::warn!(Net, "action_slot: not queued — {error}");
                }
            }
        }
        if let Some(ps) = ps.filter(|_| !frozen) {
            for slot in slots {
                if !input_iw4::weapon_select::weapon_cycle_allowed(
                    ps,
                    times.game_msec,
                    self.select.time,
                    0,
                    0,
                ) {
                    continue;
                }
                let target = match ps.action_slot_type.get(slot).copied() {
                    Some(1) => ps.action_slot_param[slot].max(0) as u32,
                    Some(2) => match level.content.weapon_combat_row(self.select.index) {
                        Some(facts) if facts.inventory_type == 3 => self.select.mapped_index,
                        Some(facts) => facts.alternate_weapon,
                        None => 0,
                    },
                    _ => 0,
                };
                if target == 0 {
                    continue;
                }
                let parent = if ps.weapons.contains(&(target as i32)) {
                    target
                } else {
                    self.select.mapped_index
                };
                let owned = ps.weapons.contains(&(parent as i32))
                    && (parent == target
                        || level
                            .content
                            .weapon_combat_row(parent)
                            .is_some_and(|f| f.alternate_weapon == target));
                if !owned {
                    continue;
                }
                self.select.index = target;
                self.select.mapped_index = parent;
                self.select.time = times.game_msec;
                input_iw4::set_ads(&mut self.actions.client, false);
            }
        }
        let cycles = std::mem::take(&mut self.actions.client.weapon_cycles);
        let in_killcam = view.is_some_and(|v| v.in_killcam());
        if let Some(ps) = ps {
            for next in cycles {
                if next && gamemode_iw4::copycat_weapnext_bind_active(ps.pm_type, in_killcam) {
                    if let Some(inbox) = self.action_inbox.as_mut() {
                        if let Err(error) =
                            inbox.push(source.client_id(), sim::ClientAction::UseCopycat)
                        {
                            diag::warn!(Net, "use_copycat: not queued — {error}");
                        }
                    }
                }
                cycle_weapon_select(
                    &mut self.select,
                    &mut self.actions.client,
                    ps,
                    &level.content,
                    times.game_msec,
                    next,
                );
            }
        }
        self.actions.client.airborne =
            ps.is_some_and(|ps| ps.ground_entity_num == playerstate_iw4::ENTITYNUM_NONE);
        let mut cmd = build_usercmd(&mut self.actions, &self.look, 0);
        use_reload_as_reload(&mut self.actions, &mut cmd, ps);
        self.look.angles = cmd.angles;
        if !frozen
            && aim.live
            && ps.is_some_and(|ps| aim.weapon == playerstate_iw4::get_viewmodel_weapon_index(ps))
        {
            cmd.gun_angle_offset = aim.angle_offset;
        }
        if let Some((mouse_x, mouse_y)) = remote_mouse {
            cmd.remote_control = remote_control_axes(&self.actions, mouse_x, mouse_y);
            cmd.buttons |= playerstate_iw4::buttons::REMOTE_CONTROL;
        }

        cmd.weapon = self.select.index as u16;
        cmd.weapon_mapped = self.select.mapped_index as u16;
        if let Some(loadout) = source
            .snapshot()
            .and_then(|snapshot| snapshot.meta.for_client(source.client_id()))
            .and_then(|meta| meta.loadout.as_ref())
        {
            if self.actions.client.kb.frag.active || self.actions.client.kb.frag.was_pressed {
                cmd.off_hand_index = loadout.lethal as u16;
            } else if self.actions.client.kb.smoke.active
                || self.actions.client.kb.smoke.was_pressed
            {
                cmd.off_hand_index = loadout.tactical as u16;
            }
        }
        match location_input {
            Some((mouse, pad)) => {
                let held = cmd.buttons;
                let directing =
                    choose_direction == Some(true) && held & playerstate_iw4::buttons::ADS != 0;
                let cancel = if choose_direction == Some(true) {
                    playerstate_iw4::buttons::MELEE_CHARGE
                } else {
                    playerstate_iw4::buttons::ADS | playerstate_iw4::buttons::MELEE_CHARGE
                };
                cmd.selected_location = self.cursor.step(
                    self.actions.sensitivity,
                    mouse,
                    pad,
                    times.client_frame_secs,
                    choose_direction == Some(true),
                    directing,
                );
                if choose_direction != Some(true) {
                    cmd.selected_location[2] = 0;
                }
                cmd.buttons &= playerstate_iw4::buttons::CROUCH | playerstate_iw4::buttons::PRONE;
                if held & playerstate_iw4::buttons::ATTACK != 0 {
                    cmd.buttons |= playerstate_iw4::buttons::LOCATION_SELECT;
                } else if held & cancel != 0 {
                    cmd.buttons |= playerstate_iw4::buttons::LOCATION_CANCEL;
                }
            }
            None => *self.cursor = LocationCursor::default(),
        }
        if cmd.buttons & playerstate_iw4::buttons::MELEE_CHARGE != 0
            && let (Some(ps), Some(snapshot)) = (ps, source.snapshot())
            && let Some((yaw, dist)) =
                melee_charge_target(ps, source.client_id(), snapshot, &collision.0)
        {
            cmd.melee_charge_yaw = yaw;
            cmd.melee_charge_dist = dist;
        }
        self.template.cmd = cmd;
        self.template.ready = true;
    }
}

pub fn sample_client_input(
    sampler: ClientCommandSampler,
    source: LocalCommandSource,
    clocks: ClientInputClocks,
    view: Option<Res<frame::ViewSubject>>,
    aim: Res<crate::ViewweaponAim>,
    level: Res<ClientLevel>,
    collision: Res<ClientCollision>,
) {
    sampler.sample(source, clocks, view, aim, level, collision);
}

const AIM_AUTOMELEE_RANGE: f32 = 128.0;
const MELEE_REGION_TAN_X: f32 = 0.849 * 0.5;
const MELEE_REGION_TAN_Y: f32 = 0.478 * 0.5;
fn melee_charge_target(
    ps: &playerstate_iw4::PlayerState,
    local: sim::ClientId,
    snapshot: &Snapshot,
    world: &sim::CollisionScene,
) -> Option<(f32, u8)> {
    if ps.pm_flags & playerstate_iw4::pm_flags::PRONE != 0 {
        return None;
    }
    let local_team = snapshot.meta.for_client(local)?.client_state_team;
    let range = if ps.perks[1] & playerstate_iw4::PERK1_EXTENDEDMELEE != 0 {
        playerstate_iw4::EXTENDED_MELEE_TARGET_RANGE
    } else {
        AIM_AUTOMELEE_RANGE
    };
    let eye = [
        ps.origin[0],
        ps.origin[1],
        ps.origin[2] + ps.view_height_current,
    ];
    let (forward, right, up) = math_iw4::angle_vectors(ps.viewangles);
    let dot = |a: [f32; 3], b: [f32; 3]| a[0] * b[0] + a[1] * b[1] + a[2] * b[2];
    let mut best: Option<(f32, [f32; 3])> = None;
    for (id, other) in &snapshot.players {
        if *id == local || other.pm_type > 1 {
            continue;
        }
        let Some(meta) = snapshot.meta.for_client(*id) else {
            continue;
        };
        if meta.lifecycle != sim::ClientLifecycle::Alive
            || meta.client_state_team == 3
            || (local_team != 0 && meta.client_state_team == local_team)
        {
            continue;
        }
        let delta = [
            other.origin[0] - ps.origin[0],
            other.origin[1] - ps.origin[1],
            other.origin[2] - ps.origin[2],
        ];
        if dot(delta, delta) > range * range {
            continue;
        }
        let center = [other.origin[0], other.origin[1], other.origin[2] + 36.0];
        let to = [center[0] - eye[0], center[1] - eye[1], center[2] - eye[2]];
        let depth = dot(to, forward);
        if depth <= 1.0 {
            continue;
        }
        let x = dot(to, right) / depth;
        let y = dot(to, up) / depth;
        if x.abs() - 15.0 / depth > MELEE_REGION_TAN_X
            || y.abs() - 36.0 / depth > MELEE_REGION_TAN_Y
        {
            continue;
        }
        let crosshair = x * x + y * y;
        if best.is_some_and(|(b, _)| b <= crosshair) {
            continue;
        }
        let hit = world.trace_static_world(eye, center, [0.0; 3], [0.0; 3], sim::MASK_SHOT);
        if hit.fraction < 1.0 {
            continue;
        }
        best = Some((crosshair, delta));
    }
    let (_, delta) = best?;
    let dist = dot(delta, delta).sqrt();
    if dist <= movement_iw4::MELEE_CHARGE_PLAYER_MELEE_RANGE_DEFAULT {
        return None;
    }
    Some((math_iw4::vec_to_yaw(delta[0], delta[1]), dist as u8))
}

/// `yaw` is counterclockwise from map up.
#[derive(Resource, Clone, Copy, Debug, PartialEq)]
pub struct LocationCursor {
    pub at: [f32; 2],
    pub aim: [f32; 2],
    pub yaw: f32,
    pub directing: bool,
}

impl Default for LocationCursor {
    fn default() -> Self {
        Self {
            at: [0.5, 0.5],
            aim: [0.5, 0.5],
            yaw: 0.0,
            directing: false,
        }
    }
}

const LOCATION_CURSOR_SPEED: f32 = 0.6;

const LOCATION_PAD_SPEED: f32 = 0.6;

const LOCATION_PAD_AIM_DEFLECTION: f32 = 0.5;

#[derive(Clone, Copy, Debug)]
struct LocationPad {
    movement: [f32; 2],
    look: [f32; 2],
    look_deflection: f32,
}

impl LocationPad {
    fn capture(input: &ClientActionInput) -> Self {
        let up = if input.pad_invert {
            -input.pad_look[1]
        } else {
            input.pad_look[1]
        };
        Self {
            movement: input.pad_move,
            look: [input.pad_look[0], up],
            look_deflection: input.pad_deflection,
        }
    }
}

impl LocationCursor {
    fn step(
        &mut self,
        sensitivity: f32,
        mouse: (f32, f32),
        pad: LocationPad,
        dt: f32,
        choose_direction: bool,
        directing: bool,
    ) -> [u8; 3] {
        let (mouse_x, mouse_y) = mouse;
        let scale = LOCATION_CURSOR_SPEED * sensitivity * 0.002;
        let further = |a: f32, b: f32| if a.abs() >= b.abs() { a } else { b };
        let (right, up) = if choose_direction {
            (pad.movement[1], pad.movement[0])
        } else {
            (
                further(pad.look[0], pad.movement[1]),
                further(pad.look[1], pad.movement[0]),
            )
        };
        let pad_step = LOCATION_PAD_SPEED * dt.clamp(0.0, 0.1);
        let moved = [
            mouse_x * scale + right * pad_step,
            mouse_y * scale - up * pad_step,
        ];
        self.directing = directing;
        if directing {
            self.aim = [
                (self.aim[0] + moved[0]).clamp(0.0, 1.0),
                (self.aim[1] + moved[1]).clamp(0.0, 1.0),
            ];
            let d = [self.aim[0] - self.at[0], self.aim[1] - self.at[1]];
            if d[0] != 0.0 || d[1] != 0.0 {
                self.yaw = (-d[0]).atan2(-d[1]).to_degrees().rem_euclid(360.0);
            }
        } else {
            self.at = [
                (self.at[0] + moved[0]).clamp(0.0, 1.0),
                (self.at[1] + moved[1]).clamp(0.0, 1.0),
            ];
            self.aim = self.at;
            if choose_direction
                && pad.look_deflection >= LOCATION_PAD_AIM_DEFLECTION
                && (pad.look[0] != 0.0 || pad.look[1] != 0.0)
            {
                self.yaw = (-pad.look[0])
                    .atan2(pad.look[1])
                    .to_degrees()
                    .rem_euclid(360.0);
            }
        }
        let byte = |v: f32| ((v * 255.0 - 128.0).round() as i32).clamp(-128, 127) as i8 as u8;
        let yaw = (self.yaw * (256.0 / 360.0)).round() as i32 as u8;
        [byte(self.at[0]), byte(self.at[1]), yaw]
    }
}

/// A stall is a stretch in which the authority acked none of the local client's
/// commands. Both measures of one — the age of the oldest command still waiting
/// to be acked, and the command time spanned by the unacknowledged history — are
/// wall-clock spans, so a single long frame fills either on its own — which is
/// why one stall is not a failure.
pub const BACKLOG_STALL_MS: i32 = 1000;

/// How many stalls the link may take before the session is failed for real. A
/// hitch costs one and is recovered from; an authority that has genuinely gone
/// away keeps earning them and fails once they add up.
pub const BACKLOG_STALLS_BEFORE_FAIL: u32 =
    (master_protocol::SESSION_IDLE.as_millis() / BACKLOG_STALL_MS as u128) as u32 - 1;

#[derive(Debug, Default)]
pub struct BacklogStalls {
    counted: u32,

    acks_at_last: u64,

    last_ms: Option<i32>,
}

impl BacklogStalls {
    /// Count a stall, at most one per `BACKLOG_STALL_MS` and only while the
    /// authority has acked nothing since the last one. `None` means the caller
    /// has already acted on the stall still in progress.
    fn note(&mut self, now_ms: i32, acks: u64) -> Option<u32> {
        if acks != self.acks_at_last {
            self.acks_at_last = acks;
            self.counted = 0;
            self.last_ms = Some(now_ms);
            return None;
        }
        if self
            .last_ms
            .is_some_and(|last| now_ms.saturating_sub(last) < BACKLOG_STALL_MS)
        {
            return None;
        }
        self.last_ms = Some(now_ms);
        self.counted = self.counted.saturating_add(1);
        Some(self.counted)
    }

    fn clear(&mut self) {
        *self = Self::default();
    }
}

pub fn enforce_client_work_limits(
    pending: Res<PendingClientSends>,
    mut prediction: ResMut<ClientPredictionState>,
    cg: Res<FrameClock>,
    cls: Res<ClientRealtime>,
    mut signon: ResMut<crate::SignonState>,
    bridge: Option<Res<crate::MasterBridge>>,
    mut stalls: Local<BacklogStalls>,
) {
    if signon.phase.is_failed() {
        return;
    }
    let oldest = pending.iter().next().map(|(_, cmd, _)| cmd.server_time);
    let duration =
        queued_command_duration(prediction.0.history().iter().map(|row| row.cmd.server_time));
    let stalled = oldest.is_some_and(|oldest| cg.time().saturating_sub(oldest) > BACKLOG_STALL_MS)
        || duration > BACKLOG_STALL_MS;
    let stall = if stalled {
        stalls.note(
            cls.realtime(),
            pending.acknowledged.map_or(0, |seq| u64::from(seq.0)),
        )
    } else {
        stalls.clear();
        None
    };
    let reason = if stall.is_some_and(|counted| counted >= BACKLOG_STALLS_BEFORE_FAIL) {
        Some("InputBacklogExceeded")
    } else {
        None
    };
    let Some(reason) = reason else {
        if let Some(counted) = stall {
            diag::warn!(
                Net,
                "input backlog: no authority ack in {BACKLOG_STALL_MS} ms \
                 (stall {counted}/{BACKLOG_STALLS_BEFORE_FAIL})"
            );
        }
        return;
    };
    prediction.0.disarm();
    let match_key = crate::signon::live_match_key(bridge.as_deref());
    if let Some(bridge) = bridge {
        bridge.fail(reason);
    }
    signon.set_phase(crate::SignonPhase::Failed(
        crate::SignonFailReason::Transport {
            source: reason.to_owned(),
            stage: crate::FailStage::Transport,
            match_key,
        },
    ));
}

fn queued_command_duration(times: impl IntoIterator<Item = i32>) -> i32 {
    let mut previous = None;
    let mut duration = 0i32;
    for time in times {
        if let Some(previous) = previous {
            duration = duration.saturating_add(time.saturating_sub(previous).max(0));
        }
        previous = Some(time);
    }
    duration
}

#[derive(bevy::ecs::system::SystemParam)]
pub struct PredictionSceneSource<'w, 's> {
    level: Res<'w, ClientLevel>,
    collision: Res<'w, ClientCollision>,
    entities: Query<'w, 's, (&'static crate::CEntity, &'static crate::CEntityRuntime)>,
    items: Query<'w, 's, &'static mut crate::client::prediction_scene::PredictedItem>,
}

impl PredictionSceneSource<'_, '_> {
    fn build(
        &mut self,
        snapshot: Option<&Snapshot>,
    ) -> crate::client::prediction_scene::PredictionScene<'_> {
        crate::client::prediction_scene::PredictionScene {
            level: &self.level,
            collision: &self.collision.0,
            bodies: crate::client::prediction_scene::player_bodies(&self.entities, snapshot),
            items: self
                .items
                .iter_mut()
                .map(|item| item.into_inner())
                .collect(),
        }
    }
}

#[derive(bevy::ecs::system::SystemParam)]
pub struct LocalCommandPrediction<'w> {
    prediction: ResMut<'w, ClientPredictionState>,
    template: Res<'w, ClientCmdTemplate>,
    actions: ResMut<'w, ClientActionInput>,
    gate: Res<'w, AuthorityInputGate>,
    pending: ResMut<'w, PendingClientSends>,
    proxy: Res<'w, RemoteProxyState>,
    cls: Res<'w, ClientRealtime>,
    cg_clock: Res<'w, FrameClock>,
}

impl LocalCommandPrediction<'_> {
    fn generate(mut self, mut scene_source: PredictionSceneSource, adopted: &LastAdoptedSnapshot) {
        if !self.gate.local_cmds_enabled
            || !self.prediction.0.is_armed()
            || !self.template.ready
            || !self.cg_clock.started()
            || !self.pending.has_command_capacity(self.cg_clock.time())
        {
            return;
        }

        let msec = self.cls.frametime();
        if msec <= 0 {
            return;
        }

        let sample = self.proxy.0.shot_sample(self.cg_clock.time());
        let mut scene = scene_source.build(adopted.next());
        let generated = self.prediction.0.predict(
            self.template.cmd,
            ServerTime::from_ms(self.cg_clock.time()),
            &mut scene,
        );
        if let Some((seq, cmd)) = generated {
            self.actions.consume_edges();
            self.pending.push(seq, cmd, sample);
        }
    }
}

pub fn predict_local_move(
    time: Res<Time>,
    mut clock: ResMut<ClientClock>,
    role: Res<RuntimeRole>,
    command: LocalCommandPrediction,
    scene: PredictionSceneSource,
    adopted: Res<LastAdoptedSnapshot>,
) {
    if *role != RuntimeRole::Replay {
        let _tick = clock.tick(time.delta_secs() * 1000.0);
    }
    command.generate(scene, &adopted);
}

pub fn flush_bootstrap_applied(
    mut link: Option<ResMut<crate::transport::udp_session::UdpClientLink>>,
    adopted: Res<LastAdoptedSnapshot>,
    defer: Option<Res<crate::DeferBootstrapApplied>>,
) {
    if defer.is_some_and(|defer| defer.0) {
        return;
    }
    if !adopted.applied_this_frame {
        return;
    }
    if let Some(link) = link.as_mut() {
        link.flush_applied_after_adopt();
    }
}

#[derive(bevy::ecs::system::SystemParam)]
pub struct ClientCommandSender<'w> {
    link: Option<ResMut<'w, crate::transport::udp_session::UdpClientLink>>,
    inbox: ResMut<'w, ClientCommandInbox>,
    action_inbox: Option<ResMut<'w, ClientActionInbox>>,
    pacer: ResMut<'w, GameplaySendPacer>,
}

impl ClientCommandSender<'_> {
    fn send(
        &mut self,
        pending: &PendingClientSends,
        local: &LocalPresentClient,
        reliable_ack: &ClientReliableAck,
        cls: &ClientRealtime,
    ) {
        if let Some(link) = self.link.as_mut() {
            if link.connection.is_none() || !link.has_entered_match() {
                return;
            }

            if let Some(inbox) = self.action_inbox.as_deref_mut() {
                inbox.ack(link.action_ack());
            }
            let actions = self
                .action_inbox
                .as_deref()
                .map_or(&[][..], |inbox| inbox.pending());
            let now_ms = cls.realtime();
            let newest = pending.newest_seq();
            let gameplay_due = self.pacer.gameplay_due(now_ms, newest);

            let ack_due = reliable_ack.0 != self.pacer.last_sent_ack;
            let control_due = ack_due
                || (!actions.is_empty()
                    && self.pacer.last_control_ms.is_none_or(|last| {
                        now_ms.saturating_sub(last) >= GAMEPLAY_SEND_INTERVAL_MS
                    }));
            if !gameplay_due && !control_due {
                return;
            }

            let (cmds, retry_cursor) = if gameplay_due {
                self.pacer.command_window(pending)
            } else {
                (Vec::new(), None)
            };
            let window = &cmds;
            if let Err(e) = link.send_commands(window, &actions, reliable_ack.0) {
                diag::warn!(Net, "udp send cmds: {e}");
                return;
            }
            if gameplay_due {
                self.pacer.note_sent(now_ms, newest, reliable_ack.0);
                self.pacer.retry_cursor = retry_cursor;
            } else {
                self.pacer.last_sent_ack = reliable_ack.0;
            }
            if control_due {
                self.pacer.last_control_ms = Some(now_ms);
            }
            return;
        }

        if !self
            .pacer
            .gameplay_due(cls.realtime(), pending.newest_seq())
        {
            return;
        }
        self.pacer
            .note_sent(cls.realtime(), pending.newest_seq(), reliable_ack.0);
        for (seq, cmd, sample) in pending.iter().copied() {
            self.inbox.push(local.0, Some(seq), cmd, Some(sample));
        }
    }
}

pub fn send_pending_commands(
    mut sender: ClientCommandSender,
    reliable_ack: Res<ClientReliableAck>,
    cls: Res<ClientRealtime>,
    pending: Res<PendingClientSends>,
    local: Res<LocalPresentClient>,
) {
    sender.send(&pending, &local, &reliable_ack, &cls);
}

pub const MAX_GAMEPLAY_SENDS_PER_SEC: i32 = 60;

pub const GAMEPLAY_SEND_INTERVAL_MS: i32 =
    (1000 + MAX_GAMEPLAY_SENDS_PER_SEC - 1) / MAX_GAMEPLAY_SENDS_PER_SEC;

#[derive(Resource, Debug, Default)]
pub struct GameplaySendPacer {
    last_send_ms: Option<i32>,

    last_sent_seq: Option<CmdSeq>,

    retry_cursor: Option<CmdSeq>,

    last_sent_ack: u16,

    last_control_ms: Option<i32>,
}

impl GameplaySendPacer {
    fn command_window(
        &self,
        pending: &PendingClientSends,
    ) -> (
        Vec<(CmdSeq, UserCmd, sim::ShotSampleProvenance)>,
        Option<CmdSeq>,
    ) {
        let (mut fresh, retries): (Vec<_>, Vec<_>) = pending
            .iter()
            .copied()
            .partition(|(seq, _, _)| self.last_sent_seq.is_none_or(|last| *seq > last));
        let mut cursor = self.retry_cursor;
        // Repeat the oldest command on every send to unblock the ordered stream,
        // plus a small rotating window. A full 16-command repeat at 60 Hz floods
        // high-latency links with commands that are already in flight.
        if let Some(oldest) = retries.first() {
            fresh.push(*oldest);
            let rotating = &retries[1..];
            if !rotating.is_empty() {
                let start = rotating
                    .iter()
                    .position(|(seq, _, _)| cursor.is_none_or(|last| *seq > last))
                    .unwrap_or(0);
                for row in rotating
                    .iter()
                    .cycle()
                    .skip(start)
                    .take(rotating.len().min(3))
                {
                    cursor = Some(row.0);
                    fresh.push(*row);
                }
            }
        }
        fresh.sort_by_key(|(seq, _, _)| *seq);
        (fresh, cursor)
    }

    fn gameplay_due(&self, now_ms: i32, newest: Option<CmdSeq>) -> bool {
        if newest.is_none() {
            return false;
        }
        match self.last_send_ms {
            None => true,
            Some(last) => now_ms.saturating_sub(last) >= GAMEPLAY_SEND_INTERVAL_MS,
        }
    }

    fn note_sent(&mut self, now_ms: i32, newest: Option<CmdSeq>, ack: u16) {
        self.last_send_ms = Some(now_ms);
        self.last_sent_seq = newest;
        self.last_sent_ack = ack;
    }
}

#[derive(Resource, Debug, Default, Clone, Copy)]
pub struct ClientReliableAck(pub u16);

#[derive(bevy::ecs::system::SystemParam)]
pub struct PresentationTimeline<'w> {
    clock: Res<'w, ClientClock>,
    cg_clock: Res<'w, FrameClock>,
    role: Res<'w, RuntimeRole>,
    last_adopted: Res<'w, LastAdoptedSnapshot>,
    proxy: Res<'w, RemoteProxyState>,
}

impl PresentationTimeline<'_> {
    fn frame_interpolation(&self) -> f32 {
        self.clock.accumulator_ms / AUTHORITY_MS as f32
    }

    fn previous_for_interpolation(&self, snapshot: &Snapshot) -> Option<Arc<Snapshot>> {
        if *self.role == RuntimeRole::Replay {
            self.last_adopted
                .snap
                .clone()
                .filter(|previous| previous.tick.0 < snapshot.tick.0)
        } else {
            None
        }
    }

    fn render_time(&self, snapshot: &Snapshot) -> i32 {
        if *self.role == RuntimeRole::Replay {
            (snapshot.tick.0 as i32 + 1) * AUTHORITY_MS + self.clock.accumulator_ms as i32
        } else {
            self.cg_clock.time()
        }
    }

    fn body_time(&self, snapshot: &Snapshot, archived: bool) -> i32 {
        if *self.role == RuntimeRole::Replay {
            snapshot.tick.0 as i32 * AUTHORITY_MS - AUTHORITY_MS + self.clock.accumulator_ms as i32
        } else if archived {
            self.cg_clock.time().saturating_sub(self.proxy.0.delay_ms())
        } else {
            self.cg_clock.time().saturating_sub(AUTHORITY_MS)
        }
    }
}

#[derive(bevy::ecs::system::SystemParam)]
pub struct PresentationPublisher<'w, 's> {
    entities: Query<'w, 's, &'static mut crate::CEntityRuntime>,
    presented: ResMut<'w, PresentedSnapshot>,
    census: ResMut<'w, PresentLocalCensus>,
}

impl PresentationPublisher<'_, '_> {
    fn publish(
        &mut self,
        timeline: &PresentationTimeline,
        local: &LocalPresentClient,
        prediction: &ClientPredictionState,
        collision: &ClientCollision,
        has_world: Option<Res<State<frame::MatchScope>>>,
    ) {
        if has_world.is_some_and(|world| *world.get() != frame::MatchScope::Live) {
            self.presented.clear();
            *self.census = PresentLocalCensus {
                choice: Some("no_next"),
                snapshot_delta_time: None,
                predicted_delta_time: prediction.0.predicted_local().map(|ps| ps.delta_time),
            };
            return;
        }
        let Some(snap_arc) = timeline.last_adopted.next_snap.clone() else {
            *self.census = PresentLocalCensus {
                choice: Some("no_next"),
                snapshot_delta_time: None,
                predicted_delta_time: prediction.0.predicted_local().map(|ps| ps.delta_time),
            };
            return;
        };
        let archived = snap_arc
            .players
            .iter()
            .any(|(id, ps)| *id == local.0 && !ps.is_live_frame());
        let snap_arc = if archived {
            timeline
                .proxy
                .0
                .snapshot_at(local.0, timeline.cg_clock.time())
                .unwrap_or(snap_arc)
        } else {
            snap_arc
        };
        let snapshot = snap_arc.as_ref();
        let snapshot_local = snapshot
            .players
            .iter()
            .find(|(id, _)| *id == local.0)
            .map(|(_, ps)| ps);
        let predicted_ps = prediction.0.predicted_local().copied();
        let previous = self.presented.player(local.0).copied();
        let armed = *timeline.role != RuntimeRole::Replay && prediction.0.is_armed();
        let frame_interpolation = timeline.frame_interpolation();
        let interpolation_previous = timeline.previous_for_interpolation(snapshot);
        let interpolated = interpolation_previous.as_deref().and_then(|previous| {
            let previous_local = previous
                .players
                .iter()
                .find(|(id, _)| *id == local.0)
                .map(|(_, ps)| ps)?;
            let next_local = snapshot_local?;
            (previous_local.is_live_frame() && next_local.is_live_frame())
                .then(|| interpolate_player_state(previous_local, next_local, frame_interpolation))
        });
        let interpolated = interpolated.map(|pose| {
            let next = snapshot_local.expect("interpolation needs a decoded player");
            let scene = collision.0.player_collision();
            keep_interpolated_player_clear(pose, next, |pose| {
                let backend = scene.backend(&[], local.0.0 as u16, pose.origin);
                let mut stance = *pose;
                let height = movement_iw4::sync_stance_tail(&mut stance);
                let hit = movement_iw4::CollisionBackend::trace(
                    &backend,
                    movement_iw4::GroundTraceInput {
                        start: pose.origin,
                        end: pose.origin,
                        mins: [-15.0, -15.0, 0.0],
                        maxs: [15.0, 15.0, height],
                        tracemask: sim::MASK_PLAYER_SOLID,
                    },
                );
                hit.startsolid == 0 && hit.allsolid == 0
            })
        });
        *self.census = PresentLocalCensus {
            choice: Some(if interpolated.is_some() {
                "interpolated"
            } else if armed {
                PresentedSnapshot::presentation_local_choice(
                    snapshot_local,
                    predicted_ps.as_ref(),
                    previous.as_ref(),
                )
            } else {
                PresentedSnapshot::presentation_local_choice_unarmed(snapshot_local)
            }),
            snapshot_delta_time: snapshot_local.map(|ps| ps.delta_time),
            predicted_delta_time: predicted_ps.as_ref().map(|ps| ps.delta_time),
        };
        let mut predicted = match interpolated {
            Some(interpolated) => interpolated,
            None if armed => PresentedSnapshot::presentation_local_player_state(
                snapshot_local,
                predicted_ps,
                previous,
            ),
            None => PresentedSnapshot::presentation_local_player_state_unarmed(snapshot_local),
        };
        let render_time_ms = timeline.render_time(snapshot);

        let body_time_ms = timeline.body_time(snapshot, archived);

        let mut remote_poses = HashMap::new();
        let mut remote_provenance = HashMap::new();
        for (id, _) in &snapshot.players {
            if *id == local.0 && !archived {
                continue;
            }
            match timeline.proxy.0.interpolate_at(*id, render_time_ms) {
                ProxySample::Pose { ps, provenance } => {
                    remote_poses.insert(*id, ps);
                    remote_provenance.insert(*id, provenance);
                }
                ProxySample::Starved { held, provenance } => {
                    if let Some(held_ps) = held {
                        remote_poses.insert(*id, held_ps);
                    }
                    remote_provenance.insert(*id, provenance);
                }
            }
        }

        if archived {
            if let Some(seat) = remote_poses.remove(&local.0) {
                predicted = seat;
            }
        }
        for mut runtime in &mut self.entities {
            runtime.presented_player = None;
            if runtime.pose_e_type as i32 == entity_iw4::ET_PLAYER
                && let Ok(number) = u32::try_from(runtime.next_state.client_num)
                && let Some(ps) = remote_poses.get(&sim::ClientId(number))
            {
                runtime.origin = ps.origin;
                runtime.angles = ps.viewangles;
                let provenance = remote_provenance[&sim::ClientId(number)];
                let sample_time = match provenance.outcome {
                    crate::PresentationSampleOutcome::Exact { snapshot }
                    | crate::PresentationSampleOutcome::Starved {
                        held_snapshot: Some(snapshot),
                        ..
                    } => snapshot.0 as i32 * AUTHORITY_MS,
                    crate::PresentationSampleOutcome::Interpolated { .. } => {
                        provenance.effective_time.0
                    }
                    crate::PresentationSampleOutcome::Starved {
                        held_snapshot: None,
                        ..
                    } => unreachable!("a timeline.proxy pose always has a source snapshot"),
                };
                runtime.presented_player = Some((
                    crate::player_state_to_entity_state(sim::ClientId(number), ps),
                    sample_time,
                ));
            } else {
                runtime.present_pose(body_time_ms);
            }
        }

        let view_offset = if archived {
            [0.0; 3]
        } else {
            prediction
                .0
                .predicted_error()
                .offset_for_pm_type(predicted.pm_type)
        };
        self.presented.publish_proxied(
            snap_arc,
            local.0,
            predicted,
            remote_poses,
            remote_provenance,
        );
        self.presented.set_view_offset(view_offset);
        self.presented
            .set_snapshot_interpolation(interpolation_previous, frame_interpolation);
        self.presented.set_trajectory_sample(
            archived.then_some(body_time_ms),
            archived
                .then(|| {
                    timeline
                        .proxy
                        .0
                        .snapshot_after(local.0, timeline.cg_clock.time())
                })
                .flatten(),
        );
    }
}

pub fn publish_presented(
    mut publisher: PresentationPublisher,
    timeline: PresentationTimeline,
    local: Res<LocalPresentClient>,
    prediction: Res<ClientPredictionState>,
    collision: Res<ClientCollision>,
    has_world: Option<Res<State<frame::MatchScope>>>,
    phase: Option<ResMut<crate::UpdatePhaseCensus>>,
) {
    let publish_started = std::time::Instant::now();
    struct PublishStamp<'a> {
        started: std::time::Instant,
        phase: Option<ResMut<'a, crate::UpdatePhaseCensus>>,
    }
    impl Drop for PublishStamp<'_> {
        fn drop(&mut self) {
            let ms = self.started.elapsed().as_secs_f32() * 1000.0;
            if let Some(phase) = self.phase.as_mut() {
                phase.publish_presented_ms = Some(ms);
            }
        }
    }
    let _publish_stamp = PublishStamp {
        started: publish_started,
        phase,
    };

    publisher.publish(&timeline, &local, &prediction, &collision, has_world);
}

fn release_timeline(
    mut presented: ResMut<PresentedSnapshot>,
    mut events: ResMut<Messages<ReliableControlEvent>>,
) {
    presented.clear();
    events.clear();
}

fn release_client(
    mut actions: ResMut<ClientActionInbox>,
    mut presented: ResMut<PresentedSnapshot>,
    mut signon: ResMut<crate::SignonState>,
    mut admission: ResMut<crate::ClientAdmission>,
    mut events: ResMut<Messages<ReliableControlEvent>>,
) {
    actions.clear();
    presented.clear();
    *signon = crate::SignonState::default();
    *admission = crate::ClientAdmission::default();
    events.clear();
    perf::instant!("cgame_hold", "iw4l.sim", |ctx| {
        let (present_choice, presented_has_ps): (&str, i64) = ("no_next", 0);
        ctx.arg("present_choice", present_choice);
        ctx.arg("presented_has_ps", presented_has_ps);
    });
}

pub fn register_client_runtime(app: &mut App) {
    app.add_systems(
        OnEnter(frame::Timeline::Live),
        reconstruct_impacts.after(frame::ScopeSet::Install),
    )
    .add_systems(
        OnEnter(frame::Timeline::Killcam),
        reconstruct_impacts.after(frame::ScopeSet::Install),
    );
    app.add_systems(OnExit(frame::Timeline::Killcam), release_timeline);
    app.add_systems(
        PreUpdate,
        (
            advance_cls_realtime.in_set(ClientSet::Load),
            receive_ticks
                .in_set(frame::InMatch)
                .in_set(ClientSet::Receive),
            crate::signon::drive_client_admission_facts
                .in_set(ClientSet::Receive)
                .after(receive_ticks),
            crate::signon::drive_signon
                .in_set(ClientSet::Receive)
                .after(crate::signon::drive_client_admission_facts),
            crate::signon::drive_map_loaded
                .in_set(frame::InMatch)
                .in_set(ClientSet::Receive)
                .after(crate::signon::drive_client_admission_facts),
            select_timeline
                .in_set(frame::InMatch)
                .in_set(ClientSet::Receive)
                .after(crate::signon::drive_map_loaded),
        ),
    );
    app.add_systems(
        OnExit(frame::MatchScope::Live),
        release_client
            .in_set(frame::ScopeSet::Release)
            .after(crate::signon::release_match_protocol),
    );
    app.add_systems(
        OnExit(frame::Timeline::Live),
        release_timeline.in_set(frame::ScopeSet::Release),
    );
    app.add_systems(
        Update,
        enforce_client_work_limits
            .in_set(frame::InMatch)
            .before(predict_local_move)
            .after(reconcile_prediction),
    );
    app.init_resource::<crate::SignonState>()
        .init_resource::<crate::ClientAdmission>()
        .scoped::<ClientLevel>(frame::MatchScope::Live)
        .scoped::<ClientCollision>(frame::MatchScope::Live)
        .scoped::<ClientPredictionState>(frame::MatchScope::Live)
        .scoped::<ClientClock>(frame::MatchScope::Live)
        .scoped::<FrameClock>(frame::Timeline::Live)
        .scoped::<GameActive>(frame::Timeline::Live)
        .scoped::<GameJoinCensus>(frame::Timeline::Live)
        .init_resource::<ClientRealtime>()
        .scoped::<ReceivedTicks>(frame::MatchScope::Live)
        .scoped::<LastAdoptedSnapshot>(frame::Timeline::Live)
        .scoped::<PresentLocalCensus>(frame::Timeline::Live)
        .scoped::<PendingPresentedEntityEvents>(frame::Timeline::Live)
        .scoped::<PendingPelletFx>(frame::Timeline::Live)
        .scoped::<PendingAcceptedImpacts>(frame::Timeline::Live)
        .scoped::<ImpactCursor>(frame::Timeline::Live)
        .scoped_message::<AcceptedImpact>(frame::Timeline::Live)
        .scoped_message::<ImpactReconstruction>(frame::Timeline::Live)
        .scoped::<PendingClientSends>(frame::MatchScope::Live)
        .init_resource::<ClientCmdTemplate>()
        .scoped::<WeaponSelect>(frame::MatchScope::Live)
        .init_resource::<LocationCursor>()
        .scoped::<ClientReliableAck>(frame::MatchScope::Live)
        .scoped::<GameplaySendPacer>(frame::MatchScope::Live)
        .add_message::<ReliableControlEvent>()
        .scoped::<RemoteProxyState>(frame::Timeline::Live)
        .scoped::<ClientShotSamples>(frame::MatchScope::Live)
        .scoped::<crate::Scoreboard>(frame::Timeline::Live)
        .scoped_message::<crate::SvcLocalSound>(frame::Timeline::Live)
        .scoped_message::<crate::SvcScriptAudio>(frame::Timeline::Live)
        .scoped_message::<crate::SvcCardSlotCmd>(frame::Timeline::Live)
        .scoped_message::<crate::SvcOpenMenuCmd>(frame::Timeline::Live)
        .scoped_message::<crate::SvcHudSplash>(frame::Timeline::Live)
        .scoped_message::<crate::SvcGameNotify>(frame::Timeline::Live);
    crate::client::frame_census::register_update_phase_census(app);
    app.configure_sets(
        Update,
        (
            crate::PresentedPublished
                .in_set(ClientSet::Present)
                .after(publish_presented),
            frame::ClassEquipResolved
                .in_set(ClientSet::Present)
                .after(publish_presented),
        ),
    )
    .add_systems(
        Update,
        (
            replay_client_prediction
                .in_set(frame::InMatch)
                .in_set(ClientSet::Reconcile)
                .after(crate::client::presentation::entities::sync_client_entities)
                .before(advance_cg_frame_clock)
                .before(enforce_client_work_limits),
            advance_cg_frame_clock
                .in_set(frame::InMatch)
                .in_set(ClientSet::Reconcile)
                .after(reconcile_prediction),
            reconcile_prediction
                .in_set(frame::InMatch)
                .in_set(ClientSet::Reconcile),
            flush_bootstrap_applied
                .in_set(frame::InMatch)
                .in_set(ClientSet::Reconcile)
                .after(reconcile_prediction),
            apply_weapon_switch_requests
                .in_set(frame::InMatch)
                .in_set(ClientSet::Input)
                .before(sample_client_input),
            sample_client_input
                .in_set(frame::InMatch)
                .in_set(ClientSet::Input),
            predict_local_move
                .in_set(frame::InMatch)
                .in_set(ClientSet::Predict),
            send_pending_commands
                .in_set(frame::InMatch)
                .in_set(ClientSet::Send),
            publish_presented
                .in_set(frame::InMatch)
                .in_set(ClientSet::Present),
        ),
    );
}

pub fn register_listen_prediction_arm(app: &mut App) {
    app.add_systems(
        PreUpdate,
        arm_listen_prediction
            .in_set(frame::InMatch)
            .in_set(ClientSet::Load),
    );
}

fn reconstruct_impacts(mut reconstruction: MessageWriter<ImpactReconstruction>) {
    reconstruction.write(ImpactReconstruction);
}

fn replay_client_prediction(
    mut prediction: ResMut<ClientPredictionState>,
    adopted: Res<LastAdoptedSnapshot>,
    mut scene_source: PredictionSceneSource,
) {
    if !adopted.applied_this_frame {
        return;
    }
    let Some(snapshot) = adopted.next() else {
        return;
    };
    let mut scene = scene_source.build(Some(snapshot));
    prediction
        .0
        .on_authority(snapshot, adopted.prediction_ack, &mut scene);
}
