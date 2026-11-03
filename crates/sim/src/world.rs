mod bodies;
mod collision;
mod content;
mod events;
mod portal_clearance;
use bodies::{PlayerAnimInputs, PlayerBodyRuntime};
use collision::{CollisionRuntime, LagcompPlan};
pub use content::{
    PlayerKitCollision, SimBrush, SimClipBsp, SimClipCmodels, SimClipMesh, SimContent,
    SimContentBuilder, SimStaticModel, SimTriggerHull,
};
use events::{EventJournal, PresentationQueue};
pub use events::{PendingLocalSound, PendingPlayerCardEvent, PendingPlayerCardKind, PendingPrint};

use anim_iw4::{PLAYER_ANIM_RAW_MASK, PlayerAnimValue};
use bevy_ecs::prelude::Component;
use playerstate_iw4::{AnimPair, PlayerState};

use crate::bullet_collision::{
    CollisionHistory, EntityCollisionCapabilities, EntityCollisionHistory,
    EntityCollisionTraceGeom, LinkedBrushCollisionBrush,
};
use crate::equipment::{EquipmentRuntimeFacts, ProjectileImpact, ProjectileState};
use crate::identities::{
    EventSequence, MatchPhase, MatchRng, ProjectileId, RngDomain, ScriptModelId, ShotId,
};
use crate::match_state::{
    ClientMatchState, EntityEventPayload, EntityEventRecord, EventAudience, SimEvent, SnapshotMeta,
};
use crate::player_anim_script::PlayerAnimScript;
use crate::snapshot::Snapshot;
use crate::spawn::MatchBootstrap;
use crate::world_objects::WorldObjectState;
use crate::{WeaponScriptSounds, WeaponSetup};
use std::collections::HashMap;
use std::sync::Arc;
use weapon_iw4::WeaponCombatFacts;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Tick(pub u32);

pub use playerstate_iw4::ClientId;

pub(crate) const CONTENTS_BODY: u32 = 0x0200_0000;

#[derive(Clone, Debug, PartialEq)]
pub struct HitvolDumpRow {
    pub client: Option<ClientId>,
    pub bone_count: u32,

    pub geom: &'static str,
    pub body_key: String,
    pub head_key: String,

    pub pose_kind: &'static str,

    pub anim: Option<AnimPair>,

    pub leaf: Option<i64>,
    pub clip: String,

    pub anim_time: Option<f32>,
    pub first_cx: Option<f32>,
    pub first_cy: Option<f32>,
    pub first_cz: Option<f32>,

    pub pelvis_hx: Option<f32>,
    pub pelvis_hy: Option<f32>,
    pub pelvis_hz: Option<f32>,

    pub pelvis_cz: Option<f32>,

    pub head_x: Option<f32>,
    pub head_y: Option<f32>,
    pub head_z: Option<f32>,

    pub controller: &'static str,

    pub pitch: Option<f32>,

    pub e_flags: Option<u32>,

    pub pm_flags: Option<u32>,

    pub movetype: Option<i64>,

    pub leanf: Option<f32>,

    pub ctl_tags: Option<i64>,

    pub tag_origin: Option<i64>,
    pub tag_ox: Option<f32>,
    pub tag_oy: Option<f32>,
    pub pen_table_loaded: bool,
    pub error: Option<String>,

    pub clock_owner: Option<&'static str>,

    pub tree_persist: Option<i64>,

    pub node_time: Option<f32>,

    pub time_unit: Option<&'static str>,
    pub goal_weight: Option<f32>,

    pub dobj_persist: Option<i64>,

    pub cycle_count: Option<i64>,

    pub dobj_models: Option<i64>,
}

#[derive(Component, Clone, Debug)]
pub struct SimState {
    content: Arc<SimContent>,
    clients: Vec<(ClientId, ClientMatchState)>,

    area_entity_world: Option<clipmap_iw4::AreaEntityWorld>,

    entity_collision_capabilities: Vec<EntityCollisionCapabilities>,

    model_library: Arc<
        std::collections::BTreeMap<String, Option<Arc<xmodel_runtime::RetainedModelCapability>>>,
    >,

    old_buttons: Vec<(ClientId, u32)>,

    old_cmd_angles: Vec<(ClientId, [i32; 3])>,

    player_bodies: PlayerBodyRuntime,

    g_hudelems: Vec<crate::hudelem::GameHudElemSlot>,

    hud_elem_sound_ids: crate::hudelem::PulseFxSoundIds,

    dying_missiles: Vec<entity_iw4::EntityState>,

    bootstrap: MatchBootstrap,

    running: bool,
    phase: MatchPhase,
    round_serial: u32,

    match_elapsed_ms: u32,

    prematch: gamemode_iw4::PrematchStep,

    max_alive_seen: u32,

    game_win_winner: Option<ClientId>,

    placement_cointoss_unwired: u32,

    root_seed: u64,
    spawn_rng: MatchRng,
    combat_rng: MatchRng,
    bot_rng: MatchRng,

    next_shot: ShotId,
    next_projectile: ProjectileId,

    collision_runtime: CollisionRuntime,

    content_digest: u64,

    content_components: crate::ContentComponents,

    events: EventJournal,

    shot_collision_verdicts: Vec<crate::combat::ShotCollisionVerdict>,

    projectile_impacts: Vec<ProjectileImpact>,

    projectile_impact_log: Vec<(Tick, ProjectileImpact)>,

    world_objects: WorldObjectState,
    portal_clearance_signature: Option<u64>,
    portal_clearance_rows: Vec<[Option<u64>; 2]>,
    portal_clearance_validated: Vec<[Option<u64>; 2]>,
    portal_clearance_job: Option<portal_clearance::ClearanceJob>,

    sound_alias_cs: crate::SoundAliasCs,

    effect_name_cs: crate::EffectNameCs,

    hud_material_cs: crate::HudMaterialCs,

    hud_string_cs: crate::HudStringCs,

    num_kills: u32,

    pub(crate) recent_kills: Vec<(ClientId, i32, u32)>,
    pub(crate) weapon_notes: Vec<crate::equipment::WeaponNote>,

    presentation: PresentationQueue,

    pending_final_kill: Option<(ClientId, ClientId)>,

    last_pmove_walking: HashMap<ClientId, i32>,

    stuck_holdrand: u32,

    last_stuck_ejects: Vec<(ClientId, ClientId)>,

    last_anim_movement: HashMap<ClientId, (u8, u8)>,
    anim_command_buttons: HashMap<ClientId, u32>,

    anim_event_seed: u32,

    corpses: crate::PlayerCorpsePool,

    entity_kernel: crate::gentity::EntityKernel,

    dobj_anim_mats: HashMap<(i32, i32), entity_iw4::DObjAnimMat>,

    kernel_phases: Vec<crate::gentity::KernelPhase>,

    last_think_order: Vec<crate::gentity::EntityRef>,

    last_think_dispatch: Vec<(i32, crate::gentity::EntityRunKind)>,
    last_use_presses: Vec<crate::gentity::UsePress>,

    pub objectives: crate::ObjectiveMatch,

    use_start_spawns: bool,

    item_pickups: Vec<crate::ItemPickupRecord>,
}

impl Default for SimState {
    fn default() -> Self {
        let mut world = Self {
            content: SimContentBuilder::bootstrap().finish(),
            clients: Vec::new(),
            area_entity_world: None,
            entity_collision_capabilities: Vec::new(),
            model_library: Arc::default(),
            old_buttons: Vec::new(),
            old_cmd_angles: Vec::new(),
            player_bodies: PlayerBodyRuntime::default(),
            g_hudelems: Vec::new(),
            hud_elem_sound_ids: crate::hudelem::PulseFxSoundIds::default(),
            dying_missiles: Vec::new(),
            bootstrap: MatchBootstrap::default(),
            running: false,
            phase: MatchPhase::Warmup,
            round_serial: 1,
            match_elapsed_ms: 0,
            prematch: gamemode_iw4::PrematchStep::default(),
            max_alive_seen: 0,
            game_win_winner: None,
            placement_cointoss_unwired: 0,
            root_seed: 0,
            spawn_rng: MatchRng::from_root(0, RngDomain::Spawn),
            combat_rng: MatchRng::from_root(0, RngDomain::Combat),
            bot_rng: MatchRng::from_root(0, RngDomain::Bot),
            next_shot: ShotId(1),
            next_projectile: ProjectileId(1),
            collision_runtime: CollisionRuntime::default(),
            content_digest: 0,
            content_components: crate::ContentComponents::default(),
            events: EventJournal::default(),
            shot_collision_verdicts: Vec::new(),
            projectile_impacts: Vec::new(),
            projectile_impact_log: Vec::new(),
            world_objects: WorldObjectState::default(),
            portal_clearance_signature: None,
            portal_clearance_rows: Vec::new(),
            portal_clearance_validated: Vec::new(),
            portal_clearance_job: None,
            sound_alias_cs: crate::SoundAliasCs::default(),
            effect_name_cs: crate::EffectNameCs::default(),
            hud_material_cs: crate::HudMaterialCs::default(),
            hud_string_cs: crate::HudStringCs::default(),
            num_kills: 0,
            recent_kills: Vec::new(),
            weapon_notes: Vec::new(),
            presentation: PresentationQueue::default(),
            pending_final_kill: None,
            last_pmove_walking: HashMap::new(),
            stuck_holdrand: 0,
            last_stuck_ejects: Vec::new(),
            last_anim_movement: HashMap::new(),
            anim_command_buttons: HashMap::new(),
            anim_event_seed: 1,
            corpses: crate::PlayerCorpsePool::default(),
            entity_kernel: crate::gentity::EntityKernel::default(),
            dobj_anim_mats: HashMap::new(),
            kernel_phases: Vec::new(),
            last_think_order: Vec::new(),
            last_think_dispatch: Vec::new(),
            last_use_presses: Vec::new(),
            objectives: crate::ObjectiveMatch::default(),
            use_start_spawns: gamemode_iw4::USE_START_SPAWNS_AT_START,
            item_pickups: Vec::new(),
        };
        world.recompute_content_digest();
        world
    }
}

