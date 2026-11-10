use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sim::{SimWorld, Tick, TickInput, TickResult};
use std::io::Write;

pub struct HeadlessSession {
    prepared: session::PreparedSimulation,
    recipe: Value,
    configuration: Value,
    failed: bool,
}

impl HeadlessSession {
    pub(crate) fn new(
        prepared: session::PreparedSimulation,
        recipe: Value,
        configuration: Value,
    ) -> Self {
        Self {
            prepared,
            recipe,
            configuration,
            failed: false,
        }
    }
    pub fn world(&self) -> &SimWorld {
        &self.prepared.world
    }
    pub fn world_mut(&mut self) -> &mut SimWorld {
        &mut self.prepared.world
    }
    pub fn next_tick(&self) -> Tick {
        self.prepared.next_tick
    }
    pub fn weapons(&self) -> &asset_game::WeaponRegistry {
        &self.prepared.weapons
    }
    pub fn admit_actor(
        &mut self,
        client: sim::ClientId,
        account: sim::AccountId,
    ) -> Result<(), String> {
        if self.prepared.world.input_program().is_some() {
            return Err("admit actors before installing an input program".into());
        }
        self.prepared
            .world
            .persistent_data_mut()
            .admit_temporary(client, account)
            .map_err(|error| format!("actor account: {error:?}"))
    }
    pub fn install_program(
        &mut self,
        primary: sim::ClientId,
        program: scenario::author::Program,
    ) -> Result<(), String> {
        if !matches!(program.schema_version, 1 | 2)
            || (program.schema_version == 1 && !program.actors.is_empty())
        {
            return Err("invalid input program schema".into());
        }
        let convert = |client, tape: scenario::author::Tape| sim::input_program::InputProgram {
            client,
            quantum_ms: tape.quantum_ms,
            first_sequence: sim::CommandSequence(tape.first_sequence),
            samples: tape
                .samples
                .into_iter()
                .map(|s| playerstate_iw4::UserCmd {
                    server_time: s.time_ms,
                    buttons: s.buttons,
                    angles: s.angles,
                    weapon: s.weapon,
                    weapon_mapped: s.weapon_mapped,
                    off_hand_index: s.off_hand_index,
                    forwardmove: s.forwardmove,
                    rightmove: s.rightmove,
                    remote_control: s.remote_control,
                    ..playerstate_iw4::UserCmd::default()
                })
                .collect(),
            future_weapons: tape.future_weapons,
            stop_on_death: tape.stop_on_death,
            allow_menu_responses: false,
            scoreboard_window_ms: tape.scoreboard_window_ms,
        };
        let actors = program
            .actors
            .into_iter()
            .map(|actor| convert(sim::ClientId(actor.client), actor.tape))
            .collect();
        self.prepared
            .world
            .install_input_ensemble(convert(primary, program.tape), actors)
    }
    fn pending_tape_commands(&self) -> usize {
        let Some(root) = self.world().input_program() else {
            return 0;
        };
        std::iter::once(root)
            .chain(&root.actors)
            .filter(|actor| {
                actor.failure.is_none()
                    && actor.death_tick.is_none()
                    && !(actor.program.stop_on_death
                        && self
                            .world()
                            .client_meta(actor.program.client)
                            .is_some_and(|meta| meta.lifecycle == sim::ClientLifecycle::Dead))
            })
            .map(|actor| {
                let start = actor.start_tick.unwrap_or(self.next_tick());
                let elapsed = i64::from(self.next_tick().0.saturating_sub(start.0) + 1) * 50;
                actor
                    .program
                    .samples
                    .partition_point(|sample| i64::from(sample.server_time) <= elapsed)
                    .saturating_sub(actor.issued)
            })
            .sum()
    }
    pub fn step(&mut self, input: &TickInput) -> Result<TickResult, RunError> {
        let tick = self.next_tick();
        if self.failed {
            return Err(RunError::new(tick, "session", "session has already failed"));
        }
        if input.cmds.iter().any(|command| {
            !command.command.melee_charge_yaw.is_finite()
                || command
                    .command
                    .gun_angle_offset
                    .iter()
                    .any(|v| !v.is_finite())
        }) || input
            .shot_samples
            .iter()
            .any(|(_, sample)| !sample.alpha.is_finite())
        {
            return Err(RunError::new(
                tick,
                "input",
                "non-finite command or shot sample",
            ));
        }
        if self.prepared.world.gsc_realm() == Some(sim::script::FamilyId::IW4) {
            for (client, action) in &input.actions {
                if matches!(action, sim::ClientAction::JoinMatch)
                    && self
                        .prepared
                        .world
                        .persistent_data()
                        .account(*client)
                        .is_none()
                {
                    return Err(RunError::new(
                        tick,
                        "admission",
                        "call admit_actor before JoinMatch",
                    ));
                }
            }
        }
        let next = tick
            .0
            .checked_add(1)
            .filter(|tick| *tick <= i32::MAX as u32 / 50)
            .ok_or_else(|| RunError::new(tick, "clock", "simulation clock exhausted"))?;
        let result = match sim::try_step(&mut self.prepared.world, tick, input, 50) {
            Ok(result) => result,
            Err(error) => {
                self.failed = true;
                return Err(RunError::new(tick, "script", error.to_string()));
            }
        };
        self.prepared.next_tick = Tick(next);
        if let Some(error) = self.prepared.world.take_script_fault() {
            self.failed = true;
            return Err(RunError::new(tick, "script", error.to_string()));
        }
        Ok(result)
    }
}

