use core::fmt;
use core::ops::{Deref, DerefMut};

use bevy_ecs::prelude::{Entity, World};
use playerstate_iw4::PlayerState;

use crate::frame::{
    FrameWorld, PayloadIndex, collect_dropped_items, collect_players, collect_projectiles,
    collect_script_movers, install_state_entity, player_ref, spawn_payload_rows,
};
use crate::gentity::ScriptMoverGentity;
use crate::identities::ScriptModelId;
use crate::input::TickInput;
use crate::snapshot::Snapshot;
use crate::world::{ClientId, SimState, Tick};

pub struct SimWorld {
    ecs: World,
    state_entity: Entity,
    schedule: bevy_ecs::schedule::Schedule,
}

impl Default for SimWorld {
    fn default() -> Self {
        let mut ecs = World::new();
        let state_entity = ecs.spawn(SimState::default()).id();
        ecs.entity_mut(state_entity).insert(PayloadIndex::default());
        install_state_entity(&mut ecs, state_entity);
        Self {
            ecs,
            state_entity,
            schedule: crate::step::schedule(),
        }
    }
}

impl Clone for SimWorld {
    fn clone(&self) -> Self {
        let mut ecs = World::new();
        let state_entity = ecs.spawn((**self).clone()).id();
        ecs.entity_mut(state_entity).insert(PayloadIndex::default());
        install_state_entity(&mut ecs, state_entity);
        spawn_payload_rows(
            &mut ecs,
            collect_players(&self.ecs),
            collect_projectiles(&self.ecs),
            collect_script_movers(&self.ecs),
            collect_dropped_items(&self.ecs),
        );
        Self {
            ecs,
            state_entity,
            schedule: crate::step::schedule(),
        }
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
    pub fn new() -> Self {
        Self::default()
    }

    pub(crate) fn frame(&mut self) -> FrameWorld<'_> {
        FrameWorld::new(&mut self.ecs, self.state_entity)
    }

    fn run(&mut self, tick: Tick, input: &TickInput, msec: i32, reason: StepReason) -> Snapshot {
        crate::step::run_schedule(&mut self.ecs, &mut self.schedule, tick, input, msec, reason)
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

    pub fn spawn_script_mover(
        &mut self,
        id: ScriptModelId,
        origin: [f32; 3],
        angles: [f32; 3],
    ) -> Result<i32, crate::EntityAllocError> {
        self.frame().spawn_script_mover(id, origin, angles)
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

    pub fn install_use_object_from_ent(
        &mut self,
        classname: &str,
        number: i32,
        use_time_seconds: f32,
    ) -> Result<u32, crate::use_object::MapUseBindError> {
        self.frame()
            .install_use_object_from_ent(classname, number, use_time_seconds)
    }

    pub fn install_dom_flags(
        &mut self,
        ents: &[gamemode_iw4::DomFlagMapEnt<'_>],
    ) -> Result<Vec<u32>, crate::use_object::DomFlagInstallError> {
        self.frame().install_dom_flags(ents)
    }

    pub fn install_trigger_radius_on_ent(
        &mut self,
        number: i32,
        radius: Option<f32>,
        height: Option<f32>,
        use_time_seconds: f32,
    ) -> Result<u32, crate::use_object::MapUseBindError> {
        self.frame()
            .install_trigger_radius_on_ent(number, radius, height, use_time_seconds)
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

    pub fn visit_players(&self, visit: impl FnMut(ClientId, &PlayerState)) {
        crate::frame::visit_players(&self.ecs, visit);
    }

    pub fn player_count(&self) -> usize {
        crate::frame::player_row_count(&self.ecs)
    }

    pub fn retire_client(&mut self, id: ClientId) {
        self.frame().retire_client(id);
    }

    pub fn debug_place_alive_player(&mut self, id: ClientId, origin: [f32; 3]) {
        self.frame().debug_place_alive_player(id, origin);
    }

    pub fn debug_set_held_ammo(&mut self, id: ClientId, clip: i32, stock: i32) {
        self.frame().debug_set_held_ammo(id, clip, stock);
    }

    pub fn debug_set_team(&mut self, id: ClientId, team: i32) {
        self.frame().debug_set_team(id, team);
    }

    pub fn debug_mark_dead(&mut self, id: ClientId) {
        self.frame().debug_mark_dead(id);
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

    pub fn adopt_snapshot(&mut self, snapshot: &Snapshot) -> crate::AdoptReport {
        self.frame().adopt_snapshot(snapshot)
    }

    pub fn adopt_prediction_snapshot(
        &mut self,
        snapshot: &Snapshot,
        local: ClientId,
    ) -> crate::AdoptReport {
        self.frame().adopt_prediction_snapshot(snapshot, local)
    }

    pub fn shutdown_game(&mut self) {
        self.frame().shutdown_game();
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

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum StepReason {
    #[default]
    AuthorityFrame,

    PredictNew,

    Replay,
}

impl StepReason {
    pub const fn advances_authority_world(self) -> bool {
        matches!(self, Self::AuthorityFrame)
    }

    pub const fn is_replay(self) -> bool {
        matches!(self, Self::Replay)
    }
}

pub fn step(
    world: &mut SimWorld,
    tick: Tick,
    input: &TickInput,
    msec: i32,
    reason: StepReason,
) -> Snapshot {
    world.run(tick, input, msec, reason)
}