impl SimState {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn bootstrap(&mut self, bootstrap: MatchBootstrap) -> Result<(), &'static str> {
        if self.running {
            return Err("MatchBootstrap refused: world already Running");
        }
        if !self.content.weapons().weapon_combat.is_empty() {
            let table_len = self.content.weapons().weapon_combat.len() as u32;
            for class in &bootstrap.classes {
                for id in class.weapon_slot_ids() {
                    if id != 0 && id >= table_len {
                        return Err("MatchBootstrap refused: class references unknown weapon id");
                    }
                }
            }
        }
        self.root_seed = bootstrap.seed;
        self.spawn_rng = MatchRng::from_root(bootstrap.seed, RngDomain::Spawn);
        self.combat_rng = MatchRng::from_root(bootstrap.seed, RngDomain::Combat);
        self.bot_rng = MatchRng::from_root(bootstrap.seed, RngDomain::Bot);
        self.stuck_holdrand = bootstrap.seed as u32;
        self.last_stuck_ejects.clear();
        self.bootstrap = bootstrap;
        self.phase = MatchPhase::Warmup;
        self.match_elapsed_ms = 0;
        self.prematch = gamemode_iw4::PrematchStep::default();
        self.max_alive_seen = 0;
        self.game_win_winner = None;
        self.placement_cointoss_unwired = 0;
        self.weapon_notes.clear();
        self.next_shot = ShotId(1);
        self.collision_runtime.reset();
        self.shot_collision_verdicts.clear();
        self.projectile_impacts.clear();
        self.projectile_impact_log.clear();
        self.g_hudelems.clear();
        self.hud_material_cs = crate::HudMaterialCs::default();
        self.hud_string_cs = crate::HudStringCs::default();
        self.bind_required_hud_materials();
        self.num_kills = 0;
        self.recent_kills.clear();
        self.presentation.reset();
        self.recompute_content_digest();
        Ok(())
    }

    pub fn shutdown_game(&mut self) {
        *self = Self::new();
    }

    pub fn is_running(&self) -> bool {
        self.running
    }

    pub fn phase(&self) -> MatchPhase {
        self.phase
    }

    pub fn match_elapsed_ms(&self) -> u32 {
        self.match_elapsed_ms
    }

    pub fn prematch(&self) -> gamemode_iw4::PrematchStep {
        self.prematch
    }

    pub fn max_alive_seen(&self) -> u32 {
        self.max_alive_seen
    }

    pub fn ffa_team(&self, client: u32) -> Option<u8> {
        self.client_meta(ClientId(client)).and_then(|m| m.ffa_team)
    }

    pub fn client_state_team(&self, id: ClientId) -> Option<i32> {
        self.client_meta(id).map(|meta| meta.client_state_team)
    }

    pub fn gsc_pers_team(&self, client: u32) -> Option<u8> {
        let meta = self.client_meta(ClientId(client))?;
        match meta.client_state_team {
            entity_iw4::TEAM_AXIS => Some(1),
            entity_iw4::TEAM_ALLIES => Some(0),
            entity_iw4::TEAM_SPECTATOR => None,
            _ => meta.ffa_team,
        }
    }

    pub fn game_win_winner(&self) -> Option<ClientId> {
        self.game_win_winner
    }

    pub fn placement_cointoss_unwired(&self) -> u32 {
        self.placement_cointoss_unwired
    }

    pub(crate) fn hud_elem_slots_mut(&mut self) -> &mut Vec<crate::hudelem::GameHudElemSlot> {
        &mut self.g_hudelems
    }

    pub(crate) fn hud_elem_sound_ids(&self) -> crate::hudelem::PulseFxSoundIds {
        self.hud_elem_sound_ids
    }

    pub(crate) fn set_hud_elem_sound_ids(&mut self, ids: crate::hudelem::PulseFxSoundIds) {
        self.hud_elem_sound_ids = ids;
    }

    pub fn collision_complete(&self) -> bool {
        self.content.collision_complete()
    }

    pub fn script_fx_available(&self, name: &str) -> bool {
        self.content
            .script_fx()
            .contains(&name.to_ascii_lowercase())
    }

    pub fn script_sound_exists(&self, name: &str) -> Option<bool> {
        self.content
            .script_sound_aliases()
            .as_ref()
            .map(|names| names.contains_key(&name.to_ascii_lowercase()))
    }

    pub fn script_sound_is_looping(&self, name: &str) -> Result<bool, &'static str> {
        let aliases = self
            .content
            .script_sound_aliases()
            .as_ref()
            .ok_or("sound alias catalog is not installed")?;
        aliases
            .get(&name.to_ascii_lowercase())
            .ok_or("sound alias not found")?
            .ok_or("sound alias looping flags are unavailable")
    }

    pub fn sound_alias_index(&mut self, name: &str) -> u8 {
        self.sound_alias_cs.index(name)
    }

    pub fn effect_name_index(&mut self, name: &str) -> u8 {
        self.effect_name_cs.index(name)
    }

    pub fn hud_material_index(&mut self, name: &str) -> u8 {
        self.hud_material_cs.index(name)
    }

    pub(crate) fn hud_string_index(&mut self, text: &str) -> Option<i32> {
        self.hud_string_cs.index(text)
    }

    pub(crate) fn push_print(&mut self, print: PendingPrint) {
        self.presentation.prints.push(print);
    }

    pub(crate) fn take_pending_prints(&mut self) -> Vec<PendingPrint> {
        core::mem::take(&mut self.presentation.prints)
    }

    pub(crate) fn push_local_sound(&mut self, sound: PendingLocalSound) {
        self.presentation.local_sounds.push(sound);
    }

    pub(crate) fn push_script_audio(&mut self, command: crate::ScriptAudioCommand) {
        self.presentation.script_audio.push(command);
    }

    pub(crate) fn take_pending_script_audio(&mut self) -> Vec<crate::ScriptAudioCommand> {
        core::mem::take(&mut self.presentation.script_audio)
    }

    pub(crate) fn take_pending_local_sounds(&mut self) -> Vec<PendingLocalSound> {
        core::mem::take(&mut self.presentation.local_sounds)
    }

    pub fn bind_required_hud_materials(&mut self) {
        for name in crate::REQUIRED_HUD_MATERIALS {
            let index = self.hud_material_cs.index(name);
            debug_assert_ne!(index, 0, "required HUD material `{name}` has no slot");
        }
    }

    pub fn script_destroy_glass(
        &mut self,
        id: crate::GlassPieceId,
        at_time_ms: i32,
    ) -> crate::GlassPieceState {
        self.world_objects_mut()
            .script_destroy_glass(id, at_time_ms)
    }

    pub fn num_kills(&self) -> u32 {
        self.num_kills
    }

    pub fn push_hud_splash(&mut self, recipient: ClientId, key: String, slot: i32, optional: i32) {
        self.presentation.player_cards.push(PendingPlayerCardEvent {
            recipient,
            kind: PendingPlayerCardKind::Splash {
                key,
                slot,
                optional,
            },
        });
    }

    pub fn push_player_card_slot(&mut self, recipient: ClientId, source: ClientId, slot: i32) {
        self.presentation.player_cards.push(PendingPlayerCardEvent {
            recipient,
            kind: PendingPlayerCardKind::SetSlot { source, slot },
        });
    }

    pub fn push_player_card_open(&mut self, recipient: ClientId, cs_index: i32) {
        self.presentation.player_cards.push(PendingPlayerCardEvent {
            recipient,
            kind: PendingPlayerCardKind::OpenMenu { cs_index },
        });
    }

    pub(crate) fn take_pending_player_cards(&mut self) -> Vec<PendingPlayerCardEvent> {
        core::mem::take(&mut self.presentation.player_cards)
    }

    pub(crate) fn take_pending_final_kill(&mut self) -> Option<(ClientId, ClientId)> {
        self.pending_final_kill.take()
    }

    pub fn pending_final_kill(&self) -> Option<(ClientId, ClientId)> {
        self.pending_final_kill
    }

    pub fn clients_scoreboard(&self) -> Vec<(ClientId, ClientMatchState)> {
        self.clients.clone()
    }

    pub(crate) fn restart_level_phase(&mut self) {
        self.round_serial = self.round_serial.wrapping_add(1);
        self.phase = MatchPhase::Warmup;
        self.match_elapsed_ms = 0;
        self.prematch = gamemode_iw4::PrematchStep::default();
        self.max_alive_seen = 0;
        self.game_win_winner = None;
        self.pending_final_kill = None;
    }

    pub(crate) fn set_phase(&mut self, phase: MatchPhase) {
        if phase == MatchPhase::Playing && self.phase == MatchPhase::Warmup {
            self.prematch = gamemode_iw4::PrematchStep::Done;
        }
        self.phase = phase;
    }

    pub fn root_seed(&self) -> u64 {
        self.root_seed
    }

    pub fn rng(&self) -> &MatchRng {
        &self.spawn_rng
    }

    pub fn spawn_rng(&self) -> &MatchRng {
        &self.spawn_rng
    }

    pub fn combat_rng(&self) -> &MatchRng {
        &self.combat_rng
    }

    pub fn bot_rng(&self) -> &MatchRng {
        &self.bot_rng
    }

    pub(crate) fn combat_rng_mut(&mut self) -> &mut MatchRng {
        &mut self.combat_rng
    }

    pub(crate) fn alloc_shot_id(&mut self) -> ShotId {
        let id = self.next_shot;
        self.next_shot = id.next();
        id
    }

    pub(crate) fn mark_running(&mut self) {
        self.running = true;
    }

    pub(crate) fn bootstrap_ref(&self) -> &MatchBootstrap {
        &self.bootstrap
    }

    pub fn game_mode_kind(&self) -> gamemode_iw4::GameModeKind {
        self.bootstrap.kind
    }

    pub fn authored_spawn_origins(&self) -> Vec<[f32; 3]> {
        self.bootstrap
            .spawns
            .iter()
            .map(|spawn| spawn.origin)
            .collect()
    }

    pub fn cheats_enabled(&self) -> bool {
        self.bootstrap.allow_debug_actions
    }

    pub fn player_anim_script(&self) -> Option<Arc<PlayerAnimScript>> {
        self.content.player_anim_script().clone()
    }

    pub(crate) fn set_anim_movement(&mut self, id: ClientId, movetype: u8, strafing: u8) {
        self.last_anim_movement.insert(id, (movetype, strafing));
    }

    pub(crate) fn set_anim_command_buttons(&mut self, id: ClientId, buttons: u32) {
        self.anim_command_buttons.insert(id, buttons);
    }

    pub(crate) fn anim_command_buttons(&self, id: ClientId) -> u32 {
        self.anim_command_buttons.get(&id).copied().unwrap_or(0)
    }

    pub fn last_anim_movetype(&self, id: ClientId) -> Option<u8> {
        self.last_anim_movement
            .get(&id)
            .map(|&(movetype, _)| movetype)
    }

    pub(crate) fn last_anim_strafing(&self, id: ClientId) -> u8 {
        self.last_anim_movement
            .get(&id)
            .map_or(0, |&(_, strafing)| strafing)
    }

    pub fn anim_event_seed(&self) -> u32 {
        self.anim_event_seed
    }

    pub fn set_anim_event_seed(&mut self, seed: u32) {
        self.anim_event_seed = seed;
    }

    pub fn pen_table_loaded(&self) -> bool {
        self.content.weapons().pen_table_loaded
    }

    fn collision_kit_index(&self, id: ClientId) -> usize {
        let Some(meta) = self.client_meta(id) else {
            return 0;
        };
        usize::from(Self::kit_assignment_is_axis(
            meta.client_state_team,
            meta.ffa_team,
        ))
    }

    fn collision_kit(&self, id: ClientId) -> &PlayerKitCollision {
        &self.content.player_kits()[self.collision_kit_index(id)]
    }

    fn kit_assignment_is_axis(client_state_team: i32, ffa_team: Option<u8>) -> bool {
        match client_state_team {
            entity_iw4::TEAM_AXIS => true,
            entity_iw4::TEAM_ALLIES => false,
            _ => ffa_team == Some(1),
        }
    }

    pub fn xanims(&self) -> Arc<crate::MantleXAnimBind> {
        Arc::clone(self.content.xanims())
    }

    pub fn player_body_pose_kind(&self) -> &'static str {
        if self.content.player_kits().iter().all(|k| k.body.is_none()) {
            "none"
        } else if self.content.player_anim_tree().is_some() {
            "anim"
        } else {
            "bind"
        }
    }

    pub fn penetration_table(&self) -> &weapon_iw4::PenetrationDepthTable {
        &self.content.weapons().pen_table
    }

    pub(crate) fn bullet_pen_facts_for(&self, weapon: u32) -> weapon_iw4::BulletPenFacts {
        self.content
            .weapons()
            .bullet_pen
            .get(weapon as usize)
            .copied()
            .unwrap_or_else(weapon_iw4::BulletPenFacts::default)
    }

    pub fn weapon_script_names(&self) -> Arc<[String]> {
        Arc::clone(&self.content.weapons().weapon_script_names)
    }

    pub(crate) fn rank_for_xp(&self, xp: i32) -> Option<u32> {
        self.content.weapons().rank_progression.rank(xp)
    }

    pub(crate) fn weapon_unlock_requirement(
        &self,
        weapon: u32,
    ) -> Option<&Result<gamemode_iw4::progression::UnlockRequirement, String>> {
        self.content
            .weapons()
            .unlock_requirements
            .get(weapon as usize)
    }

    pub fn weapon_setup(&self, weapon: u32) -> Option<&WeaponSetup> {
        self.content
            .weapons()
            .weapon_setups
            .get(weapon as usize)?
            .as_ref()
    }

    pub fn weapon_script_name(&self, weapon: u32) -> &str {
        self.content
            .weapons()
            .weapon_script_names
            .get(weapon as usize)
            .map(String::as_str)
            .unwrap_or("")
    }

    pub(crate) fn shock(&self, name: &str) -> Option<&hud_iw4::ShockParams> {
        self.content.shocks().get(&name.to_ascii_lowercase())
    }

    pub(crate) fn map_custom(&self, key: &str) -> &str {
        self.content
            .map_custom()
            .get(&key.to_ascii_lowercase())
            .map_or("", String::as_str)
    }

    pub(crate) fn weapon_projectile_model(&self, weapon: u32) -> &str {
        self.content
            .weapons()
            .weapon_projectile_models
            .get(weapon as usize)
            .map_or("", String::as_str)
    }

    pub fn weapon_script_sounds(&self, weapon: u32) -> Option<&WeaponScriptSounds> {
        self.content
            .weapons()
            .weapon_script_sounds
            .get(weapon as usize)
    }

    pub(crate) fn weapon_is_melee_only(&self, weapon: u32) -> bool {
        self.content
            .weapons()
            .weapon_melee_only
            .get(weapon as usize)
            .copied()
            .unwrap_or(false)
    }

    pub(crate) fn shield_weapon_for_model(
        &self,
        model: &str,
        weapons: impl IntoIterator<Item = i32>,
    ) -> Option<u32> {
        weapons
            .into_iter()
            .filter_map(|weapon| u32::try_from(weapon).ok())
            .find(|weapon| {
                self.content
                    .weapons()
                    .weapon_world_models
                    .get(*weapon as usize)
                    .is_some_and(|(name, _)| name == model)
                    && self
                        .content
                        .weapons()
                        .shield_models
                        .get(*weapon as usize)
                        .is_some_and(Option::is_some)
            })
    }

    pub(crate) fn weapon_world_model(&self, weapon: u32) -> Option<(&str, &[String])> {
        self.content
            .weapons()
            .weapon_world_models
            .get(weapon as usize)
            .map(|(model, tags)| (model.as_str(), tags.as_slice()))
    }

    pub fn vehicle_compass(&self, name: &str) -> Option<&([String; 2], [i32; 2])> {
        self.content.vehicle_compass().get(name)
    }

    pub fn vehicle_accel(&self, name: &str) -> Option<f32> {
        self.content.vehicle_accel().get(name).copied()
    }

    pub fn vehicle_turret_weapon(&self, vehicle: &str) -> Option<u32> {
        let path = self.content.vehicle_turrets().get(vehicle)?;
        self.weapon_index_by_script_name(path.rsplit('/').next().unwrap_or(path))
    }

    pub fn weapon_index_by_script_name(&self, name: &str) -> Option<u32> {
        if let Some(&weapon) = self.content.weapons().weapon_script_aliases.get(name) {
            return Some(weapon);
        }
        self.content
            .weapons()
            .weapon_script_names
            .iter()
            .position(|n| n == name)
            .and_then(|i| u32::try_from(i).ok())
            .filter(|&i| i != 0)
    }

    pub fn equipment_facts_for(&self, weapon: u32) -> Option<EquipmentRuntimeFacts> {
        self.content
            .weapons()
            .equipment_runtime
            .get(weapon as usize)
            .copied()
            .filter(|facts| facts.is_usable())
    }

    pub(crate) fn offhand_loadout_row(&self, weapon: u32) -> Option<EquipmentRuntimeFacts> {
        self.content
            .weapons()
            .equipment_runtime
            .get(weapon as usize)
            .copied()
            .filter(|facts| facts.is_offhand())
    }

    pub(crate) fn missile_launch_facts(&self, weapon: u32) -> Option<EquipmentRuntimeFacts> {
        self.content.missile_launch_facts(weapon)
    }

    pub(crate) fn allocate_projectile_id(&mut self) -> ProjectileId {
        let id = self.next_projectile;
        self.next_projectile = ProjectileId(self.next_projectile.0.wrapping_add(1).max(1));
        id
    }

    pub fn anim_mat(&self, entnum: i32, bone: i32) -> Option<entity_iw4::DObjAnimMat> {
        self.dobj_anim_mats.get(&(entnum, bone)).copied()
    }

    pub(crate) fn allocate_dynamic_entity(
        &mut self,
        kind: crate::gentity::EntityRunKind,
    ) -> Result<crate::gentity::EntityRef, crate::gentity::EntityAllocError> {
        let entity = self.entity_kernel.allocate(kind)?;
        self.entity_kernel
            .set_linked(entity, true)
            .expect("newly allocated dynamic entity must resolve");
        Ok(entity)
    }

    pub(crate) fn free_dynamic_entity_number(&mut self, number: i32) {
        let entity = self
            .entity_kernel
            .current_ref(number)
            .expect("dynamic store referenced a free or invalid entity slot");
        self.entity_kernel
            .free(entity)
            .expect("dynamic entity generation changed while its typed store was alive");
    }

    pub(crate) fn retained_missile_events(&self) -> &[entity_iw4::EntityState] {
        &self.dying_missiles
    }

    pub(crate) fn expire_dying_missiles(&mut self, tick: Tick) {
        debug_assert_eq!(
            self.entity_kernel.level_time_ms(),
            crate::corpse::level_time_ms(tick)
        );
        let expired: Vec<i32> = self
            .entity_kernel
            .expire_transient_events()
            .into_iter()
            .map(crate::gentity::EntityRef::number)
            .collect();
        self.dying_missiles
            .retain(|state| !expired.contains(&state.number));
    }

    pub(crate) fn note_dying_missile(&mut self, tick: Tick, projectile: ProjectileState) {
        if projectile.entnum != playerstate_iw4::ENTITYNUM_NONE {
            let entity = self
                .entity_kernel
                .current_ref(projectile.entnum)
                .expect("terminal projectile lost its dynamic entity slot");
            self.entity_kernel
                .mark_transient_event(entity, crate::corpse::level_time_ms(tick))
                .expect("terminal projectile generation changed before event retention");
            // The event lingers for the killcam, but the projectile's flight
            // has ended. Keeping TR_GRAVITY here drives the replay camera
            // through the ground after the explosion.
            let time = crate::corpse::level_time_ms(tick);
            let position = entity_iw4::Trajectory {
                tr_type: entity_iw4::TR_STATIONARY,
                tr_time: time,
                tr_base: projectile.origin,
                ..Default::default()
            };
            let angles = entity_iw4::Trajectory {
                tr_type: entity_iw4::TR_STATIONARY,
                tr_time: time,
                tr_base: entity_iw4::evaluate_trajectory(&projectile.apos, time),
                ..Default::default()
            };
            self.dying_missiles.push(crate::gentity::init_missile_state(
                projectile.entnum,
                projectile.weapon,
                position,
                angles,
                projectile.launch_time,
            ));
        }
    }

    pub fn weapon_combat_len(&self) -> usize {
        self.content.weapons().weapon_combat.len()
    }

    pub fn weapon_combat_row(&self, weapon: u32) -> Option<WeaponCombatFacts> {
        self.content
            .weapons()
            .weapon_combat
            .get(weapon as usize)
            .copied()
    }

    pub fn content_digest(&self) -> u64 {
        self.content_digest
    }

    pub fn content_components(&self) -> crate::ContentComponents {
        self.content_components
    }

    fn recompute_content_digest(&mut self) {
        self.content_digest = crate::content::content_digest(
            self.content.weapons(),
            &self.bootstrap,
            self.content.clip_brushes(),
            &self.entity_collision_capabilities,
            self.content.map(),
        );
        self.content_components = crate::content::content_components(
            self.content.weapons(),
            &self.bootstrap,
            self.content.clip_brushes(),
            &self.entity_collision_capabilities,
            self.content.map(),
        );
    }

    /// Read-only weapon timing and magazine facts, as the combat step reads
    /// them. A narrow view for callers that must not guess these numbers.
    pub fn weapon_combat_facts(&self, weapon: u32) -> Option<WeaponCombatFacts> {
        self.combat_facts_for(weapon)
    }

    pub(crate) fn combat_facts_for(&self, weapon: u32) -> Option<WeaponCombatFacts> {
        self.content
            .weapons()
            .weapon_combat
            .get(weapon as usize)
            .copied()
            .filter(|f| f.is_usable())
    }

    pub(crate) fn can_transition_weapon(&self, from: u32, to: u32) -> bool {
        if from == 0 || from == to {
            return false;
        }
        let groups = &self.content.weapons().weapon_transition_groups;
        let Some(&group) = groups.get(from as usize) else {
            return false;
        };
        group != 0 && groups.get(to as usize) == Some(&group)
    }

    pub(crate) fn weapon_camouflage_allowed(&self, weapon: u32, model: u8) -> bool {
        self.content
            .weapons()
            .weapon_camouflage_slots
            .get(weapon as usize)
            .is_some_and(|slots| slots.contains(&model))
    }

    pub(crate) fn weapon_runnable(&self, id: u32) -> bool {
        self.content.weapons().is_runnable(id)
    }

    fn reset_area_entity_world(&mut self) {
        let Some(world_model) = self.content.clip_cmodels().models.first() else {
            self.area_entity_world = None;
            return;
        };
        let world_bounds =
            clipmap_iw4::AreaBounds::from_mins_maxs(world_model.mins, world_model.maxs)
                .unwrap_or_else(|_| {
                    panic!("area query: world cmodel Bounds are invalid");
                });
        self.area_entity_world = Some(clipmap_iw4::AreaEntityWorld::new(world_bounds));
    }

    pub(crate) fn link_player_area(
        &mut self,
        id: ClientId,
        origin: [f32; 3],
        mins: [f32; 3],
        maxs: [f32; 3],
    ) -> bool {
        let Some(area_world) = self.area_entity_world.as_mut() else {
            return false;
        };
        let entity_num = u16::try_from(id.0).unwrap_or_else(|_| {
            panic!("player entity index does not fit the 1024-entry collision world");
        });
        let absmin = [
            origin[0] + mins[0] - 1.0,
            origin[1] + mins[1] - 1.0,
            origin[2] + mins[2] - 1.0,
        ];
        let absmax = [
            origin[0] + maxs[0] + 1.0,
            origin[1] + maxs[1] + 1.0,
            origin[2] + maxs[2] + 1.0,
        ];
        let bounds = clipmap_iw4::AreaBounds::from_mins_maxs(absmin, absmax).unwrap_or_else(|_| {
            panic!("player link Bounds are invalid");
        });
        area_world
            .link(
                entity_num,
                CONTENTS_BODY,
                u32::MAX,
                bounds,
                [absmin[0], absmin[1]],
                [absmax[0], absmax[1]],
            )
            .unwrap_or_else(|_| {
                panic!("collision link rejected player link state");
            });
        true
    }

    pub(crate) fn translate_player_area(&mut self, id: ClientId, delta: [f32; 3]) -> bool {
        let Some(area_world) = self.area_entity_world.as_mut() else {
            return false;
        };
        let Ok(entity_num) = u16::try_from(id.0) else {
            panic!("player entity index does not fit the 1024-entry collision world");
        };
        area_world.translate(entity_num, delta).unwrap_or_else(|_| {
            panic!("collision link rejected translated player Bounds");
        })
    }

    pub(crate) fn unlink_player_area(&mut self, id: ClientId) -> bool {
        let Some(area_world) = self.area_entity_world.as_mut() else {
            return false;
        };
        let Ok(entity_num) = u16::try_from(id.0) else {
            panic!("player entity index does not fit the 1024-entry collision world");
        };
        area_world.unlink(entity_num).unwrap_or_else(|_| {
            panic!("collision unlink rejected player entity index");
        })
    }

    pub(crate) fn forget_client_membership(&mut self, id: ClientId) {
        self.unlink_player_area(id);
        self.clients.retain(|(c, _)| *c != id);
        self.player_bodies.forget_client(id);
        self.collision_runtime.forget_client(id);
        self.last_pmove_walking.remove(&id);
        self.last_anim_movement.remove(&id);
        self.anim_command_buttons.remove(&id);
    }

    pub(crate) fn area_entity_candidates(
        &self,
        bounds: clipmap_iw4::AreaBounds,
        mask: u32,
        capacity: usize,
    ) -> Vec<u16> {
        self.area_entity_world
            .as_ref()
            .unwrap_or_else(|| {
                panic!("area query needs an initialized world cmodel Bounds root");
            })
            .query(bounds, mask, capacity)
    }

    pub(crate) fn player_area_bounds(&self, id: ClientId) -> Option<clipmap_iw4::AreaBounds> {
        let entity_num = u16::try_from(id.0).ok()?;
        self.area_entity_world.as_ref()?.entity_bounds(entity_num)
    }

    pub fn clip_brushes(&self) -> &[SimBrush] {
        self.content.clip_brushes()
    }

    pub fn clip_bsp(&self) -> &SimClipBsp {
        self.content.clip_bsp()
    }

    pub fn clip_mesh(&self) -> &SimClipMesh {
        self.content.clip_mesh()
    }

    pub fn clip_cmodels(&self) -> &SimClipCmodels {
        self.content.clip_cmodels()
    }

    pub fn entity_kernel(&self) -> &crate::gentity::EntityKernel {
        &self.entity_kernel
    }

    pub(crate) fn entity_kernel_mut(&mut self) -> &mut crate::gentity::EntityKernel {
        &mut self.entity_kernel
    }

    pub fn last_think_order(&self) -> &[crate::gentity::EntityRef] {
        &self.last_think_order
    }

    pub fn last_think_dispatch(&self) -> &[(i32, crate::gentity::EntityRunKind)] {
        &self.last_think_dispatch
    }

    pub fn last_use_presses(&self) -> &[crate::gentity::UsePress] {
        &self.last_use_presses
    }

    pub(crate) fn set_use_start_spawns(&mut self, value: bool) {
        self.use_start_spawns = value;
    }

    pub fn use_start_spawns(&self) -> bool {
        self.use_start_spawns
    }

    pub(crate) fn stamp_think_order_rows(&mut self, rows: Vec<crate::gentity::EntityRef>) {
        self.last_think_order = rows;
    }

    pub(crate) fn stamp_think_dispatch(&mut self, rows: Vec<(i32, crate::gentity::EntityRunKind)>) {
        self.last_think_dispatch = rows;
    }

    pub(crate) fn stamp_use_presses(&mut self, presses: Vec<crate::gentity::UsePress>) {
        self.last_use_presses = presses;
    }

    pub(crate) fn begin_entity_frame(&mut self, tick: Tick) {
        self.entity_kernel
            .begin_frame(crate::corpse::level_time_ms(tick));
        self.kernel_phases.clear();
        self.enter_kernel_phase(crate::gentity::KernelPhase::AdvanceTime);
    }

    pub(crate) fn enter_kernel_phase(&mut self, phase: crate::gentity::KernelPhase) {
        let expected = crate::gentity::KERNEL_PHASE_ORDER
            .get(self.kernel_phases.len())
            .copied();
        assert_eq!(expected, Some(phase), "sim kernel phase order diverged");
        self.kernel_phases.push(phase);
    }

    pub fn install_entity_collision_capabilities(
        &mut self,
        mut proxies: Vec<EntityCollisionCapabilities>,
    ) {
        proxies.sort_by_key(|capabilities| capabilities.owner);
        self.entity_collision_capabilities = proxies;
        self.recompute_content_digest();
    }

    pub fn entity_collision_capabilities(&self) -> &[EntityCollisionCapabilities] {
        &self.entity_collision_capabilities
    }

    pub(crate) fn model_movement_brushes(&self) -> Vec<SimBrush> {
        self.model_movement_brushes_where(|_| true)
    }

    pub(crate) fn model_movement_brushes_where(
        &self,
        keep: impl Fn(&EntityCollisionCapabilities) -> bool,
    ) -> Vec<SimBrush> {
        crate::collision_scene::model_movement_brushes(&self.entity_collision_capabilities, keep)
    }

    pub fn entity_collision_capabilities_mut(&mut self) -> &mut [EntityCollisionCapabilities] {
        &mut self.entity_collision_capabilities
    }

    pub(crate) fn collision_owner_mut(
        &mut self,
        id: ScriptModelId,
    ) -> Option<&mut EntityCollisionCapabilities> {
        let owner = crate::AuthorityModelOwner::ScriptModel(id);
        let index = self
            .entity_collision_capabilities
            .binary_search_by_key(&owner, |row| row.owner)
            .ok()?;
        Some(&mut self.entity_collision_capabilities[index])
    }

    pub(crate) fn insert_collision_owner(&mut self, row: EntityCollisionCapabilities) {
        match self
            .entity_collision_capabilities
            .binary_search_by_key(&row.owner, |have| have.owner)
        {
            Ok(index) => self.entity_collision_capabilities[index] = row,
            Err(index) => self.entity_collision_capabilities.insert(index, row),
        }
    }

    pub(crate) fn remove_collision_owner(&mut self, id: ScriptModelId) {
        let owner = crate::AuthorityModelOwner::ScriptModel(id);
        self.entity_collision_capabilities
            .retain(|row| row.owner != owner);
    }

    pub fn install_model_library(
        &mut self,
        models: std::collections::BTreeMap<
            String,
            Option<Arc<xmodel_runtime::RetainedModelCapability>>,
        >,
    ) {
        self.model_library = Arc::new(models);
    }

    pub fn model_capability(
        &self,
        name: &str,
    ) -> Option<Option<Arc<xmodel_runtime::RetainedModelCapability>>> {
        self.model_library.get(name).cloned()
    }

    pub fn trace_world(
        &self,
        start: [f32; 3],
        end: [f32; 3],
        mins: [f32; 3],
        maxs: [f32; 3],
        mask: u32,
    ) -> trace_iw4::Trace {
        self.trace_clip(start, end, mins, maxs, mask)
    }

    pub(crate) fn trace_world_except(
        &self,
        start: [f32; 3],
        end: [f32; 3],
        mask: u32,
        exclude: Option<crate::AuthorityModelOwner>,
    ) -> trace_iw4::Trace {
        self.trace_world_hull_except(
            movement_iw4::GroundTraceInput {
                start,
                end,
                mins: [0.0; 3],
                maxs: [0.0; 3],
                tracemask: mask,
            },
            exclude,
        )
    }

    pub(crate) fn trace_world_hull_except(
        &self,
        input: movement_iw4::GroundTraceInput,
        exclude: Option<crate::AuthorityModelOwner>,
    ) -> trace_iw4::Trace {
        self.trace_scene(input, false, exclude)
    }

    pub fn trace_static_world(
        &self,
        start: [f32; 3],
        end: [f32; 3],
        mins: [f32; 3],
        maxs: [f32; 3],
        mask: u32,
    ) -> trace_iw4::Trace {
        clip_trace(
            self.content.clip_brushes(),
            self.content.clip_bsp(),
            self.content.clip_mesh(),
            &clipmap_iw4::TraceExtents {
                start,
                end,
                mins,
                maxs,
                mask,
            },
            &|piece| self.world_objects.glass_is_solid(piece as u32),
        )
    }

    pub fn has_world_clip(&self) -> bool {
        !self.content.clip_brushes().is_empty() || self.content.clip_mesh().tables.tri_count() >= 1
    }

    pub(crate) fn trace_clip(
        &self,
        start: [f32; 3],
        end: [f32; 3],
        mins: [f32; 3],
        maxs: [f32; 3],
        mask: u32,
    ) -> trace_iw4::Trace {
        self.trace_scene(
            movement_iw4::GroundTraceInput {
                start,
                end,
                mins,
                maxs,
                tracemask: mask,
            },
            false,
            None,
        )
    }

    /// Potential navigation clearance after destructible panes have been removed.
    /// This does not mutate glass state or omit linked world obstacles.
    pub fn trace_navigation(
        &self,
        start: [f32; 3],
        end: [f32; 3],
        mins: [f32; 3],
        maxs: [f32; 3],
        mask: u32,
    ) -> trace_iw4::Trace {
        self.trace_scene(
            movement_iw4::GroundTraceInput {
                start,
                end,
                mins,
                maxs,
                tracemask: mask,
            },
            true,
            None,
        )
    }

    fn trace_scene(
        &self,
        input: movement_iw4::GroundTraceInput,
        ignore_glass: bool,
        exclude: Option<crate::AuthorityModelOwner>,
    ) -> trace_iw4::Trace {
        crate::collision_scene::trace_scene(
            &self.content,
            &self.world_objects,
            &self.entity_collision_capabilities,
            input,
            ignore_glass,
            exclude,
        )
    }

    pub fn penetrations(
        &self,
        input: movement_iw4::GroundTraceInput,
        contacts: &mut movement_iw4::recovery::ContactBuffer,
    ) -> movement_iw4::recovery::Coverage {
        let linked: Vec<_> = self
            .entity_collision_capabilities
            .iter()
            .flat_map(|row| row.solid_brushes().iter().cloned())
            .collect();
        let models = self.model_movement_brushes_where(|_| true);
        crate::penetration::contacts(
            &crate::penetration::RecoveryScene {
                brushes: self.content.clip_brushes(),
                bsp: self.content.clip_bsp(),
                mesh: self.content.clip_mesh(),
                cmodels: &self.content.clip_cmodels().models,
                linked: &linked,
                models: &models,
                glass_is_solid: &|piece| self.world_objects.glass_is_solid(u32::from(piece)),
            },
            input,
            contacts,
        )
    }

    pub fn collision_history(&self) -> &CollisionHistory {
        &self.collision_runtime.players
    }

    pub fn entity_collision_history(&self) -> &EntityCollisionHistory {
        &self.collision_runtime.entities
    }

    fn lagcomp_plan(&self, attacker: ClientId, current: Tick) -> LagcompPlan {
        self.collision_runtime.plan(attacker, current)
    }
    pub(crate) fn set_lagcomp_commands(
        &mut self,
        rows: impl IntoIterator<Item = ((ClientId, i32), crate::ShotSampleProvenance)>,
    ) {
        self.collision_runtime.set_commands(rows);
    }
    pub(crate) fn select_lagcomp_command(&mut self, client: ClientId, command_time: i32) {
        self.collision_runtime.select_command(client, command_time);
    }
    pub fn set_lagcomp_sample(
        &mut self,
        client: ClientId,
        sample: crate::bullet_collision::ShotSampleProvenance,
    ) {
        self.collision_runtime.set_sample(client, sample);
    }
    pub fn lagcomp_sample(
        &self,
        client: ClientId,
    ) -> Option<crate::bullet_collision::ShotSampleProvenance> {
        self.collision_runtime.sample(client)
    }

    pub fn lagcomp_query_for(
        &self,
        attacker: ClientId,
        current: Tick,
        player: impl Fn(ClientId) -> Option<PlayerState>,
    ) -> crate::bullet_collision::LagcompQuery {
        let plan = self.lagcomp_plan(attacker, current);
        crate::bullet_collision::LagcompQuery {
            players: self.lagcomp_players(current, plan, player),
            entities: self.lagcomp_entities(current, plan),
        }
    }

    fn lagcomp_players(
        &self,
        current: Tick,
        plan: LagcompPlan,
        player: impl Fn(ClientId) -> Option<PlayerState>,
    ) -> crate::bullet_collision::HistorySample {
        use crate::bullet_collision::{HistoryClampReason, HistorySample, HistorySampleVerdict};

        match plan {
            LagcompPlan::CurrentAuthority { reason } => HistorySample {
                poses: self.alive_collision_poses(player),
                verdict: HistorySampleVerdict::CurrentAuthority {
                    tick: current,
                    reason,
                },
            },
            LagcompPlan::Refused { requested, reason } => HistorySample {
                poses: Vec::new(),
                verdict: HistorySampleVerdict::Refused { requested, reason },
            },
            LagcompPlan::Rewind {
                requested,
                used,
                clamped,
            } => {
                let Some(frame) = self.collision_runtime.players.frame_at(used) else {
                    return HistorySample {
                        poses: Vec::new(),
                        verdict: HistorySampleVerdict::Refused {
                            requested: used,
                            reason: "missing history frame",
                        },
                    };
                };
                let verdict = if clamped {
                    HistorySampleVerdict::Clamped {
                        requested,
                        used,
                        frame: frame.tick,
                        phase: frame.phase,
                        reason: HistoryClampReason::MaxRewindWindow,
                    }
                } else {
                    HistorySampleVerdict::Exact {
                        requested,
                        frame: frame.tick,
                        phase: frame.phase,
                    }
                };
                HistorySample {
                    poses: self.poses_of_current_life(&frame.poses),
                    verdict,
                }
            }
        }
    }

    fn poses_of_current_life(
        &self,
        poses: &[crate::bullet_collision::PlayerCollisionPose],
    ) -> Vec<crate::bullet_collision::PlayerCollisionPose> {
        poses
            .iter()
            .filter(|pose| {
                self.client_meta(pose.client)
                    .is_none_or(|meta| meta.life_sequence == pose.life_sequence)
            })
            .cloned()
            .collect()
    }

    fn lagcomp_entities(
        &self,
        current: Tick,
        plan: LagcompPlan,
    ) -> crate::bullet_collision::EntityCollisionSample {
        use crate::bullet_collision::{
            EntityCollisionEpoch, EntityCollisionSample, HistoryClampReason, HistorySampleVerdict,
        };

        match plan {
            LagcompPlan::CurrentAuthority { reason } => EntityCollisionSample {
                rows: self
                    .entity_collision_capabilities
                    .iter()
                    .map(EntityCollisionCapabilities::trace_geom)
                    .collect(),
                verdict: HistorySampleVerdict::CurrentAuthority {
                    tick: current,
                    reason,
                },
            },
            LagcompPlan::Refused { requested, reason } => EntityCollisionSample {
                rows: Vec::new(),
                verdict: HistorySampleVerdict::Refused { requested, reason },
            },
            LagcompPlan::Rewind {
                requested,
                used,
                clamped,
            } => {
                let Some(frame) = self.collision_runtime.entities.frame_at(used) else {
                    return EntityCollisionSample {
                        rows: Vec::new(),
                        verdict: HistorySampleVerdict::Refused {
                            requested: used,
                            reason: "missing history frame",
                        },
                    };
                };
                let verdict = if clamped {
                    HistorySampleVerdict::Clamped {
                        requested,
                        used,
                        frame: frame.tick,
                        phase: frame.phase,
                        reason: HistoryClampReason::MaxRewindWindow,
                    }
                } else {
                    HistorySampleVerdict::Exact {
                        requested,
                        frame: frame.tick,
                        phase: frame.phase,
                    }
                };
                let rows = frame
                    .rows
                    .iter()
                    .cloned()
                    .map(|mut row| {
                        row.epoch = EntityCollisionEpoch::Historical { frame: frame.tick };
                        row
                    })
                    .collect();
                EntityCollisionSample { rows, verdict }
            }
        }
    }

    pub fn world_objects(&self) -> &WorldObjectState {
        &self.world_objects
    }

    pub fn world_objects_mut(&mut self) -> &mut WorldObjectState {
        &mut self.world_objects
    }

    pub(crate) fn alive_collision_poses(
        &self,
        player: impl Fn(ClientId) -> Option<PlayerState>,
    ) -> Vec<crate::bullet_collision::PlayerCollisionPose> {
        self.alive_collision_poses_inner(player, &[], |_| {})
    }

    pub(crate) fn player_poses_since_record(
        &self,
        player: impl Fn(ClientId) -> Option<PlayerState>,
    ) -> Option<Vec<crate::bullet_collision::PlayerCollisionPose>> {
        use crate::match_state::ClientLifecycle;

        let recorded = self
            .collision_runtime
            .players
            .latest_poses()
            .map_or(&[][..], |(_, poses)| poses);
        let mut alive = 0;
        for (id, meta) in &self.clients {
            if meta.lifecycle != ClientLifecycle::Alive {
                continue;
            }
            let Some(ps) = player(*id) else {
                continue;
            };
            alive += 1;
            let held = recorded.iter().any(|pose| {
                pose.client == *id
                    && pose.life_sequence == meta.life_sequence
                    && pose.origin == ps.origin
                    && pose.viewangles == ps.viewangles
                    && pose.shield == meta.shield
            });
            if !held {
                return Some(self.alive_collision_poses_inner(player, recorded, |_| {}));
            }
        }
        (alive != recorded.len())
            .then(|| self.alive_collision_poses_inner(player, recorded, |_| {}))
    }

    fn alive_collision_poses_inner(
        &self,
        player: impl Fn(ClientId) -> Option<PlayerState>,
        reuse: &[crate::bullet_collision::PlayerCollisionPose],
        mut on_err: impl FnMut(&str),
    ) -> Vec<crate::bullet_collision::PlayerCollisionPose> {
        use crate::bullet_collision::{
            HitVolumeKind, PLAYER_MAXS, PLAYER_MINS, PlayerCollisionPose,
        };
        use crate::match_state::ClientLifecycle;

        let mut poses = Vec::new();
        for (id, meta) in &self.clients {
            if meta.lifecycle != ClientLifecycle::Alive {
                continue;
            }
            let Some(ps) = player(*id) else {
                continue;
            };
            if let Some(pose) = reuse.iter().find(|pose| {
                pose.client == *id
                    && pose.life_sequence == meta.life_sequence
                    && pose.origin == ps.origin
                    && pose.viewangles == ps.viewangles
                    && pose.shield == meta.shield
            }) {
                poses.push(pose.clone());
                continue;
            }
            let (bones, shield_normal) = match self.player_hitvol_bones(*id, &ps) {
                Ok(bones) => bones,
                Err(error) => {
                    on_err(&error);
                    (Vec::new(), None)
                }
            };
            poses.push(PlayerCollisionPose {
                client: *id,
                origin: ps.origin,
                mins: PLAYER_MINS,
                maxs: PLAYER_MAXS,
                life_sequence: meta.life_sequence,
                hit_volume: HitVolumeKind::Standing,
                bones,
                viewangles: ps.viewangles,
                shield: meta.shield,
                shield_normal,
            });
        }
        poses
    }

    fn player_dobj_request(
        &self,
        id: ClientId,
        ps: &PlayerState,
    ) -> (
        xmodel_runtime::DObjPoseRequest,
        &'static str,
        u16,
        f32,
        Option<i64>,
        Option<f32>,
        Option<f32>,
    ) {
        let legs = PlayerAnimValue::from_raw((ps.legs_anim as u16) & PLAYER_ANIM_RAW_MASK)
            .map(PlayerAnimValue::effective_index)
            .unwrap_or(0);
        let bind = || {
            (
                xmodel_runtime::DObjPoseRequest::bind_pose(),
                "bind",
                legs,
                0.0,
                None,
                None,
                None,
            )
        };
        if self.content.player_anim_tree().is_none() || legs == 0 {
            return bind();
        }
        let Some(slot) = self.player_bodies.player_anim_trees.get(&id.0) else {
            return bind();
        };
        if slot.leaf != legs {
            return bind();
        }
        let state = slot.runtime.states().get(legs as usize);
        let node_time = state.map(|s| s.time);
        let goal_weight = state.map(|s| s.goal_weight);
        let sample = match slot.runtime.definition().nodes().get(legs as usize) {
            Some(node) => match &node.kind {
                xmodel_runtime::XAnimNodeKind::Leaf { clip, .. } => {
                    clip.duration() * node_time.unwrap_or(0.0)
                }
                _ => 0.0,
            },
            None => 0.0,
        };
        (
            xmodel_runtime::DObjPoseRequest::with_tree(slot.runtime.clone()),
            "anim",
            legs,
            sample,
            Some(slot.persist),
            node_time,
            goal_weight,
        )
    }

    pub(crate) fn player_attachment_bones(
        &self,
        id: ClientId,
        life: crate::LifeSequence,
        ps: Option<&PlayerState>,
    ) -> Option<Vec<xmodel_runtime::CollisionBone>> {
        let meta = self.client_meta(id)?;
        // Never carry an attachment into a respawn or a reused corpse slot.
        if meta.life_sequence != life {
            return None;
        }
        if meta.lifecycle == crate::ClientLifecycle::Alive {
            return self.player_hitvol_bones(id, ps?).ok().map(|p| p.0);
        }
        let corpse = self
            .corpses
            .slots
            .iter()
            .filter(|c| c.occupied && c.victim == id && c.spawn_time_ms >= meta.item_use_spawn_ms)
            .max_by_key(|c| c.spawn_time_ms)?;
        let tree = self.player_bodies.corpse_anim_trees.get(&corpse.entnum)?;
        let kit = self.collision_kit(id);
        let body = kit.body.as_ref()?;
        let mut models = vec![(body.as_ref(), None)];
        if let (Some(head), Some(tag)) = (
            kit.head.as_ref(),
            xmodel_runtime::tp_head_attach_tag(&body.pose.bone_names),
        ) {
            models.push((
                head.as_ref(),
                Some(xmodel_runtime::Attach {
                    parent_model: 0,
                    tag: tag.into(),
                }),
            ));
        }
        let axis = math_iw4::angles_to_axis(corpse.viewangles);
        let transform = glam::Mat4::from_cols(
            glam::Vec3::from_array(axis[0]).extend(0.0),
            glam::Vec3::from_array(axis[1]).extend(0.0),
            glam::Vec3::from_array(axis[2]).extend(0.0),
            glam::Vec3::from_array(corpse.origin).extend(1.0),
        );
        xmodel_runtime::collision_models(
            &models,
            &xmodel_runtime::DObjPoseRequest::with_tree(tree.clone()),
            transform,
        )
        .ok()
    }

    fn player_hitvol_bones(
        &self,
        id: ClientId,
        ps: &PlayerState,
    ) -> Result<(Vec<xmodel_runtime::CollisionBone>, Option<[f32; 3]>), String> {
        let kit = self.collision_kit(id);
        let Some(cap) = kit.body.as_ref() else {
            return Ok((Vec::new(), None));
        };
        let world = glam::Mat4::from_rotation_translation(
            glam::Quat::from_rotation_z(ps.viewangles[1].to_radians()),
            glam::Vec3::from_array(ps.origin),
        );
        let legs = PlayerAnimValue::from_raw((ps.legs_anim as u16) & PLAYER_ANIM_RAW_MASK)
            .map(PlayerAnimValue::effective_index)
            .unwrap_or(0);
        let torso = PlayerAnimValue::from_raw((ps.torso_anim as u16) & PLAYER_ANIM_RAW_MASK)
            .map(PlayerAnimValue::effective_index)
            .unwrap_or(0);
        if legs != 0
            && !self
                .player_bodies
                .player_anim_trees
                .get(&id.0)
                .is_some_and(|slot| {
                    slot.leaf == legs
                        && slot.torso == torso
                        && slot.kit == self.collision_kit_index(id)
                })
        {
            return Err(format!(
                "player {} body tree unavailable for legs {legs}/torso {torso}",
                id.0
            ));
        }
        let (request, _, _, _, _, _, _) = self.player_dobj_request(id, ps);
        let input = player_controller_input(ps);
        let controller = move |dobj: &xmodel_runtime::DObj,
                               _: &anim_iw4::PartBits,
                               locals: &mut [anim_iw4::Local]| {
            xmodel_runtime::apply_player_controller(dobj, locals, input);
        };
        let mut models: Vec<(
            &xmodel_runtime::RetainedModelCapability,
            Option<xmodel_runtime::Attach>,
        )> = match (
            kit.head.as_ref(),
            xmodel_runtime::tp_head_attach_tag(&cap.pose.bone_names),
        ) {
            (Some(head), Some(tag)) => vec![
                (cap.as_ref(), None),
                (
                    head.as_ref(),
                    Some(xmodel_runtime::Attach {
                        parent_model: 0,
                        tag: tag.into(),
                    }),
                ),
            ],
            (Some(_), None) | (None, _) => vec![(cap.as_ref(), None)],
        };
        if let Some(shield) = self.client_meta(id).and_then(|meta| meta.shield) {
            let capability = self
                .content
                .weapons()
                .shield_models
                .get(shield.weapon as usize)
                .and_then(Option::as_ref)
                .ok_or("shield collision capability unavailable")?;
            models.push((
                capability.as_ref(),
                Some(xmodel_runtime::Attach {
                    parent_model: 0,
                    tag: shield.tag().into(),
                }),
            ));
            let specs: Vec<_> = models
                .iter()
                .map(|(model, attach)| (&model.pose, attach.clone()))
                .collect();
            let dobj = xmodel_runtime::DObj::build(&specs).map_err(|e| format!("{e:?}"))?;
            let mut bones = xmodel_runtime::collision_dobj_with_controller(
                &dobj, &models, &request, world, controller,
            )
            .map_err(|e| format!("{e:?}"))?;
            let shield_slot = dobj.models.last().ok_or("shield DObj model missing")?;
            for bone in &mut bones {
                if usize::from(bone.bone) >= shield_slot.base {
                    bone.part_classification = crate::shield::HITLOC;
                }
            }
            let posed = xmodel_runtime::pose_dobj_with_controller(
                &dobj,
                &request,
                world,
                |dobj, _, locals| {
                    xmodel_runtime::apply_player_controller(dobj, locals, input);
                },
            )
            .map_err(|e| format!("{e:?}"))?;
            if !bones
                .iter()
                .any(|bone| bone.part_classification == crate::shield::HITLOC)
            {
                for surf in &capability.coll_surfs {
                    let index = shield_slot.base + usize::from(surf.bone);
                    if let Some(transform) = posed.get(index)
                        && let Some(mut bone) = xmodel_runtime::collision_bone_from_local_box(
                            index as u16,
                            surf.midpoint,
                            surf.half_size,
                            *transform,
                        )
                    {
                        bone.part_classification = crate::shield::HITLOC;
                        bones.push(bone);
                    }
                }
            }
            let tag = dobj
                .bones
                .iter()
                .position(|bone| bone.name == shield.tag())
                .ok_or("shield tag missing")?;
            let normal = posed
                .get(tag)
                .and_then(|matrix| matrix.x_axis.truncate().try_normalize())
                .ok_or("shield tag transform invalid")?
                .to_array();
            if !bones
                .iter()
                .any(|bone| bone.part_classification == crate::shield::HITLOC)
            {
                return Err("shield model has no collision geometry".into());
            }
            return Ok((bones, Some(normal)));
        }
        if let Some(slot) = self.player_bodies.player_dobjs.get(&id.0) {
            return xmodel_runtime::collision_dobj_with_controller(
                &slot.dobj, &models, &request, world, controller,
            )
            .map(|bones| (bones, None))
            .map_err(|e| format!("{e:?}"));
        }
        xmodel_runtime::collision_models_with_controller(&models, &request, world, controller)
            .map(|bones| (bones, None))
            .map_err(|e| format!("{e:?}"))
    }

    pub(crate) fn record_collision_history(
        &mut self,
        tick: Tick,
        msec: i32,
        player: impl Fn(ClientId) -> Option<PlayerState> + Copy,
    ) {
        use crate::bullet_collision::{HistoryFrame, HistoryPhase};
        let alive = || {
            self.clients
                .iter()
                .filter(|(_, meta)| meta.lifecycle == crate::ClientLifecycle::Alive)
        };
        self.player_bodies.tick_dobjs(alive().map(|(id, meta)| {
            (
                *id,
                &self.content.player_kits()[usize::from(Self::kit_assignment_is_axis(
                    meta.client_state_team,
                    meta.ffa_team,
                ))],
            )
        }));
        self.player_bodies.tick_trees(
            PlayerAnimInputs {
                definitions: [
                    self.content.player_anim_tree().as_ref(),
                    self.content.player_axis_anim_tree().as_ref(),
                ],
                properties: self.content.player_anim_properties(),
                branches: *self.content.player_body_branches(),
            },
            alive().map(|(id, meta)| {
                (
                    *id,
                    usize::from(Self::kit_assignment_is_axis(
                        meta.client_state_team,
                        meta.ffa_team,
                    )),
                )
            }),
            msec,
            player,
        );
        let mut error = None;
        let poses = self.alive_collision_poses_inner(player, &[], |e| {
            if error.is_none() {
                error = Some(e.to_owned());
            }
        });
        self.player_bodies.materialize_error = error;
        self.collision_runtime.players.push_frame(HistoryFrame {
            tick,
            phase: HistoryPhase::PostMovement,
            poses,
        });
    }

    pub fn record_entity_collision_history(&mut self, tick: Tick) {
        use crate::bullet_collision::{EntityCollisionEpoch, EntityCollisionFrame, HistoryPhase};
        let rows = self
            .entity_collision_capabilities
            .iter()
            .map(|capabilities| {
                let mut row = capabilities.trace_geom();
                row.epoch = EntityCollisionEpoch::Historical { frame: tick };
                row
            })
            .collect();
        self.collision_runtime
            .entities
            .push_frame(EntityCollisionFrame {
                tick,
                phase: HistoryPhase::PostMovement,
                rows,
            });
    }

    pub fn bullet_trace(
        &self,
        query: crate::bullet_collision::BulletTraceQuery,
        poses: Option<&[crate::bullet_collision::PlayerCollisionPose]>,
    ) -> crate::bullet_collision::TraceOutcome {
        let geoms: Vec<EntityCollisionTraceGeom> = self
            .entity_collision_capabilities
            .iter()
            .filter(|capabilities| {
                capabilities.ray_may_hit(self.content.clip_cmodels(), query.start, query.end)
            })
            .map(EntityCollisionCapabilities::trace_geom)
            .collect();
        let default = [];
        let players = poses.unwrap_or_else(|| {
            self.collision_runtime
                .players
                .latest_poses()
                .map(|(_, p)| p)
                .unwrap_or(&default)
        });
        crate::bullet_collision::bullet_trace_with_entity_models(
            crate::world_graph::AtlasClip {
                brushes: self.content.clip_brushes(),
                bsp: self.content.clip_bsp(),
                cmodels: self.content.clip_cmodels(),
                mesh: self.content.clip_mesh(),
            },
            players,
            &geoms,
            &query,
            &|piece| self.world_objects.glass_is_solid(piece as u32),
        )
    }

    /// Readonly sensor path: same world clip and current script-model geoms as
    /// `bullet_trace`. Sight still uses `MASK_SIGHT` (no glass); shots use `MASK_SHOT`.
    pub fn sensor_trace(
        &self,
        query: crate::bullet_collision::BulletTraceQuery,
    ) -> crate::bullet_collision::TraceOutcome {
        use crate::bullet_collision::TraceOutcome;
        let Some(atlas) = self.content.atlas() else {
            return self.bullet_trace(query, None);
        };
        let players = self
            .collision_runtime
            .players
            .latest_poses()
            .map(|(_, p)| p)
            .unwrap_or(&[]);
        let base = crate::world_graph::AtlasClip {
            brushes: self.content.clip_brushes(),
            bsp: self.content.clip_bsp(),
            mesh: self.content.clip_mesh(),
            cmodels: self.content.clip_cmodels(),
        };
        let (mut start, mut end) = (query.start, query.end);
        for _ in 0..16 {
            let crossing = atlas.crossing(start, end);
            let leg_end = crossing.map_or(end, |(fraction, _, _)| {
                std::array::from_fn(|i| start[i] + (end[i] - start[i]) * fraction)
            });
            let outcome = if crate::world_graph::world_at(start) == 0 {
                self.bullet_trace(
                    crate::bullet_collision::BulletTraceQuery {
                        start,
                        end: leg_end,
                        ..query
                    },
                    None,
                )
            } else {
                atlas
                    .trace_masked(base, players, query.ignore, start, leg_end, query.mask)
                    .1
            };
            if !matches!(outcome, TraceOutcome::Miss { .. }) {
                return outcome;
            }
            let Some((_, portal, from_a)) = crossing else {
                return outcome;
            };
            if query.mask & 1 != 0
                && let Some(barrier) = atlas.unavailable_exit_hit(
                    self.world_objects.portal_blocked_exits(),
                    start,
                    end,
                )
            {
                return barrier;
            }
            let (near, far, yaw) = atlas.portals[portal.index()].oriented(from_a);
            start = crate::world_graph::carry_point(&near, &far, yaw, leg_end);
            end = crate::world_graph::carry_point(&near, &far, yaw, end);
        }
        TraceOutcome::Miss { end: start }
    }

    pub fn clip_brush_count(&self) -> usize {
        self.content.clip_brushes().len()
    }

    pub(crate) fn pmove_walking(&self, id: ClientId) -> Option<i32> {
        self.last_pmove_walking.get(&id).copied()
    }

    pub(crate) fn set_pmove_walking(&mut self, id: ClientId, walking: i32) {
        self.last_pmove_walking.insert(id, walking);
    }

    pub(crate) fn stuck_holdrand_mut(&mut self) -> &mut u32 {
        &mut self.stuck_holdrand
    }

    pub(crate) fn set_stuck_ejects(&mut self, pairs: Vec<(ClientId, ClientId)>) {
        self.last_stuck_ejects = pairs;
    }

    pub(crate) fn client_ids_sorted(&self) -> Vec<ClientId> {
        let mut ids: Vec<_> = self.clients.iter().map(|(id, _)| *id).collect();
        ids.sort_by_key(|c| c.0);
        ids
    }

    pub fn client_meta(&self, id: ClientId) -> Option<&ClientMatchState> {
        self.clients.iter().find(|(c, _)| *c == id).map(|(_, m)| m)
    }

    pub fn packed_client_name(&self, id: ClientId) -> [u8; 16] {
        self.client_meta(id).map(|m| m.name).unwrap_or([0; 16])
    }

    pub fn client_count(&self) -> usize {
        self.clients.len()
    }

    pub fn corpses(&self) -> &crate::PlayerCorpsePool {
        &self.corpses
    }

    pub(crate) fn corpses_mut(&mut self) -> &mut crate::PlayerCorpsePool {
        &mut self.corpses
    }

    pub(crate) fn script_model_anim(&self, name: &str) -> Option<crate::ScriptModelPlayAnim> {
        self.content
            .script_model_anims()
            .get(&name.to_ascii_lowercase())
            .copied()
    }

    pub(crate) fn player_anim_clip(&self, legs_anim: i32) -> Option<Arc<xmodel_runtime::AnimClip>> {
        let definition = self.content.player_anim_tree().as_ref()?;
        let value = PlayerAnimValue::from_raw((legs_anim as u16) & PLAYER_ANIM_RAW_MASK)?;
        match &definition
            .nodes()
            .get(value.effective_index() as usize)?
            .kind
        {
            xmodel_runtime::XAnimNodeKind::Leaf { clip, .. } => Some(Arc::clone(clip)),
            _ => None,
        }
    }

    pub(crate) fn player_anim_clip_named(
        &self,
        name: &str,
    ) -> Option<Arc<xmodel_runtime::AnimClip>> {
        let definition = self.content.player_anim_tree().as_ref()?;
        definition.nodes().iter().find_map(|node| match &node.kind {
            xmodel_runtime::XAnimNodeKind::Leaf { clip, .. }
                if clip.name.eq_ignore_ascii_case(name) =>
            {
                Some(Arc::clone(clip))
            }
            _ => None,
        })
    }

    pub(crate) fn corpse_dobj_tree_install(&mut self, entnum: i32, legs_anim: i32) {
        self.player_bodies.corpse_anim_trees.remove(&entnum);
        let Some(definition) = self.content.player_anim_tree().clone() else {
            return;
        };
        let Some(value) = PlayerAnimValue::from_raw((legs_anim as u16) & PLAYER_ANIM_RAW_MASK)
        else {
            return;
        };
        let leaf = value.effective_index();
        if leaf == 0
            || !matches!(
                definition.nodes().get(leaf as usize).map(|n| &n.kind),
                Some(xmodel_runtime::XAnimNodeKind::Leaf { .. })
            )
        {
            return;
        }
        let mut runtime = xmodel_runtime::XAnimTreeRuntime::new(definition);
        if runtime
            .set_complete_goal_weight(xmodel_runtime::XAnimNodeId(leaf), 0.0, 1.0)
            .is_err()
        {
            return;
        }
        self.player_bodies.corpse_anim_trees.insert(entnum, runtime);
    }

    pub(crate) fn corpse_dobj_tree_delta(&mut self, entnum: i32, msec: i32) -> Option<[f32; 3]> {
        self.player_bodies.corpse_delta(entnum, msec)
    }

    pub(crate) fn corpse_dobj_tree_retain(&mut self, live: &[i32]) {
        self.player_bodies
            .corpse_anim_trees
            .retain(|k, _| live.contains(k));
    }

    pub(crate) fn item_pickups_mut(&mut self) -> &mut Vec<crate::ItemPickupRecord> {
        &mut self.item_pickups
    }

    pub(crate) fn client_meta_mut(&mut self, id: ClientId) -> &mut ClientMatchState {
        if let Some(idx) = self.clients.iter().position(|(c, _)| *c == id) {
            return &mut self.clients[idx].1;
        }
        self.clients.push((id, ClientMatchState::default()));
        &mut self.clients.last_mut().expect("just pushed").1
    }

    pub(crate) fn snapshot_with_dynamic_rows(
        &self,
        tick: Tick,
        players: Vec<(ClientId, PlayerState)>,
        projectiles: Vec<ProjectileState>,
        script_movers: Vec<crate::gentity::ScriptMoverGentity>,
        dropped_items: Vec<crate::item::DroppedItem>,
    ) -> Snapshot {
        let clients = self
            .clients
            .iter()
            .map(|(id, m)| {
                let mut meta = m.to_snapshot_meta();
                if meta.shield.is_some()
                    && meta.lifecycle == crate::ClientLifecycle::Alive
                    && let Some(ps) = players
                        .iter()
                        .find_map(|(client, ps)| (*client == *id).then_some(ps))
                    && let Ok((bones, Some(normal))) = self.player_hitvol_bones(*id, ps)
                {
                    meta.shield_collision = Some(crate::ShieldCarrierCollision { normal, bones });
                }

                let (archival, current) = crate::hudelem::hud_elem_update_client(
                    &self.g_hudelems,
                    *id,
                    meta.client_state_team,
                    crate::hudelem::HUDELEM_UPDATE_BOTH,
                );
                meta.hud_archival = archival;
                meta.hud_current = current;
                (*id, meta)
            })
            .collect();
        let mut entities: Vec<_> = script_movers.iter().map(|mover| mover.state).collect();
        let shown_hidden: std::collections::BTreeSet<ScriptModelId> = script_movers
            .iter()
            .filter(|mover| mover.shown_to != 0)
            .map(|mover| mover.id)
            .collect();
        for projectile in projectiles.iter() {
            if projectile.entnum != playerstate_iw4::ENTITYNUM_NONE {
                entities.push(crate::gentity::init_missile_state(
                    projectile.entnum,
                    projectile.weapon,
                    projectile.pos,
                    projectile.apos,
                    projectile.launch_time,
                ));
            }
        }
        entities.extend(self.dying_missiles.iter().copied());
        for item in &dropped_items {
            entities.push(item.state);
        }
        let item_ammo = dropped_items
            .iter()
            .map(|item| crate::DroppedItemAmmo {
                entnum: item.state.number,
                clip_r: item.clip_r,
                clip_l: item.clip_l,
                stock: item.stock,
                scavenger: i32::from(item.scavenger),
                parented: self
                    .entity_kernel
                    .current_ref(item.state.number)
                    .and_then(|entity| self.entity_kernel.resolve(entity))
                    .is_ok_and(|view| view.relations.parent.is_some()),
            })
            .collect();
        Snapshot {
            tick,
            players,
            projectiles,
            meta: SnapshotMeta {
                objectives: self.objectives.clone(),
                phase: self.phase,
                round_serial: self.round_serial,
                match_elapsed_ms: self.match_elapsed_ms,
                prematch: self.prematch,
                score_limit: self.bootstrap.score_limit,
                time_limit_ms: self.bootstrap.time_limit_ms,
                kind: self.bootstrap.kind,
                clients,
                journal: self.events.journal().to_vec(),
                entity_events: self.events.entity_events().to_vec(),
                pellet_fx: self.events.pellet_fx().to_vec(),
                sound_aliases: self.sound_alias_cs.occupied(),
                effect_names: self.effect_name_cs.occupied(),
                hud_materials: self.hud_material_cs.occupied(),
                hud_strings: self.hud_string_cs.occupied(),
                world_objects: {
                    let mut world_objects = self.world_objects.to_snapshot();
                    world_objects.as_of_ms =
                        i32::try_from(self.match_elapsed_ms).unwrap_or(i32::MAX);
                    world_objects
                },
                area_entities: self
                    .area_entity_world
                    .as_ref()
                    .map(crate::AreaEntityWorldSnapshot::capture),
                entity_dobjs: self
                    .entity_collision_capabilities
                    .iter()
                    .filter(|capabilities| {
                        !capabilities.hidden
                            || capabilities
                                .owner
                                .script_model()
                                .is_some_and(|id| shown_hidden.contains(&id))
                    })
                    .filter_map(|capabilities| {
                        capabilities
                            .dobj
                            .as_ref()
                            .map(|dobj| (capabilities.owner, dobj.semantic_state.clone()))
                    })
                    .collect(),
                entities,
                script_movers,
                generations: self.entity_kernel.generations(),
                corpses: self.corpses,
                item_ammo,
                item_pickups: self.item_pickups.clone(),
            },
        }
    }

    pub fn set_old_buttons(&mut self, id: ClientId, buttons: u32) {
        if let Some(row) = self.old_buttons.iter_mut().find(|(c, _)| *c == id) {
            row.1 = buttons;
            return;
        }
        self.old_buttons.push((id, buttons));
    }

    pub fn set_old_cmd_angles(&mut self, id: ClientId, angles: [i32; 3]) {
        if let Some(row) = self.old_cmd_angles.iter_mut().find(|(c, _)| *c == id) {
            row.1 = angles;
            return;
        }
        self.old_cmd_angles.push((id, angles));
    }

    pub fn set_old_cmd(&mut self, id: ClientId, buttons: u32, angles: [i32; 3]) {
        self.set_old_buttons(id, buttons);
        self.set_old_cmd_angles(id, angles);
    }

    pub fn old_buttons(&self, id: ClientId) -> Option<u32> {
        self.old_buttons
            .iter()
            .find(|(c, _)| *c == id)
            .map(|(_, buttons)| *buttons)
    }

    pub fn old_cmd_angles(&self, id: ClientId) -> Option<[i32; 3]> {
        self.old_cmd_angles
            .iter()
            .find(|(c, _)| *c == id)
            .map(|(_, angles)| *angles)
    }

    pub(crate) fn push_event(&mut self, tick: Tick, audience: EventAudience, event: SimEvent) {
        self.events.push(tick, audience, event);
    }
    pub fn push_entity_event(
        &mut self,
        tick: Tick,
        audience: EventAudience,
        event: entity_iw4::EntityEventKind,
        payload: EntityEventPayload,
    ) {
        self.events.push_entity(tick, audience, event, payload);
    }
    pub fn entity_events(&self) -> &[EntityEventRecord] {
        self.events.entity_events()
    }
    pub fn next_entity_event_sequence(&self) -> EventSequence {
        self.events.next_entity_event_sequence()
    }
    pub fn pellet_fx(&self) -> &[crate::PelletFxRecord] {
        self.events.pellet_fx()
    }
    pub(crate) fn push_pellet_fx(&mut self, record: crate::PelletFxRecord) {
        self.events.push_pellet_fx(record);
    }

    pub fn collision_scene(&self) -> crate::CollisionScene {
        let mut scene = crate::CollisionScene {
            content: Arc::clone(&self.content),
            capabilities: self.entity_collision_capabilities.clone(),
            world_objects: self.world_objects.clone(),
        };
        scene.materialize_models();
        scene
    }

    pub fn content(&self) -> Arc<SimContent> {
        Arc::clone(&self.content)
    }

    fn replace_content(&mut self, content: Arc<SimContent>) {
        if !Arc::ptr_eq(&self.content, &content) {
            self.player_bodies.reset();
            self.collision_runtime.reset();
            self.portal_clearance_signature = None;
            self.portal_clearance_rows.clear();
            self.portal_clearance_validated.clear();
            self.portal_clearance_job = None;
        }
        self.content = content;
    }

    pub fn install_content(&mut self, content: Arc<SimContent>) {
        let panes = match &content.map().glass {
            asset_core::GlassContent::Prepared(glass) => glass
                .panes()
                .iter()
                .copied()
                .enumerate()
                .map(|(id, pane)| (id as u32, pane))
                .collect(),
            _ => Vec::new(),
        };
        self.world_objects.install_glass_panes(panes);
        self.replace_content(content);
        self.player_bodies.materialize_error = None;
        self.reset_area_entity_world();
        self.recompute_content_digest();
    }

    pub(crate) fn old_buttons_mut(&mut self) -> &mut Vec<(ClientId, u32)> {
        &mut self.old_buttons
    }

    pub(crate) fn old_cmd_angles_mut(&mut self) -> &mut Vec<(ClientId, [i32; 3])> {
        &mut self.old_cmd_angles
    }

    pub fn shot_collision_verdicts(&self) -> &[crate::combat::ShotCollisionVerdict] {
        &self.shot_collision_verdicts
    }

    pub(crate) fn record_shot_collision_verdicts(
        &mut self,
        verdicts: &[crate::combat::ShotCollisionVerdict],
    ) {
        self.shot_collision_verdicts.extend_from_slice(verdicts);
    }

    pub fn projectile_impacts(&self) -> &[ProjectileImpact] {
        &self.projectile_impacts
    }

    pub fn projectile_impact_log(&self) -> &[(Tick, ProjectileImpact)] {
        &self.projectile_impact_log
    }

    pub(crate) fn record_projectile_impacts(&mut self, tick: Tick, impacts: &[ProjectileImpact]) {
        self.projectile_impacts.extend_from_slice(impacts);
        for impact in impacts {
            if self.projectile_impact_log.len() >= 64 {
                self.projectile_impact_log.remove(0);
            }
            self.projectile_impact_log.push((tick, *impact));
        }
    }

    pub fn hitvol_dump(
        &self,
        player: impl Fn(ClientId) -> Option<PlayerState>,
    ) -> Vec<HitvolDumpRow> {
        let pen_table_loaded = self.content.weapons().pen_table_loaded;
        let error = self.player_bodies.materialize_error.clone();
        match self.collision_runtime.players.latest_poses() {
            Some((_, poses)) if !poses.is_empty() => poses
                .iter()
                .map(|pose| {
                    let kit = self.collision_kit(pose.client);
                    let ps = player(pose.client);
                    let (pose_kind, leaf, clip, anim_time, persist, node_time, goal_weight) =
                        self.hitvol_anim_census(pose.client, ps.as_ref());
                    let (dobj_persist, dobj_models) = self
                        .player_bodies
                        .player_dobjs
                        .get(&pose.client.0)
                        .map(|s| (s.persist, s.dobj.models.len() as i64))
                        .map_or((None, None), |(p, n)| (Some(p), Some(n)));
                    let cycle_count = self
                        .player_bodies
                        .player_anim_trees
                        .get(&pose.client.0)
                        .and_then(|s| {
                            s.runtime
                                .states()
                                .get(s.leaf as usize)
                                .map(|st| i64::from(st.cycle_count))
                        });
                    let bone_count = pose.bones.len() as u32;
                    let first = pose.bones.first();
                    let pelvis = pose.bones.iter().find(|bone| bone.part_classification == 5);
                    let head = pose.bones.iter().find(|bone| bone.part_classification == 2);
                    let ctl = self.hitvol_controller_census(pose.client, ps.as_ref());
                    HitvolDumpRow {
                        client: Some(pose.client),
                        bone_count,
                        geom: if bone_count > 0 { "bones" } else { "aabb" },
                        body_key: kit.body_key.clone(),
                        head_key: kit.head_key.clone(),
                        pose_kind,
                        anim: ps.as_ref().map(PlayerState::anim),
                        leaf: Some(i64::from(leaf)),
                        clip,
                        anim_time: Some(anim_time),
                        first_cx: first.map(|b| b.center[0]),
                        first_cy: first.map(|b| b.center[1]),
                        first_cz: first.map(|b| b.center[2]),
                        pelvis_hx: pelvis.map(|b| b.half_size[0]),
                        pelvis_hy: pelvis.map(|b| b.half_size[1]),
                        pelvis_hz: pelvis.map(|b| b.half_size[2]),
                        pelvis_cz: pelvis.map(|b| b.center[2]),
                        head_x: head.map(|b| b.center[0]),
                        head_y: head.map(|b| b.center[1]),
                        head_z: head.map(|b| b.center[2]),
                        controller: ctl.kind,
                        pitch: ctl.pitch,
                        e_flags: ctl.e_flags,
                        pm_flags: ctl.pm_flags,
                        movetype: self.last_anim_movetype(pose.client).map(i64::from),
                        leanf: ctl.leanf,
                        ctl_tags: ctl.ctl_tags,
                        tag_origin: ctl.tag_origin,
                        tag_ox: ctl.tag_ox,
                        tag_oy: ctl.tag_oy,
                        pen_table_loaded,
                        error: error.clone(),
                        clock_owner: (pose_kind == "anim").then_some("tree"),
                        tree_persist: persist,
                        node_time,
                        time_unit: node_time.map(|_| "normalized"),
                        goal_weight,
                        dobj_persist,
                        cycle_count,
                        dobj_models,
                    }
                })
                .collect(),
            _ => vec![HitvolDumpRow {
                client: None,
                bone_count: 0,
                geom: "none",
                body_key: self.content.player_kits()[0].body_key.clone(),
                head_key: self.content.player_kits()[0].head_key.clone(),
                pose_kind: self.player_body_pose_kind(),
                anim: None,
                leaf: None,
                clip: String::new(),
                anim_time: None,
                first_cx: None,
                first_cy: None,
                first_cz: None,
                pelvis_hx: None,
                pelvis_hy: None,
                pelvis_hz: None,
                pelvis_cz: None,
                head_x: None,
                head_y: None,
                head_z: None,
                controller: "none",
                pitch: None,
                e_flags: None,
                pm_flags: None,
                movetype: None,
                leanf: None,
                ctl_tags: None,
                tag_origin: None,
                tag_ox: None,
                tag_oy: None,
                pen_table_loaded,
                error,
                clock_owner: None,
                tree_persist: None,
                node_time: None,
                time_unit: None,
                goal_weight: None,
                dobj_persist: None,
                cycle_count: None,
                dobj_models: None,
            }],
        }
    }

    fn hitvol_anim_census(
        &self,
        client: ClientId,
        ps: Option<&PlayerState>,
    ) -> (
        &'static str,
        u16,
        String,
        f32,
        Option<i64>,
        Option<f32>,
        Option<f32>,
    ) {
        let Some(ps) = ps else {
            return ("none", 0, String::new(), 0.0, None, None, None);
        };
        let (_, kind, leaf, time, persist, node_time, goal_weight) =
            self.player_dobj_request(client, ps);
        let clip = self
            .content
            .player_anim_node_names()
            .get(leaf as usize)
            .cloned()
            .unwrap_or(String::new());
        (kind, leaf, clip, time, persist, node_time, goal_weight)
    }

    fn hitvol_controller_census(
        &self,
        client: ClientId,
        ps: Option<&PlayerState>,
    ) -> HitvolControllerCensus {
        let Some(ps) = ps else {
            return HitvolControllerCensus::none();
        };
        let input = player_controller_input(ps);
        let tags = self.collision_kit(client).body.as_ref().map_or(0, |cap| {
            xmodel_runtime::PLAYER_CONTROLLER_TAGS
                .iter()
                .filter(|tag| cap.pose.bone_names.iter().any(|n| n == *tag))
                .count() as i64
        });
        let kind = if input.prone { "prone" } else { "standing" };
        let (off, _) = xmodel_runtime::player_controller_tag_origin(input);
        HitvolControllerCensus {
            kind,
            pitch: Some(ps.viewangles[0]),
            e_flags: Some(ps.e_flags),
            pm_flags: Some(ps.pm_flags),
            leanf: Some(ps.leanf),
            ctl_tags: Some(tags),
            tag_origin: Some(1),
            tag_ox: Some(off[0]),
            tag_oy: Some(off[1]),
        }
    }

    pub(crate) fn clear_tick_events(&mut self, tick: Tick) {
        self.events.clear_tick(tick);
        self.item_pickups.clear();
        self.shot_collision_verdicts.clear();
        self.projectile_impacts.clear();
    }
}