impl Drop for HeadlessSession {
    fn drop(&mut self) {
        self.prepared.world.shutdown_game();
    }
}

pub struct InputContext<'a> {
    pub(crate) world: &'a mut SimWorld,
    pub(crate) tick: Tick,
}
impl InputContext<'_> {
    pub fn world(&self) -> &SimWorld {
        self.world
    }
    pub fn tick(&self) -> Tick {
        self.tick
    }
    pub fn drive_bots(&mut self, bots: &mut crate::BotInputs) -> TickInput {
        bots.input(self.tick, self.world)
    }
}

pub enum InputFrame {
    Step(TickInput),
    Finish,
    Cancel(String),
}

pub trait InputSource {
    fn input(&mut self, context: &mut InputContext<'_>) -> Result<InputFrame, String>;
}
impl<F> InputSource for F
where
    F: FnMut(&mut InputContext<'_>) -> Result<InputFrame, String>,
{
    fn input(&mut self, context: &mut InputContext<'_>) -> Result<InputFrame, String> {
        self(context)
    }
}

#[derive(Clone, Copy, Debug)]
pub struct RunLimits {
    pub ticks: u32,
    pub commands_per_tick: usize,
    pub log_bytes: u64,
}

pub struct RunRequest<S> {
    pub build_id: String,
    pub session: HeadlessSession,
    pub input: S,
    pub limits: RunLimits,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct RunSummary {
    pub ticks: u32,
    pub last_tick: Option<u32>,
    pub log_bytes: u64,
    pub final_snapshot_hash: u64,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct RunError {
    pub tick: u32,
    pub stage: String,
    pub message: String,
}
impl RunError {
    fn new(tick: Tick, stage: &str, message: impl Into<String>) -> Self {
        Self {
            tick: tick.0,
            stage: stage.into(),
            message: message.into(),
        }
    }
}
impl std::fmt::Display for RunError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "tick {} {}: {}", self.tick, self.stage, self.message)
    }
}
impl std::error::Error for RunError {}

#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum RunRecord {
    Header {
        build_id: String,
        platform: String,
        schema: u32,
        protocol: u32,
        seed: u64,
        content: u64,
        first_tick: u32,
        recipe: Value,
        configuration: Value,
        input_program: Value,
        initial_frame: Vec<u8>,
    },
    Tick {
        tick: u32,
        submitted: Value,
        authority_input: Value,
        program_receipts: Value,
        players: Value,
        events: Value,
        shots: Value,
        shot_verdicts: Value,
        projectile_impacts: Value,
        frame: Vec<u8>,
        state_hash: u64,
    },
    End {
        ticks: u32,
        last_tick: Option<u32>,
        final_snapshot_hash: u64,
    },
    Failure {
        error: RunError,
    },
}

