use core::fmt;
use core::ops::{Deref, DerefMut};

use bevy_ecs::prelude::{Entity, World};
use playerstate_iw4::PlayerState;

use crate::frame::{
    FrameWorld, PayloadIndex, collect_dropped_items, collect_players, collect_projectiles,
    collect_script_movers, install_state_entity, player_ref,
};
use crate::gentity::ScriptMoverGentity;
use crate::identities::ScriptModelId;
use crate::input::TickInput;
use crate::snapshot::Snapshot;
use crate::world::{ClientId, SimState, Tick};

pub struct SimWorld {
    ecs: World,
    state_entity: Entity,
}

impl Default for SimWorld {
    fn default() -> Self {
        let mut ecs = World::new();
        let state_entity = ecs.spawn(SimState::default()).id();
        ecs.entity_mut(state_entity).insert(PayloadIndex::default());
        install_state_entity(&mut ecs, state_entity);
        ecs.insert_resource(crate::LocalPlayerProfile::default());
        ecs.insert_resource(crate::PersistentDataStore::default());
        ecs.init_resource::<crate::script::MatchScript>();
        ecs.insert_resource(crate::script::RoundScript::new(
            ecs.resource::<crate::script::MatchScript>(),
        ));
        ecs.insert_resource(crate::script::Mechanics::default());
        ecs.insert_resource(crate::script::NativeRegistry::default());
        Self { ecs, state_entity }
    }
}

impl fmt::Debug for SimWorld {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("SimWorld").field("state", &**self).finish()
    }
}

impl Deref for SimWorld {
    type Target = SimState;

    fn deref(&self) -> &Self::Target {
        self.ecs
            .get::<SimState>(self.state_entity)
            .expect("simulation state entity is missing its SimState component")
    }
}

impl DerefMut for SimWorld {
    fn deref_mut(&mut self) -> &mut Self::Target {
        self.ecs
            .get_mut::<SimState>(self.state_entity)
            .expect("simulation state entity is missing its SimState component")
            .into_inner()
    }
}

impl SimWorld {
    pub fn validate_entity_lifetimes(&self) -> Result<(), String> {
        crate::frame::validate_entity_payloads(&self.ecs)
    }

    pub fn entity_lifetime_summary(&self) -> String {
        let snapshot = self.entity_kernel().to_snapshot();
        let occupied = snapshot
            .slots
            .iter()
            .filter(|slot| slot.occupied.is_some())
            .count();
        let held = snapshot
            .slots
            .iter()
            .filter(|slot| {
                slot.occupied
                    .is_some_and(|slot| slot.transient_event_time_ms.is_some())
            })
            .count();
        let runtime = self.ecs.resource::<crate::script::RoundScript>();
        format!(
            "occupied_slots = {occupied}\nevent_holds = {held}\nprojectile_payloads = {}\nitem_payloads = {}\nmover_payloads = {}\nscript_objects = {}\nscript_defined_entities = {}\nentity_consistency = {:?}\nmap_glass = {:?}\n",
            collect_projectiles(&self.ecs).len(),
            collect_dropped_items(&self.ecs).len(),
            collect_script_movers(&self.ecs).len(),
            runtime.objects.len(),
            runtime
                .entities
                .keys()
                .filter(|id| runtime.script_is_defined(id))
                .count(),
            self.validate_entity_lifetimes(),
            self.content().map().glass
        )
    }

    pub fn new() -> Self {
        Self::default()
    }

    pub fn persistent_data(&self) -> &crate::PersistentDataStore {
        self.ecs.resource::<crate::PersistentDataStore>()
    }

    pub fn persistent_data_mut(&mut self) -> &mut crate::PersistentDataStore {
        self.ecs
            .resource_mut::<crate::PersistentDataStore>()
            .into_inner()
    }

    pub fn set_persistent_data(&mut self, store: crate::PersistentDataStore) {
        self.ecs.insert_resource(store);
    }

    pub fn local_player_profile(&self) -> crate::LocalPlayerProfile {
        *self.ecs.resource::<crate::LocalPlayerProfile>()
    }