struct HitvolControllerCensus {
    kind: &'static str,
    pitch: Option<f32>,
    e_flags: Option<u32>,
    pm_flags: Option<u32>,
    leanf: Option<f32>,
    ctl_tags: Option<i64>,
    tag_origin: Option<i64>,
    tag_ox: Option<f32>,
    tag_oy: Option<f32>,
}

impl HitvolControllerCensus {
    fn none() -> Self {
        Self {
            kind: "none",
            pitch: None,
            e_flags: None,
            pm_flags: None,
            leanf: None,
            ctl_tags: None,
            tag_origin: None,
            tag_ox: None,
            tag_oy: None,
        }
    }
}

fn player_controller_input(ps: &PlayerState) -> xmodel_runtime::PlayerControllerInput {
    xmodel_runtime::PlayerControllerInput {
        view_pitch_deg: ps.viewangles[0],
        prone: ps.e_flags & playerstate_iw4::eflags::PRONE != 0,
        crouch: ps.e_flags & playerstate_iw4::eflags::DUCK != 0,
        lean_frac: math_iw4::get_lean_fraction(ps.leanf),
    }
}

pub(crate) fn clip_move_to_model_brushes(
    mut hit: trace_iw4::Trace,
    brushes: &[SimBrush],
    input: movement_iw4::GroundTraceInput,
) -> trace_iw4::Trace {
    if hit.fraction == 0.0 || brushes.is_empty() {
        return hit;
    }
    let other = trace_iw4::trace_capsule(
        brushes.iter().map(clipmap_iw4::brush_ref),
        input.start,
        input.end,
        input.mins,
        input.maxs,
        input.tracemask,
    );
    let startsolid = hit.startsolid | other.startsolid;
    let allsolid = hit.allsolid | other.allsolid;
    if other.fraction < hit.fraction {
        hit = other;
    }
    hit.startsolid = startsolid;
    hit.allsolid = allsolid;
    hit
}