fn commands(input: &TickInput) -> Value {
    let mut actions = net::WireWriter::new();
    net::encode_actions(&mut actions, &input.actions);
    json!({"commands":input.cmds.iter().map(|command| json!({"client":command.client.0,"sequence":command.sequence.map(|sequence|sequence.0),"bytes":command.command.canonical_bytes().to_vec()})).collect::<Vec<_>>(),
        "actions_wire":actions.finish(),
        "shot_samples":input.shot_samples.iter().map(|((id,time),sample)| json!({"client":id.0,"time":time,"left":sample.left.0,"right":sample.right.0,"alpha":sample.alpha,"quality":sample.quality.dump_label()})).collect::<Vec<_>>()})
}

fn program_header(world: &SimWorld) -> Value {
    let Some(root) = world.input_program() else {
        return Value::Null;
    };
    json!({"ensemble_sha256":root.ensemble_digest().to_vec(),"actors":std::iter::once(root).chain(&root.actors).map(|actor|json!({
        "client":actor.program.client.0,"digest":actor.digest.to_vec(),"quantum_ms":actor.program.quantum_ms,
        "first_sequence":actor.program.first_sequence.0,"samples":actor.program.samples.len(),
        "base_command_time":actor.initial_player.command_time,"life":actor.life.0,
        "future_weapons":actor.program.future_weapons,"stop_on_death":actor.program.stop_on_death,
        "scoreboard_window_ms":actor.program.scoreboard_window_ms,
    })).collect::<Vec<_>>()})
}

fn program_receipts(world: &SimWorld, tick: Tick) -> Value {
    let Some(root) = world.input_program() else {
        return json!([]);
    };
    json!(std::iter::once(root).chain(&root.actors).flat_map(|actor|actor.receipts.iter().filter(move|receipt|receipt.tick==tick).map(move|receipt|json!({
        "client":actor.program.client.0,"life":actor.life.0,"sequence":receipt.command.sequence.map(|seq|seq.0),
        "source_bytes":receipt.source_command.command.canonical_bytes().to_vec(),
        "bytes":receipt.command.command.canonical_bytes().to_vec(),
        "origin":receipt.player.origin,"velocity":receipt.player.velocity,"health":receipt.player.health,"weapon":receipt.player.weapon,
    }))).collect::<Vec<_>>())
}

fn player_rows(snapshot: &sim::Snapshot) -> Value {
    json!(
        snapshot
            .players
            .iter()
            .map(|(client, player)| {
                let meta = snapshot.meta.for_client(*client);
                json!({"client":client.0,"life":meta.map(|meta|meta.life_sequence.0),
            "lifecycle":meta.map(|meta|format!("{:?}",meta.lifecycle)),
            "world":sim::world_graph::world_at(player.origin),"simulation_origin":player.origin,
            "velocity":player.velocity,"viewangles":player.viewangles,"health":player.health,
            "weapon":player.weapon,"command_time":player.command_time})
            })
            .collect::<Vec<_>>()
    )
}