    pub fn set_local_player_profile(&mut self, profile: crate::LocalPlayerProfile) {
        self.ecs.insert_resource(profile);
    }

    pub fn frame(&mut self) -> FrameWorld<'_> {
        FrameWorld::new(&mut self.ecs, self.state_entity)
    }

    pub fn input_program(&self) -> Option<&crate::input_program::InputProgramRun> {
        self.ecs
            .get_resource::<crate::input_program::InputProgramRun>()
    }

    pub fn input_program_owns(&self, client: ClientId) -> bool {
        self.input_program()
            .is_some_and(|run| run.actor(client).is_some())
    }

    pub fn install_input_program(
        &mut self,
        program: crate::input_program::InputProgram,
    ) -> Result<(), String> {
        self.install_input_ensemble(program, Vec::new())
    }

    pub fn install_input_ensemble(
        &mut self,
        program: crate::input_program::InputProgram,
        mut actors: Vec<crate::input_program::InputProgram>,
    ) -> Result<(), String> {
        if self.input_program().is_some() {
            return Err(
                "input program already owns this match; reload the map for a new take".into(),
            );
        }
        actors.sort_by_key(|actor| actor.client);
        let mut owners = std::collections::BTreeSet::from([program.client]);
        for actor in &actors {
            if !owners.insert(actor.client) {
                return Err("duplicate input program actor".into());
            }
        }
        let mut run = crate::input_program::InputProgramRun::prepare(self, program)?;
        run.actors = actors
            .into_iter()
            .map(|actor| crate::input_program::InputProgramRun::prepare(self, actor))
            .collect::<Result<_, _>>()?;
        self.ecs.insert_resource(run);
        Ok(())
    }

    fn run(
        &mut self,
        tick: Tick,
        input: &TickInput,
        msec: i32,
    ) -> Result<crate::TickResult, crate::script::Fault> {
        let resolved = if let Some(mut run) = self
            .ecs
            .remove_resource::<crate::input_program::InputProgramRun>()
        {
            // Ownership outlives capture, so a forgotten tape cannot grow logs forever.
            let duration = std::iter::once(&run)
                .chain(&run.actors)
                .filter_map(|actor| actor.program.samples.last().map(|cmd| cmd.server_time))
                .max()
                .unwrap_or(0);
            if run.start_tick.is_some_and(|start| {
                i64::from(tick.0.saturating_sub(start.0)) * 50 > i64::from(duration) + 120_000
            }) {
                run.failure
                    .get_or_insert("input program capture exceeded 120 seconds of postroll".into());
            }
            let life = self
                .client_meta(run.program.client)
                .map(|meta| meta.life_sequence);
            let lifecycle = self
                .client_meta(run.program.client)
                .map(|meta| meta.lifecycle);
            let player = self.player(run.program.client).copied();
            let alternate = |mapped: u16, selected: u16| {
                self.combat_facts_for(u32::from(mapped))
                    .is_some_and(|facts| facts.alternate_weapon == u32::from(selected))
            };
            let mut resolved = run.input(
                tick,
                input,
                msec,
                crate::input_program::InputActorState {
                    life,
                    lifecycle,
                    player: player.as_ref(),
                },
                alternate,
            );
            if run.failure.is_none() {
                for actor in &mut run.actors {
                    let life = self
                        .client_meta(actor.program.client)
                        .map(|meta| meta.life_sequence);
                    let lifecycle = self
                        .client_meta(actor.program.client)
                        .map(|meta| meta.lifecycle);
                    let player = self.player(actor.program.client).copied();
                    resolved = actor.input(
                        tick,
                        &resolved,
                        msec,
                        crate::input_program::InputActorState {
                            life,
                            lifecycle,
                            player: player.as_ref(),
                        },
                        alternate,
                    );
                    if let Some(error) = &actor.failure {
                        run.failure = Some(format!("actor {}: {error}", actor.program.client.0));
                        break;
                    }
                }
            }
            if run.failure.is_some() {
                resolved.cmds.retain(|cmd| run.actor(cmd.client).is_none());
                resolved
                    .shot_samples
                    .retain(|((id, _), _)| run.actor(*id).is_none());
                resolved.actions.retain(|(id, _)| run.actor(*id).is_none());
            }
            self.ecs.insert_resource(run);
            resolved
        } else {
            input.clone()
        };
        let result = crate::step::run_frame(&mut self.ecs, tick, &resolved, msec)?;
        if self
            .input_program()
            .is_none_or(|run| run.start_tick.is_none() || run.failure.is_some())
        {
            return Ok(result);
        }
        let final_seats = self.script_seats();
        let ended = matches!(self.ecs.resource_mut::<crate::script::RoundScript>().object_field(0,"gameended"), crate::script::Value::Int(n) if n!=0);
        let score_limit = self
            .ecs
            .resource::<crate::script::MatchScript>()
            .dvars
            .get("ui_scorelimit")
            .and_then(|v| v.parse().ok())
            .unwrap_or(result.snapshot.meta.score_limit);
        let scores = result
            .snapshot
            .meta
            .clients
            .iter()
            .map(|(id, m)| (*id, m.score, m.kills, m.deaths))
            .collect();
        let match_state = crate::input_program::InputMatchState {
            tick,
            ended,
            score_limit,
            scores,
        };
        if let Some(mut run) = self
            .ecs
            .get_resource_mut::<crate::input_program::InputProgramRun>()
            && run.start_tick.is_some()
        {
            if result
                .snapshot
                .meta
                .script_dvars(run.program.client)
                .int("iw4l_ui_scores")
                .unwrap_or(0)
                != 0
                && let Some((_, ps)) = result
                    .snapshot
                    .players
                    .iter()
                    .find(|(id, _)| *id == run.program.client)
            {
                run.scoreboard_samples.push((tick, ps.origin));
            }
            if run.match_states.last().is_none_or(|last| {
                last.ended != match_state.ended
                    || last.score_limit != match_state.score_limit
                    || last.scores != match_state.scores
            }) {
                run.match_states.push(match_state);
            }
            let seats: Vec<_> = final_seats
                .iter()
                .filter(|(_, seat)| seat.final_kill)
                .map(|(client, seat)| (*client, seat.spectator_client, seat.length_ms))
                .collect();
            crate::input_program::observe_final_killcams(&mut run.final_killcams, tick, &seats);
            run.events.extend(
                result
                    .snapshot
                    .meta
                    .journal
                    .iter()
                    .filter(|record| record.tick == tick)
                    .cloned(),
            );
            let last_seq = run.pickups.last().map_or(0, |(_, pickup)| pickup.seq);
            run.pickups.extend(
                result
                    .snapshot
                    .meta
                    .item_pickups
                    .iter()
                    .copied()
                    .filter(|pickup| pickup.seq > last_seq)
                    .map(|pickup| (tick, pickup)),
            );
        }
        Ok(result)
    }

    pub fn visit_projectiles(&self, visit: impl FnMut(&crate::ProjectileState)) {
        crate::frame::visit_projectiles(&self.ecs, visit);
    }

    pub fn script_mover_count(&self) -> usize {
        crate::frame::script_mover_row_count(&self.ecs)
    }

    pub fn visit_script_movers(&self, visit: impl FnMut(&ScriptMoverGentity)) {
        crate::frame::visit_script_movers(&self.ecs, visit);
    }

    pub fn script_mover_by_number(&self, number: i32) -> Option<ScriptMoverGentity> {
        crate::frame::script_mover_by_number(&self.ecs, number)
    }

    pub fn script_mover_by_id(&self, id: ScriptModelId) -> Option<ScriptMoverGentity> {
        crate::frame::script_mover_by_id(&self.ecs, id)
    }

    pub fn install_gsc_program(
        &mut self,
        program: crate::script::Program,
        natives: crate::script::NativeRegistry,
        level: crate::script::LevelData,
    ) -> Result<(), crate::script::Fault> {
        crate::script::install(&mut self.ecs, program, natives, level)
    }

    pub fn start_gsc(
        &mut self,
        name: &str,
        receiver: crate::script::Value,
        arguments: Vec<crate::script::Value>,
    ) -> Result<u64, crate::script::Fault> {
        crate::script::start(&mut self.ecs, name, receiver, arguments)
    }

    /// Registers the local FoF floats before installing scripts on listen authority.
    pub fn register_local_presentation_dvars(&mut self, local: Option<ClientId>) {
        crate::script::host::natives::iw4::register_local_presentation_dvars(&mut self.ecs, local);
    }

    pub fn set_gsc_dvar(&mut self, name: &str, value: &str) {
        crate::script::set_dvar(&mut self.ecs, name, value);
    }

    pub fn gsc_realm(&self) -> Option<crate::script::FamilyId> {
        self.ecs
            .resource::<crate::script::RoundScript>()
            .program
            .as_ref()
            .map(|program| program.rules())
    }

    pub fn set_script_archive_time(
        &mut self,
        client: ClientId,
        requested_ms: i32,
        attained_ms: i32,
    ) {
        let mut runtime = self.ecs.resource_mut::<crate::script::RoundScript>();
        if let Some(slot) = runtime.players.get_mut(&client.0)
            && slot.seat.archive_ms == requested_ms
        {
            slot.seat.archive_ms = attained_ms;
        }
    }

    pub fn script_seats(&self) -> Vec<(ClientId, crate::ScriptSeat)> {
        crate::script::script_seats(&self.ecs)
    }

    pub fn take_script_fault(&mut self) -> Option<crate::script::Fault> {
        let mut runtime = self.ecs.resource_mut::<crate::script::RoundScript>();
        if runtime.fault_reported {
            return None;
        }
        runtime.fault_reported = runtime.fault.is_some();
        runtime.fault.clone()
    }

    pub fn spawn_script_mover(
        &mut self,
        id: ScriptModelId,
        origin: [f32; 3],
        angles: [f32; 3],
    ) -> Result<i32, crate::EntityAllocError> {
        self.frame().spawn_script_mover(id, origin, angles)
    }

    pub fn spawn_brush_mover(
        &mut self,
        id: ScriptModelId,
        cmodel: u32,
        origin: [f32; 3],
        angles: [f32; 3],
    ) -> Result<i32, crate::EntityAllocError> {
        let mut frame = self.frame();
        let number = frame.spawn_script_mover(id, origin, angles)?;
        if let Some(mover) = frame.script_mover_mut_by_number(number) {
            mover.state.index = cmodel as i32;
            mover.state.solid = entity_iw4::SCRIPT_MOVER_BMODEL_SOLID;
        }
        Ok(number)
    }

    pub fn gentity_number(&self, id: ScriptModelId) -> Option<i32> {
        self.script_mover_by_id(id).map(|mover| mover.state.number)
    }

    pub fn begin_script_mover_rotate_velocity(
        &mut self,
        id: ScriptModelId,
        speed: [f32; 3],
        total_time_seconds: f32,
        level_time_ms: i32,
    ) -> bool {
        self.frame().begin_script_mover_rotate_velocity(
            id,
            speed,
            total_time_seconds,
            level_time_ms,
        )
    }

    pub fn begin_script_movers_rotate_velocity_supplied(
        &mut self,
        speed: f32,
        level_time_ms: i32,
    ) -> usize {
        self.frame()
            .begin_script_movers_rotate_velocity_supplied(speed, level_time_ms)
    }

    pub fn set_script_mover_r_box(
        &mut self,
        number: i32,
        box_mid: [f32; 3],
        box_half: [f32; 3],
    ) -> bool {
        self.frame()
            .set_script_mover_r_box(number, box_mid, box_half)
    }

    pub fn set_script_mover_origin(&mut self, number: i32, origin: [f32; 3]) -> bool {
        self.frame().set_script_mover_origin(number, origin)
    }

    pub fn player(&self, id: ClientId) -> Option<&PlayerState> {
        player_ref(&self.ecs, id)
    }

    pub fn prepare_input_actor(
        &mut self,
        id: ClientId,
        p: &crate::input_program::InputActorPreparation,
    ) -> Result<(), String> {
        if self.input_program().is_some() || !self.cheats_enabled() || !self.is_running() {
            return Err(
                "cast preparation needs an active cheat authority before input programs".into(),
            );
        }
        if !(0..=69).contains(&p.rank)
            || !(0..=10).contains(&p.prestige)
            || p.name.is_empty()
            || p.name.len() > 15
            || p.name.bytes().any(|b| b < 32)
            || p.score < 0
            || p.kills < 0
            || p.deaths < 0
        {
            return Err("invalid cast identity or scoreboard".into());
        }
        if self
            .client_meta(id)
            .is_none_or(|m| m.lifecycle != crate::ClientLifecycle::Alive)
        {
            return Err("cast actor must be alive".into());
        }
        if let Some(streak) = &p.streak {
            if streak.count > p.kills {
                return Err("live streak cannot exceed total kills".into());
            }
            crate::script::host::players::validate_streak(&self.ecs, streak)?;
        }
        crate::script::host::players::prepare_card(&mut self.ecs, id.0, p.title, p.icon)?;
        crate::script::host::players::prepare_round_stats(
            &mut self.ecs,
            id.0,
            p.score,
            p.kills,
            p.deaths,
        )?;
        if let Some(streak) = &p.streak {
            self.ecs
                .get_resource_or_init::<crate::input_program::PendingInputStreaks>()
                .0
                .insert(id, streak.clone());
        }
        let mut frame = self.frame();
        let meta = frame.client_meta_mut(id);
        meta.name = entity_iw4::pack_client_state_name(&p.name);
        meta.rank = p.rank;
        meta.prestige = p.prestige;
        meta.score = p.score;
        meta.kills = p.kills;
        meta.deaths = p.deaths;
        Ok(())
    }

    pub fn input_actor_streak(&self, id: ClientId) -> crate::input_program::InputStreakState {
        crate::script::host::players::streak_state(&self.ecs, id.0)
    }

    pub fn prepare_input_team_scores(&mut self, allies: i32, axis: i32) -> Result<(), String> {
        if self.input_program().is_some()
            || !self.cheats_enabled()
            || !self.is_running()
            || !matches!(
                self.phase(),
                crate::MatchPhase::Warmup | crate::MatchPhase::Playing
            )
        {
            return Err(
                "score preparation needs an active cheat authority before input programs".into(),
            );
        }
        if !self.game_mode_kind().is_team() {
            return Err("score preparation requires a team mode".into());
        }
        let limit = self
            .ecs
            .get_resource::<crate::script::MatchScript>()
            .and_then(|script| script.dvars.get("ui_scorelimit"))
            .and_then(|value| value.parse().ok())
            .unwrap_or(self.bootstrap_ref().score_limit);
        if [allies, axis]
            .iter()
            .any(|&score| score < 0 || (limit > 0 && score >= limit))
        {
            return Err(
                "prepared team scores must be nonnegative and below the score limit".into(),
            );
        }
        crate::script::host::objectives::prepare_team_scores(&mut self.ecs, allies, axis)?;
        self.objectives.scores = [0, axis, allies];
        Ok(())
    }

    pub fn prepare_input_actor_health(&mut self, id: ClientId, health: i32) -> Result<(), String> {
        if self.input_program().is_some() {
            return Err("actor health must be prepared before installing input programs".into());
        }
        let mut frame = self.frame();
        if !frame.cheats_enabled() || !frame.is_running() {
            return Err("actor preparation needs an active authority with cheats".into());
        }
        if !(1..=100_000).contains(&health) {
            return Err("actor health must be 1..100000".into());
        }
        if frame
            .client_meta(id)
            .is_none_or(|meta| meta.lifecycle != crate::ClientLifecycle::Alive)
            || frame.player(id).is_none()
        {
            return Err("actor must be prepared alive".into());
        }
        frame.client_meta_mut(id).max_health = health;
        let ps = frame.player_mut(id).expect("validated actor");
        ps.max_health = health;
        ps.health = health;
        Ok(())
    }

    pub fn visit_players(&self, visit: impl FnMut(ClientId, &PlayerState)) {
        crate::frame::visit_players(&self.ecs, visit);
    }

    pub fn player_count(&self) -> usize {
        crate::frame::player_row_count(&self.ecs)
    }

    pub fn retire_client(&mut self, id: ClientId) {
        let mut runtime = self.ecs.resource_mut::<crate::script::RoundScript>();
        if runtime.players.contains_key(&id.0) {
            runtime.disconnects.insert(id.0);
        } else {
            self.frame().retire_client(id);
            self.ecs
                .resource_mut::<crate::PersistentDataStore>()
                .unbind(id);
        }
    }

    pub fn set_origin(&mut self, id: ClientId, origin: [f32; 3]) -> bool {
        self.frame().set_origin(id, origin)
    }

    pub fn set_legs_anim(&mut self, id: ClientId, legs_anim: i32) -> bool {
        self.frame().set_legs_anim(id, legs_anim)
    }

    pub fn set_viewangles(&mut self, id: ClientId, viewangles: [f32; 3]) -> bool {
        self.frame().set_viewangles(id, viewangles)
    }

    pub fn set_e_flags(&mut self, id: ClientId, e_flags: u32) -> bool {
        self.frame().set_e_flags(id, e_flags)
    }

    pub fn set_leanf(&mut self, id: ClientId, leanf: f32) -> bool {
        self.frame().set_leanf(id, leanf)
    }

    pub fn set_view_height_target(&mut self, id: ClientId, view_height_target: i32) -> bool {
        self.frame().set_view_height_target(id, view_height_target)
    }

    pub fn gate_nudge_origin(&mut self, id: ClientId, delta: [f32; 3]) {
        self.frame().gate_nudge_origin(id, delta);
    }

    pub fn snapshot(&self, tick: Tick) -> Snapshot {
        let state = self
            .ecs
            .get::<SimState>(self.state_entity)
            .expect("simulation state entity is missing its SimState component");
        state.snapshot_with_dynamic_rows(
            tick,
            collect_players(&self.ecs),
            collect_projectiles(&self.ecs),
            collect_script_movers(&self.ecs),
            collect_dropped_items(&self.ecs),
        )
    }

    pub fn slow_motion(&self) -> Option<crate::ScriptSlowMotion> {
        self.ecs
            .resource::<crate::script::RoundScript>()
            .engine
            .slow_motion
    }

    pub fn request_round_restart(&mut self) -> Result<(), String> {
        if !self.is_running() || !self.cheats_enabled() {
            return Err("round restart needs an active match with cheats".into());
        }
        self.ecs
            .resource_mut::<crate::script::RoundScript>()
            .request_round_restart(true)
    }

    pub fn shutdown_game(&mut self) {
        self.frame().shutdown_game();
        crate::script::reset(&mut self.ecs);
        self.ecs
            .resource_mut::<crate::PersistentDataStore>()
            .clear_bindings();
    }

    pub fn hitvol_dump(&self) -> Vec<crate::world::HitvolDumpRow> {
        SimState::hitvol_dump(self, |id| player_ref(&self.ecs, id).copied())
    }

    pub fn lagcomp_query_for(
        &self,
        attacker: ClientId,
        current: Tick,
    ) -> crate::bullet_collision::LagcompQuery {
        SimState::lagcomp_query_for(self, attacker, current, |id| {
            player_ref(&self.ecs, id).copied()
        })
    }
}

pub fn step(world: &mut SimWorld, tick: Tick, input: &TickInput, msec: i32) -> crate::TickResult {
    try_step(world, tick, input, msec)
        .unwrap_or_else(|fault| panic!("GSC execution failed: {fault}"))
}

pub fn try_step(
    world: &mut SimWorld,
    tick: Tick,
    input: &TickInput,
    msec: i32,
) -> Result<crate::TickResult, crate::script::Fault> {
    world.run(tick, input, msec)
}