pub(crate) fn clip_move_to_bmodels<'a>(
    mut hit: trace_iw4::Trace,
    cmodels: &[clipmap_iw4::ClipCmodel],
    leafbrushes: &[u16],
    brushes: &[SimBrush],
    linked: impl IntoIterator<Item = &'a LinkedBrushCollisionBrush>,
    input: movement_iw4::GroundTraceInput,
) -> trace_iw4::Trace {
    if hit.fraction == 0.0 {
        return hit;
    }
    for brush in linked {
        let Some(cmodel) = clipmap_iw4::clip_handle_to_model(cmodels, brush.cmodel_handle) else {
            continue;
        };
        let other = clipmap_iw4::transformed_capsule_trace(
            cmodel,
            leafbrushes,
            brushes,
            &clipmap_iw4::TraceExtents {
                start: input.start,
                end: input.end,
                mins: input.mins,
                maxs: input.maxs,
                mask: input.tracemask,
            },
            brush.origin,
            brush.angles,
        );
        if other.fraction >= hit.fraction && other.startsolid == 0 && other.allsolid == 0 {
            continue;
        }
        let startsolid = hit.startsolid | other.startsolid;
        let allsolid = hit.allsolid | other.allsolid;
        if other.fraction < hit.fraction {
            hit = other;
        }
        hit.startsolid = startsolid;
        hit.allsolid = allsolid;
        if hit.fraction == 0.0 && startsolid != 0 {
            return hit;
        }
    }
    hit
}