fn event_rows(snapshot: &sim::Snapshot) -> Value {
    json!(snapshot.meta.journal.iter().filter(|record|record.tick==snapshot.tick).map(|record| {
        let event = match &record.event {
            sim::SimEvent::Spawned {class_id,spawn_index,life_sequence} => json!({"kind":"spawned","class":class_id.0,"spawn":spawn_index,"life":life_sequence.0}),
            sim::SimEvent::Died {victim,life_sequence,attacker,attacker_life,weapon,killcam_entity_start_time,..} => json!({"kind":"died","victim":victim.0,"life":life_sequence.0,"attacker":attacker.map(|id|id.0),"attacker_life":attacker_life.map(|life|life.0),"weapon":weapon,"killcam_start":killcam_entity_start_time}),
            sim::SimEvent::ScoreChanged {client,score,kills,deaths} => json!({"kind":"score_changed","client":client.0,"score":score,"kills":kills,"deaths":deaths}),
            sim::SimEvent::MatchEnded {reason} => json!({"kind":"match_ended","reason":format!("{reason:?}")}),
            sim::SimEvent::AttackReleased => json!({"kind":"attack_released"}),
            sim::SimEvent::WeaponSwitchRequested {weapon} => json!({"kind":"weapon_switch_requested","weapon":weapon}),
        };
        let audience=match &record.audience {
            sim::EventAudience::All => json!({"all":true}),
            sim::EventAudience::AllExcept(client) => json!({"except":client.0}),
            sim::EventAudience::Client(client) => json!({"clients":[client.0]}),
            sim::EventAudience::Clients(clients) => json!({"clients":clients.iter().map(|client|client.0).collect::<Vec<_>>()}),
        };
        json!({"tick":record.tick.0,"sequence":record.sequence.0,"audience":audience,"event":event})
    }).collect::<Vec<_>>())
}

fn frame(snapshot: &sim::Snapshot) -> Vec<u8> {
    let mut encoder = net::SnapshotEncoder::new();
    let frame = net::frame_from_tick(&mut encoder, snapshot);
    let mut writer = net::WireWriter::new();
    frame.encode(&mut writer);
    writer.finish()
}

struct LogSink<'a, W> {
    sink: &'a mut W,
    written: u64,
    limit: u64,
}
impl<W: Write> LogSink<'_, W> {
    fn emit(&mut self, tick: Tick, record: &RunRecord) -> Result<(), RunError> {
        let mut bytes = serde_json::to_vec(record)
            .map_err(|error| RunError::new(tick, "encode", error.to_string()))?;
        bytes.push(b'\n');
        if self.written.saturating_add(bytes.len() as u64) > self.limit {
            return Err(RunError::new(
                tick,
                "log_budget",
                "capture byte budget exhausted",
            ));
        }
        self.sink
            .write_all(&bytes)
            .map_err(|error| RunError::new(tick, "sink", error.to_string()))?;
        self.written += bytes.len() as u64;
        Ok(())
    }
}

