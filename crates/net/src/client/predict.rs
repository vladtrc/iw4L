use entity_iw4::adjust_position_for_mover;
use playerstate_iw4::{PlayerState, UserCmd, buttons, eflags, other_flags, pm_flags};
use sim::{ClientId, EventSequence, Snapshot, Tick};
use std::collections::VecDeque;

use crate::ServerTime;
use crate::client::predicted_error::PredictedError;
use crate::transport::netfields::{Deviation, field_error_magnitude, ps_deviation, ps_deviations};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct CmdSeq(pub u32);

impl CmdSeq {
    pub const fn next(self) -> Self {
        Self(self.0.wrapping_add(1))
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct MoveRecord {
    pub seq: CmdSeq,

    pub tick: Tick,

    pub cmd: UserCmd,
    pub input: PlayerState,
    pub output: PlayerState,
    pub sent: std::time::Instant,
}

#[derive(Clone, Debug)]
pub struct MoveHistory {
    moves: VecDeque<MoveRecord>,
    cap: usize,
}

pub const DEFAULT_HISTORY_CAP: usize = 256;

impl Default for MoveHistory {
    fn default() -> Self {
        Self::with_cap(DEFAULT_HISTORY_CAP)
    }
}

impl MoveHistory {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_cap(cap: usize) -> Self {
        let cap = cap.max(1);
        Self {
            moves: VecDeque::with_capacity(cap.min(256)),
            cap,
        }
    }

    pub fn len(&self) -> usize {
        self.moves.len()
    }

    pub fn is_empty(&self) -> bool {
        self.moves.is_empty()
    }

    pub fn cap(&self) -> usize {
        self.cap
    }

    pub fn oldest_seq(&self) -> Option<CmdSeq> {
        self.moves.front().map(|m| m.seq)
    }

    pub fn newest_seq(&self) -> Option<CmdSeq> {
        self.moves.back().map(|m| m.seq)
    }

    pub fn newest(&self) -> Option<&MoveRecord> {
        self.moves.back()
    }

    pub fn iter(&self) -> impl Iterator<Item = &MoveRecord> {
        self.moves.iter()
    }

    pub fn push(&mut self, record: MoveRecord) -> bool {
        if self.moves.len() >= self.cap {
            return true;
        }
        self.moves.push_back(record);
        false
    }

    pub fn clear(&mut self) {
        self.moves.clear();
    }

    pub fn clear_acknowledged(&mut self, acked: CmdSeq) -> AckMatch {
        match (self.moves.front(), self.moves.back()) {
            (Some(oldest), Some(newest)) => {
                if acked.0 > newest.seq.0 {
                    return AckMatch::Broken;
                }

                if acked.0 < oldest.seq.0.saturating_sub(1) {
                    return AckMatch::Broken;
                }
            }

            _ => return AckMatch::Retired,
        }
        let mut source = None;
        let mut retained = VecDeque::with_capacity(self.moves.len());
        for record in self.moves.drain(..) {
            match record.seq.0.cmp(&acked.0) {
                std::cmp::Ordering::Greater => retained.push_back(record),
                std::cmp::Ordering::Equal => source = Some(record),
                std::cmp::Ordering::Less => {}
            }
        }
        self.moves = retained;
        match source {
            Some(record) => AckMatch::Matched(record),
            None => AckMatch::Retired,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
#[expect(
    clippy::large_enum_variant,
    reason = "Acknowledgement transfers one existing move record without a per-command allocation."
)]
pub enum AckMatch {
    Matched(MoveRecord),

    Retired,

    Broken,
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct PredictionMetrics {
    pub predicted_moves: u64,
    pub snapshots: u64,
    pub acks_matched: u64,

    pub deviations: u64,
    pub replayed_moves: u64,

    pub forced_adopts: u64,

    pub retired_acks: u64,

    pub evicted_moves: u64,

    pub deepest_history: usize,

    pub last_deviation: Option<(&'static str, f32)>,
}

impl PredictionMetrics {
    pub fn report_line(&self) -> String {
        let replay_ratio = if self.snapshots > 0 {
            self.replayed_moves as f64 / self.snapshots as f64
        } else {
            0.0
        };
        let dev = self
            .last_deviation
            .map(|(field, dist)| format!("{field}@{dist:.1}"))
            .unwrap_or_else(|| "none".into());
        format!(
            "pred moves={} snaps={} acks={} retired={} dev={} replay={} ratio={replay_ratio:.2} forced={} evicted={} depth={} last={dev}",
            self.predicted_moves,
            self.snapshots,
            self.acks_matched,
            self.retired_acks,
            self.deviations,
            self.replayed_moves,
            self.forced_adopts,
            self.evicted_moves,
            self.deepest_history,
        )
    }
}

pub const MAX_UNACKED_SNAPSHOTS: u32 = 20;

use crate::client::prediction_scene::PredictionScene;
pub struct ClientPrediction {
    local: ClientId,
    meta: Option<sim::ClientSnapshotMeta>,
    phase: sim::MatchPhase,
    command_state: sim::bg::predict_player::PlayerCommandState,
    actions: Vec<(Tick, weapon_iw4::WeaponAction)>,
    next_owner_event: EventSequence,
    history: MoveHistory,
    roundtrip: Option<std::time::Duration>,
    armed: bool,
    next_seq: CmdSeq,

    next_tick: u32,

    snapshots_since_ack: u32,
    predicted_error: PredictedError,
    metrics: PredictionMetrics,

    predicted_local: Option<PlayerState>,

    last_cmd: Option<UserCmd>,
    acknowledged_cmd: Option<UserCmd>,

    replay_floor: Option<(CmdSeq, UserCmd)>,

    last_predict_msec: Option<i32>,

    had_local_last_snap: bool,
}

impl ClientPrediction {
    pub fn new(local: ClientId) -> Self {
        Self::with_history_cap(local, DEFAULT_HISTORY_CAP)
    }

    pub fn with_history_cap(local: ClientId, cap: usize) -> Self {
        Self {
            local,
            meta: None,
            phase: Default::default(),
            command_state: Default::default(),
            actions: Vec::new(),
            next_owner_event: EventSequence(1),
            history: MoveHistory::with_cap(cap),
            roundtrip: None,
            armed: false,
            next_seq: CmdSeq(1),
            next_tick: 0,
            snapshots_since_ack: 0,
            predicted_error: PredictedError::default(),
            metrics: PredictionMetrics::default(),
            predicted_local: None,
            last_cmd: None,
            acknowledged_cmd: None,
            replay_floor: None,
            last_predict_msec: None,
            had_local_last_snap: false,
        }
    }

    pub fn local(&self) -> ClientId {
        self.local
    }

    pub fn set_local(&mut self, local: ClientId) {
        self.local = local;
    }

    pub fn is_armed(&self) -> bool {
        self.armed
    }

    pub fn disarm(&mut self) {
        *self = Self::new(self.local);
    }

    pub fn history(&self) -> &MoveHistory {
        &self.history
    }

    /// Last observed send-to-ack time of a predicted command.
    pub fn roundtrip(&self) -> Option<std::time::Duration> {
        self.roundtrip
    }

    pub fn metrics(&self) -> PredictionMetrics {
        self.metrics
    }

    pub fn predicted_error(&self) -> &PredictedError {
        &self.predicted_error
    }

    pub fn predicted_error_mut(&mut self) -> &mut PredictedError {
        &mut self.predicted_error
    }

    pub fn predicted_local(&self) -> Option<&PlayerState> {
        self.predicted_local.as_ref()
    }

    pub fn last_cmd(&self) -> Option<&UserCmd> {
        self.last_cmd.as_ref()
    }

    pub fn last_predict_msec(&self) -> Option<i32> {
        self.last_predict_msec
    }

    pub fn had_local_last_snap(&self) -> bool {
        self.had_local_last_snap
    }

    pub fn local_life(&self) -> Option<sim::LifeSequence> {
        self.meta.as_ref().map(|meta| meta.life_sequence)
    }

    pub fn arm(&mut self) {
        self.armed = true;
    }

    pub(crate) fn next_owner_event(&mut self) -> EventSequence {
        let sequence = self.next_owner_event;
        self.next_owner_event = sequence.next();
        sequence
    }

    pub fn take_actions(&mut self) -> Vec<(Tick, weapon_iw4::WeaponAction)> {
        std::mem::take(&mut self.actions)
    }

    pub fn predict(
        &mut self,
        mut cmd: UserCmd,
        server_time: ServerTime,
        scene: &mut PredictionScene,
    ) -> Option<(CmdSeq, UserCmd)> {
        if !self.armed || self.history.len() >= self.history.cap() {
            return None;
        }
        let server_time = server_time.ms();
        cmd.server_time = server_time;
        let input = self.local_state().unwrap_or(PlayerState::ZERO);
        let delta = server_time.wrapping_sub(input.command_time);
        let msec = if delta > 0 { delta.min(200) } else { 0 };
        self.last_predict_msec = Some(msec);

        if self.predicted_local.is_some_and(|ps| !pmove_runs_for(&ps)) {
            let seq = self.next_seq;
            self.next_seq = seq.next();
            self.next_tick = self.next_tick.wrapping_add(1);
            self.last_cmd = Some(cmd);
            self.replay_floor = Some((seq, cmd));
            return Some((seq, cmd));
        }

        let seq = self.next_seq;
        self.next_seq = seq.next();

        let tick = Tick(self.next_tick);
        self.next_tick = self.next_tick.wrapping_add(1);

        let output = if msec > 0 {
            self.step_predicted(tick, seq, cmd, msec, true, scene)
                .unwrap_or(input)
        } else {
            input
        };

        let evicted = self.history.push(MoveRecord {
            seq,
            tick,
            cmd,
            input,
            output,
            sent: std::time::Instant::now(),
        });
        if evicted {
            self.metrics.evicted_moves += 1;
        }
        self.metrics.predicted_moves += 1;
        self.metrics.deepest_history = self.metrics.deepest_history.max(self.history.len());
        self.predicted_local = Some(output);
        self.last_cmd = Some(cmd);

        Some((seq, cmd))
    }

    /// The floor lets stale acks for the dropped commands retire; read as
    /// `Broken` they would drop every later window too, for good.
    fn drop_history(&mut self) {
        if let Some(cmd) = self.last_cmd {
            self.replay_floor = Some((CmdSeq(self.next_seq.0.wrapping_sub(1)), cmd));
        }
        self.history.clear();
    }

    pub fn retire_acks(&mut self, ack: Option<CmdSeq>) {
        if !self.armed {
            return;
        }
        self.metrics.snapshots += 1;
        let _ = self.apply_ack(ack);
    }

    fn apply_ack(&mut self, ack: Option<CmdSeq>) -> Option<MoveRecord> {
        let replay_ack = ack.map(|seq| {
            self.replay_floor
                .map_or(seq, |(floor, _)| if seq.0 < floor.0 { floor } else { seq })
        });
        match replay_ack {
            Some(seq) => match self.history.clear_acknowledged(seq) {
                AckMatch::Matched(record) => {
                    self.roundtrip = Some(record.sent.elapsed());
                    self.acknowledged_cmd = Some(record.cmd);
                    self.metrics.acks_matched += 1;
                    self.snapshots_since_ack = 0;
                    Some(record)
                }
                AckMatch::Retired => {
                    self.metrics.retired_acks += 1;
                    self.snapshots_since_ack = 0;
                    None
                }
                AckMatch::Broken => {
                    self.metrics.forced_adopts += 1;
                    self.drop_history();
                    None
                }
            },
            None if self.history.is_empty() => {
                self.snapshots_since_ack = 0;
                None
            }
            None => {
                self.snapshots_since_ack = self.snapshots_since_ack.saturating_add(1);
                if self.snapshots_since_ack > MAX_UNACKED_SNAPSHOTS {
                    self.metrics.forced_adopts += 1;
                    self.drop_history();
                    self.snapshots_since_ack = 0;
                }
                None
            }
        }
    }

    pub fn on_authority(
        &mut self,
        snapshot: &Snapshot,
        ack: Option<CmdSeq>,
        scene: &mut PredictionScene,
    ) {
        if !self.armed {
            return;
        }
        self.metrics.snapshots += 1;

        let pre_state = self.predicted_local;
        let pre_view = pre_state.map(|ps| ps.origin);
        let pre_e_flags = self.predicted_local.map(|ps| ps.e_flags);

        let replay_tick = snapshot.tick.0.wrapping_add(1);
        let authoritative = snapshot_local(snapshot, self.local);
        let was_present = self.had_local_last_snap;
        self.meta = snapshot.meta.for_client(self.local).cloned();
        self.phase = snapshot.meta.phase;
        self.predicted_local = authoritative;
        self.had_local_last_snap = authoritative.is_some();
        self.command_state.old_buttons = 0;
        self.command_state.old_angles = None;

        if authoritative.is_some_and(|ps| !pmove_runs_for(&ps)) {
            if let Some(cmd) = self.last_cmd {
                self.replay_floor = Some((CmdSeq(self.next_seq.0.saturating_sub(1)), cmd));
            }
            self.history.clear();
            self.predicted_local = authoritative;
            self.next_tick = replay_tick;
        } else {
            let source = self.apply_ack(ack);

            if let (Some(source), Some(authoritative)) = (source.as_ref(), authoritative.as_ref())
                && let Some(deviation) =
                    record_prediction_errors(source, authoritative, snapshot.tick, "current")
            {
                self.metrics.deviations += 1;
                self.metrics.last_deviation = Some((deviation.field, deviation.distance));
            }

            self.seed_old_cmd(source.as_ref());

            self.next_tick = replay_tick;

            self.metrics.replayed_moves += self.replay(scene) as u64;

            self.predicted_local = self.local_state().or(authoritative);
        }

        let in_now = authoritative.is_some();
        let player_view = authoritative.is_some_and(|ps| {
            (ps.other_flags & (other_flags::PLAYER | other_flags::DEAD_KILLCAM_TPV)) != 0
        });
        let post_origin = self.predicted_local.map(|ps| ps.origin);
        if player_view && in_now && !was_present {
            if let (Some(pre), Some(post)) = (pre_view, post_origin) {
                self.predicted_error.note_teleport_delta(pre, post);
            }
            self.predicted_error.reset_new_entity();
        } else if let (Some(pre), Some(post)) = (pre_view, post_origin) {
            let teleported = match (pre_e_flags, self.predicted_local.map(|ps| ps.e_flags)) {
                (Some(a), Some(b)) => (a ^ b) & eflags::TELEPORT != 0,
                _ => false,
            };
            if teleported || sim::world_graph::world_at(pre) != sim::world_graph::world_at(post) {
                self.predicted_error.note_teleport_delta(pre, post);
                self.predicted_error.reset();
            } else if pre_state
                .zip(self.predicted_local)
                .is_some_and(|(pre, post)| pre.command_time == post.command_time)
            {
                let ground = self
                    .predicted_local
                    .map(|ps| ps.ground_entity_num)
                    .unwrap_or(0);

                let e_type = snapshot_ground_e_type(snapshot, ground);
                let post = adjust_position_for_mover(post, ground, e_type, None, 0, 0);
                self.predicted_error.begin(pre, post);
            }
        }
        self.had_local_last_snap = in_now;
    }

    fn replay(&mut self, scene: &mut PredictionScene) -> usize {
        let mut tick = self.next_tick;
        let pending = self.history.len();

        for index in 0..pending {
            let cmd = self.history.moves[index].cmd;
            let sequence = self.history.moves[index].seq;

            let input = self.local_state().unwrap_or(PlayerState::ZERO);

            if cmd.server_time <= input.command_time {
                continue;
            }

            let msec = cmd.server_time.wrapping_sub(input.command_time);
            let output = self
                .step_predicted(Tick(tick), sequence, cmd, msec, false, scene)
                .unwrap_or(input);

            let record = &mut self.history.moves[index];
            record.tick = Tick(tick);
            record.cmd = cmd;
            record.input = input;
            record.output = output;

            tick = tick.wrapping_add(1);
        }

        self.next_tick = tick;
        pending
    }

    fn local_state(&self) -> Option<PlayerState> {
        self.predicted_local.filter(|_| self.had_local_last_snap)
    }

    fn seed_old_cmd(&mut self, acked: Option<&MoveRecord>) {
        let floor_predecessor = self.replay_floor.and_then(|(floor, cmd)| {
            (self.history.oldest_seq() == Some(floor.next())).then_some(cmd)
        });
        let (buttons, angles) = match acked
            .map(|r| r.cmd)
            .or(floor_predecessor)
            .or(self.acknowledged_cmd)
        {
            Some(cmd) => (cmd.buttons, cmd.angles),
            None => {
                let mut seeded = 0;
                if self
                    .local_state()
                    .is_some_and(|ps| (ps.pm_flags & pm_flags::SPRINTING) != 0)
                {
                    seeded |= buttons::SPRINT;
                }
                (seeded, [0; 3])
            }
        };
        self.command_state.old_buttons = buttons;
        self.command_state.old_angles = Some(angles);
    }

    fn step_predicted(
        &mut self,
        tick: Tick,
        sequence: CmdSeq,
        cmd: UserCmd,
        _msec: i32,
        new_command: bool,
        scene: &mut PredictionScene,
    ) -> Option<PlayerState> {
        if self.had_local_last_snap {
            if let (Some(ps), Some(meta)) = (self.predicted_local.as_mut(), self.meta.as_mut()) {
                let actions = sim::bg::predict_player::predict_player(
                    ps,
                    meta,
                    &mut self.command_state,
                    sim::bg::predict_player::PredictionWorld {
                        content: &scene.level.content,
                        collision: scene.collision,
                        bodies: &scene.bodies,
                    },
                    self.phase,
                    scene.level.root_seed,
                    sim::bg::predict_player::PredictionCommand {
                        local: self.local,
                        tick,
                        sequence: sim::CommandSequence(sequence.0),
                        cmd,
                    },
                );
                scene.advance_items(ps, meta, tick, self.phase);
                if new_command {
                    self.actions
                        .extend(actions.into_iter().map(|action| (tick, action)));
                }
                Some(*ps)
            } else {
                None
            }
        } else {
            None
        }
    }
}

fn snapshot_local(snapshot: &Snapshot, local: ClientId) -> Option<PlayerState> {
    snapshot
        .players
        .iter()
        .find(|(id, _)| *id == local)
        .map(|(_, ps)| *ps)
}

fn pmove_runs_for(ps: &PlayerState) -> bool {
    (ps.other_flags & other_flags::DEAD_KILLCAM_TPV) == 0
}

pub fn snapshot_ground_e_type(snapshot: &Snapshot, mover_num: i32) -> Option<i32> {
    if !entity_iw4::mover_num_in_adjust_range(mover_num) {
        return None;
    }
    if snapshot
        .players
        .iter()
        .any(|(id, _)| i32::try_from(id.0).ok() == Some(mover_num))
    {
        return Some(entity_iw4::ET_PLAYER);
    }
    if snapshot
        .meta
        .corpses
        .slots
        .iter()
        .any(|slot| slot.occupied && slot.entnum == mover_num)
    {
        return Some(entity_iw4::ET_PLAYER_CORPSE);
    }
    None
}

fn record_prediction_errors(
    source: &MoveRecord,
    authoritative: &PlayerState,
    tick: Tick,
    arm: &'static str,
) -> Option<Deviation> {
    if !perf::enabled() {
        return ps_deviation(&source.output, authoritative);
    }
    let frame = perf::frames::open_index();
    perf::instant!("prediction_check", "iw4l.sim", |ctx| {
        ctx.arg("arm", arm);
        ctx.arg("frame", frame as i64);
        ctx.arg("command", i64::from(source.seq.0));
        ctx.arg("tick", i64::from(tick.0));
        ctx.arg(
            "predicted_command_time",
            i64::from(source.output.command_time),
        );
        ctx.arg(
            "authority_command_time",
            i64::from(authoritative.command_time),
        );
    });
    let mut first = None;
    for deviation in ps_deviations(&source.output, authoritative) {
        perf::instant!("prediction_field_error", "iw4l.sim", |ctx| {
            ctx.arg("arm", arm);
            ctx.arg("frame", frame as i64);
            ctx.arg("command", i64::from(source.seq.0));
            ctx.arg("tick", i64::from(tick.0));
            ctx.arg("field", deviation.field);
            ctx.arg(
                "distance",
                field_error_magnitude(&source.output, authoritative, deviation.index),
            );
        });
        first.get_or_insert(deviation);
    }
    first
}