thread_local! {
    static LEAF_AABB_SCRATCH: std::cell::RefCell<Vec<u16>> =
        std::cell::RefCell::new(Vec::new());
}

pub(crate) fn clip_trace(
    brushes: &[SimBrush],
    bsp: &SimClipBsp,
    mesh: &SimClipMesh,
    ext: &clipmap_iw4::TraceExtents,
    glass_is_solid: &dyn Fn(u16) -> bool,
) -> trace_iw4::Trace {
    use std::cell::Cell;

    let map = clipmap_iw4::ClipMapRef {
        nodes: &bsp.nodes,
        leaves: &bsp.leaves,
        leafbrushes: &bsp.leafbrushes,
        brushes,
    };
    let mesh_ref = clipmap_iw4::ClipMeshRef {
        verts: &mesh.tables.verts,
        tri_indices: &mesh.tables.tri_indices,
        tri_edge_is_walkable: &mesh.tables.tri_edge_is_walkable,
        tri_surface_flags: &mesh.tables.tri_surface_flags,
        tri_content_flags: &mesh.tables.tri_content_flags,
        aabb_trees: &mesh.tables.aabb_trees,
        partitions: &mesh.tables.partitions,
        borders: &mesh.tables.borders,
        aabb_roots: &mesh.tables.aabb_roots,
    };

    let open = || trace_iw4::Trace {
        fraction: 1.0,
        endpos: ext.end,
        ..trace_iw4::Trace::default()
    };

    let no_brushes = map.brushes.is_empty();
    let no_mesh = mesh_ref.tri_indices.len() < 3;
    if no_brushes && no_mesh {
        return open();
    }

    if map.nodes.is_empty() || map.leaves.is_empty() {
        let mut best = open();

        if !no_brushes {
            best = clipmap_iw4::trace_linear_with_glass(&map, ext, glass_is_solid);
        }

        if !no_mesh {
            clipmap_iw4::trace_through_mesh_into(&mesh_ref, ext, &mut best);
        }

        return best;
    }

    let mut best = open();

    let has_leaf_aabbs = !no_mesh && clipmap_iw4::leaves_have_coll_aabb(map.leaves);
    let frac = Cell::new(1.0_f32);
    LEAF_AABB_SCRATCH.with(|scratch| {
        let mut scratch = scratch.borrow_mut();
        clipmap_iw4::walk_clip_tree(&map, ext, &|| frac.get(), &mut |leaf| {
            if best.fraction == 0.0 {
                frac.set(0.0);
                return;
            }
            if !no_brushes {
                clipmap_iw4::trace_leaf_brushes_into(&map, leaf, ext, glass_is_solid, &mut best);

                if best.fraction == 0.0 {
                    frac.set(0.0);
                    return;
                }
            }
            if !no_mesh {
                clipmap_iw4::trace_leaf_mesh_into(
                    &mesh_ref,
                    leaf,
                    ext,
                    has_leaf_aabbs,
                    &mut best,
                    &mut scratch,
                );
            }
            frac.set(best.fraction);
        })
    });
    if !no_mesh {
        let mut ignored_winner = clipmap_iw4::ClipWorldWinner::Open;
        clipmap_iw4::finish_mesh_forest_fallback(
            &mesh_ref,
            has_leaf_aabbs,
            ext,
            &mut best,
            &mut ignored_winner,
        );
    }
    best
}