pub fn run<S: InputSource>(
    mut request: RunRequest<S>,
    sink: &mut impl Write,
) -> Result<RunSummary, RunError> {
    let first = request.session.next_tick();
    if request.build_id.trim().is_empty() {
        return Err(RunError::new(
            first,
            "identity",
            "build_id must identify the caller build",
        ));
    }
    if request.limits.ticks == 0
        || request.limits.commands_per_tick == 0
        || request.limits.log_bytes == 0
    {
        return Err(RunError::new(
            first,
            "limits",
            "run limits must be positive",
        ));
    }
    let mut log = LogSink {
        sink,
        written: 0,
        limit: request.limits.log_bytes,
    };
    let result = (|| -> Result<(u32, Option<u32>, u64), RunError> {
        let initial = request
            .session
            .world()
            .snapshot(Tick(first.0.saturating_sub(1)));
        log.emit(
            first,
            &RunRecord::Header {
                build_id: request.build_id.clone(),
                platform: format!("{}-{}", std::env::consts::ARCH, std::env::consts::OS),
                schema: 1,
                protocol: net::PROTOCOL_VERSION,
                seed: request.session.world().root_seed(),
                content: request.session.world().content_digest(),
                first_tick: first.0,
                recipe: request.session.recipe.clone(),
                configuration: request.session.configuration.clone(),
                input_program: program_header(request.session.world()),
                initial_frame: frame(&initial),
            },
        )?;
        let mut last_tick = None;
        let mut ticks = 0;
        let mut state_hash = net::authoritative_snapshot_hash(&initial);
        for _ in 0..request.limits.ticks {
            let tick = request.session.next_tick();
            let mut context = InputContext {
                world: &mut request.session.prepared.world,
                tick,
            };
            let mut input = match request
                .input
                .input(&mut context)
                .map_err(|error| RunError::new(tick, "input", error))?
            {
                InputFrame::Finish => break,
                InputFrame::Cancel(reason) => return Err(RunError::new(tick, "canceled", reason)),
                InputFrame::Step(input) => input,
            };
            let entries = input
                .cmds
                .len()
                .saturating_add(input.actions.len())
                .saturating_add(input.shot_samples.len())
                .saturating_add(request.session.pending_tape_commands());
            if entries > request.limits.commands_per_tick {
                return Err(RunError::new(
                    tick,
                    "input_budget",
                    "tick input budget exhausted",
                ));
            }
            input.canonicalize();
            let result = request.session.step(&input)?;
            state_hash = net::authoritative_snapshot_hash(&result.snapshot);
            log.emit(
                tick,
                &RunRecord::Tick {
                    tick: tick.0,
                    submitted: commands(&input),
                    authority_input: commands(&result.input),
                    program_receipts: program_receipts(request.session.world(),tick),
                    players: player_rows(&result.snapshot),
                    events: event_rows(&result.snapshot),
                    shots: json!(result.accepted_shots.iter().map(|shot|json!({
                        "id":shot.shot_id.0,"client":shot.attacker.0,"life":shot.attacker_life.0,"weapon":shot.weapon,"hand":shot.hand,
                        "cause":shot.fire_cause.map(|cause|json!({"client":cause.client.0,"life":cause.life.0,"command":cause.command.0,"ordinal":cause.ordinal,"hand":cause.hand})),
                        "ammo_used":shot.ammo_used,"origin":shot.origin,"angles":shot.angles,"velocity":shot.owner_velocity,
                        "combat_seed":shot.combat_seed,"spread_degrees":shot.spread_degrees,"ads_frac":shot.ads_frac,
                    })).collect::<Vec<_>>()),
                    shot_verdicts:json!(request.session.world().shot_collision_verdicts().iter().map(|verdict|json!({"shot":verdict.shot_id.0,"terminal":format!("{:?}",verdict.terminal),"geometry":format!("{:?}",verdict.geometry),"end":verdict.end,"startsolid":verdict.startsolid})).collect::<Vec<_>>()),
                    projectile_impacts:json!(request.session.world().projectile_impacts().iter().map(|impact|json!({"id":impact.id.0,"client":impact.owner.0,"weapon":impact.weapon,"origin":impact.origin,"terminal":format!("{:?}",impact.terminal),"geometry":format!("{:?}",impact.geometry),"amount":impact.amount})).collect::<Vec<_>>()),
                    frame: frame(&result.snapshot),
                    state_hash,
                },
            )?;
            last_tick = Some(tick.0);
            ticks += 1;
            if let Some(run) = request.session.world().input_program()
                && let Some(error) = &run.failure
            {
                return Err(RunError::new(tick, "ownership", error.clone()));
            }
        }
        if request
            .session
            .world()
            .input_program()
            .is_some_and(|program| !program.complete())
        {
            return Err(RunError::new(
                request.session.next_tick(),
                "incomplete",
                "input program has unconsumed samples",
            ));
        }
        log.emit(
            request.session.next_tick(),
            &RunRecord::End {
                ticks,
                last_tick,
                final_snapshot_hash: state_hash,
            },
        )?;
        Ok((ticks, last_tick, state_hash))
    })();
    match result {
        Ok((ticks, last_tick, final_snapshot_hash)) => {
            log.sink.flush().map_err(|error| {
                RunError::new(request.session.next_tick(), "sink", error.to_string())
            })?;
            Ok(RunSummary {
                ticks,
                last_tick,
                log_bytes: log.written,
                final_snapshot_hash,
            })
        }
        Err(error) => {
            let _ = log.emit(
                Tick(error.tick),
                &RunRecord::Failure {
                    error: RunError::new(Tick(error.tick), &error.stage, &error.message),
                },
            );
            let _ = log.sink.flush();
            Err(error)
        }
    }
}