/// A trace against one map's world clip alone, with glass intact: no
/// entities and no static models. For tools that read a map without a match.
pub fn trace_world_clip(
    brushes: &[SimBrush],
    bsp: &SimClipBsp,
    mesh: &SimClipMesh,
    ext: &clipmap_iw4::TraceExtents,
) -> trace_iw4::Trace {
    clip_trace(brushes, bsp, mesh, ext, &|_| true)
}

pub(crate) fn gsc_give_weapon_is_akimbo(script_name: &str) -> bool {
    script_name.contains("_akimbo")
}

pub(crate) fn give_weapon_to_ps_akimbo(ps: &mut PlayerState, weapon: u32, akimbo: bool) {
    if weapon == 0 {
        ps.weapon = 0;
        ps.scope_zoom_level = 0;
        ps.weapon_primary = 0;
        ps.last_weapon_hand = 0;
        return;
    }
    inventory_add_weapon(ps, weapon, akimbo);
    if ps.weapons.iter().all(|&slot| slot != weapon as i32) {
        if let Some(slot) = ps.weapons.first_mut() {
            *slot = weapon as i32;
            weapon_iw4::latch_weapon_dual_wield(&ps.weapons, &mut ps.weapon_data, weapon, akimbo);
        }
    }
    ps.weapon = weapon;
    ps.scope_zoom_level = 0;
    ps.weapon_primary = weapon;
    ps.last_weapon_hand = weapon_iw4::num_hands_for_held(&ps.weapons, &ps.weapon_data, weapon);
}

pub(crate) fn inventory_add_weapon(ps: &mut PlayerState, weapon: u32, akimbo: bool) {
    if weapon == 0 {
        return;
    }
    let want = weapon as i32;
    if !ps.weapons.iter().any(|&slot| slot == want) {
        if let Some(slot) = ps.weapons.iter_mut().find(|slot| **slot == 0) {
            *slot = want;
        }
    }
    weapon_iw4::latch_weapon_dual_wield(&ps.weapons, &mut ps.weapon_data, weapon, akimbo);
}

pub fn blank_player_state() -> PlayerState {
    PlayerState::ZERO
}

pub(crate) fn spawn_player_state(origin: [f32; 3], viewangles: [f32; 3]) -> PlayerState {
    let mut ps = blank_player_state();
    ps.origin = origin;
    ps.viewangles = viewangles;
    ps.speed = 190;
    ps.health = 100;
    ps.max_health = 100;
    ps.move_speed_scale_multiplier = 1.0;
    ps.view_height_target = 60;
    ps.view_height_current = 60.0;
    ps.gravity = 800;
    ps.ground_entity_num = 0x7fe;

    ps.other_flags |= playerstate_iw4::other_flags::PLAYER;
    ps.corpse_index = -1;
    ps
}
