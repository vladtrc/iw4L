use bevy_ecs::world::World;
use std::sync::Arc;

use crate::Tick;
use crate::frame::FrameCheckpoint;
use crate::script::{MatchScript, RoundScript};

/// An in-process campaign checkpoint. Immutable asset and program data remain
/// shared; mutable simulation, VM, animation and entity payload state is owned.
/// This is not a disk save format and cannot be loaded into another map session.
#[derive(Clone)]
pub struct CampaignCheckpoint {
    content_digest: u64,
    save_info: Option<CampaignSaveInfo>,
    tick: Tick,
    frame: FrameCheckpoint,
    round: RoundScript,
    match_script: MatchScript,
    mechanics: crate::script::Mechanics,
    remote_missiles: Option<crate::remote_missile::RemoteMissiles>,
}

impl CampaignCheckpoint {
    pub fn save_info(&self) -> Option<&CampaignSaveInfo> {
        self.save_info.as_ref()
    }

    pub fn tick(&self) -> Tick {
        self.tick
    }

    pub(crate) fn capture(world: &World) -> Result<Self, String> {
        quiescent(world)?;
        let round = world.resource::<RoundScript>();
        if !round.started || round.loading || round.fault.is_some() {
            return Err("checkpoint requires healthy started campaign scripts".into());
        }
        let tick = round
            .last_tick
            .ok_or("checkpoint requires a completed frame")?;
        Ok(Self {
            content_digest: world
                .get::<crate::world::SimState>(crate::frame::state_entity(world))
                .unwrap()
                .checkpoint_content_digest(),
            save_info: None,
            tick,
            frame: FrameCheckpoint::capture(world)?,
            round: round.clone(),
            match_script: world.resource::<MatchScript>().clone(),
            mechanics: world.resource::<crate::script::Mechanics>().clone(),
            remote_missiles: world
                .get_resource::<crate::remote_missile::RemoteMissiles>()
                .cloned(),
        })
    }

    pub(crate) fn restore(&self, world: &mut World) -> Result<Tick, String> {
        quiescent(world)?;
        if world.contains_resource::<crate::input_program::InputProgramRun>() {
            return Err(
                "checkpoint restore requires releasing the input program's timeline".into(),
            );
        }
        let round = world.resource::<RoundScript>();
        let same_program = round
            .program
            .as_ref()
            .zip(self.round.program.as_ref())
            .is_some_and(|(a, b)| Arc::ptr_eq(a, b));
        let same_session = round
            .restart
            .as_ref()
            .zip(self.round.restart.as_ref())
            .is_some_and(|(a, b)| Arc::ptr_eq(a, b));
        let content = world
            .get::<crate::world::SimState>(crate::frame::state_entity(world))
            .unwrap()
            .content();
        if !same_program || !same_session || !Arc::ptr_eq(&content, &self.frame.content()) {
            return Err("checkpoint belongs to a different campaign map session".into());
        }

        Ok(self.apply(world))
    }

    fn apply(&self, world: &mut World) -> Tick {
        // User preferences, account state and unsaved dvars belong to the live
        // session. Restore only registered savegame dvars, alongside match state.
        let mut match_script = self.match_script.clone();
        let live = world.resource::<MatchScript>();
        match_script.dvars = live.dvars.clone();
        match_script.latched_dvars = live.latched_dvars.clone();
        let saved = crate::script::host::natives::iw4::saved::is_saved;
        match_script.dvars.retain(|name, _| !saved(name));
        match_script.latched_dvars.retain(|name, _| !saved(name));
        match_script.dvars.extend(
            self.match_script
                .dvars
                .iter()
                .filter(|(name, _)| saved(name))
                .map(|(k, v)| (k.clone(), v.clone())),
        );
        match_script.latched_dvars.extend(
            self.match_script
                .latched_dvars
                .iter()
                .filter(|(name, _)| saved(name))
                .map(|(k, v)| (k.clone(), v.clone())),
        );

        self.frame.restore(world);
        world.insert_resource(self.round.clone());
        world.insert_resource(match_script);
        world.insert_resource(self.mechanics.clone());
        // The menu command tail is delivery history, not an active menu stack.
        // A fresh frontend must reconstruct the current script menu without
        // replaying an Open command whose response the VM already consumed.
        {
            let mut frame = crate::frame::FrameWorld::from_world(world);
            for client in frame.client_ids_sorted() {
                let active = self
                    .round
                    .players
                    .get(&client.0)
                    .and_then(|slot| slot.menu.as_ref());
                frame.client_meta_mut(client).menu_commands.clear();
                if let Some(menu) = active {
                    if let Some(index) = hud_iw4::script_menu_cs_index(menu) {
                        frame.push_player_card_open(client, index);
                    } else {
                        frame
                            .client_meta_mut(client)
                            .push_menu_command(crate::MenuCommandKind::Open(menu.to_string()));
                    }
                }
            }
        }
        world.remove_resource::<crate::remote_missile::RemoteMissiles>();
        if let Some(missiles) = &self.remote_missiles {
            world.insert_resource(missiles.clone());
        }
        if let Some(mut saves) = world.get_resource_mut::<SaveMemory>() {
            saves.pending = None;
            saves.waiting = None;
            saves.forced = false;
            saves.loaded_at = Some(self.tick);
            saves.error = None;
        }
        crate::campaign_reload::reset(world);
        self.tick
    }
}

/// A committed checkpoint plus live user state, transferred across a complete
/// host teardown/install. The host recreates presentation and transport resources.
pub struct CampaignCheckpointLoad {
    checkpoint: CampaignCheckpoint,
    dvars: std::collections::BTreeMap<String, String>,
    latched_dvars: std::collections::BTreeMap<String, String>,
    profile: crate::LocalPlayerProfile,
    bindings: crate::LocalKeyBindings,
    persistent: crate::PersistentDataStore,
    next_save_id: i32,
}

impl std::fmt::Debug for CampaignCheckpointLoad {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CampaignCheckpointLoad")
            .field("tick", &self.checkpoint.tick())
            .field("save", &self.checkpoint.save_info())
            .finish_non_exhaustive()
    }
}

impl CampaignCheckpointLoad {
    pub(crate) fn prepare(world: &World) -> Result<Self, String> {
        quiescent(world)?;
        let saves = world
            .get_resource::<SaveMemory>()
            .ok_or("campaign save memory is absent")?;
        let checkpoint = saves
            .checkpoint()
            .ok_or("no committed campaign checkpoint")?
            .clone();
        let live = world.resource::<MatchScript>();
        Ok(Self {
            checkpoint,
            dvars: live.dvars.clone(),
            latched_dvars: live.latched_dvars.clone(),
            profile: *world.resource::<crate::LocalPlayerProfile>(),
            bindings: world.resource::<crate::LocalKeyBindings>().clone(),
            persistent: world.resource::<crate::PersistentDataStore>().clone(),
            next_save_id: saves.next_id,
        })
    }

    pub(crate) fn install(&self, world: &mut World) -> Result<Tick, String> {
        quiescent(world)?;
        let round = world.resource::<RoundScript>();
        let same_program = round
            .program
            .as_ref()
            .zip(self.checkpoint.round.program.as_ref())
            .is_some_and(|(a, b)| a.fingerprint() == b.fingerprint());
        let same_authored_level = round
            .restart
            .as_ref()
            .zip(self.checkpoint.round.restart.as_ref())
            .is_some_and(|(a, b)| {
                a.entities == b.entities
                    && a.path_nodes == b.path_nodes
                    && a.keys == b.keys
                    && a.absent_effects == b.absent_effects
            });
        let state = world
            .get::<crate::world::SimState>(crate::frame::state_entity(world))
            .unwrap();
        if !same_program
            || !same_authored_level
            || state.checkpoint_content_digest() != self.checkpoint.content_digest
        {
            return Err(
                "campaign checkpoint program or map content differs from the prepared map".into(),
            );
        }
        if !round
            .players
            .keys()
            .eq(self.checkpoint.round.players.keys())
        {
            return Err("campaign checkpoint local player differs from the prepared map".into());
        }
        if round.started
            || round.last_tick.is_some()
            || world.contains_resource::<crate::input_program::InputProgramRun>()
        {
            return Err("campaign checkpoint load requires a fresh map installation".into());
        }
        let map_epoch = state.world_objects().map_round_epoch();
        {
            let mut live = world.resource_mut::<MatchScript>();
            live.dvars = self.dvars.clone();
            live.latched_dvars = self.latched_dvars.clone();
        }
        world.insert_resource(self.profile);
        world.insert_resource(self.bindings.clone());
        world.insert_resource(self.persistent.clone());
        let tick = self.checkpoint.apply(world);
        crate::frame::FrameWorld::from_world(world)
            .world_objects_mut()
            .set_map_round_epoch(map_epoch);
        let mut saves = world.resource_mut::<SaveMemory>();
        saves.next_id = self.next_save_id;
        saves.committed = Some((
            self.checkpoint.save_info.as_ref().map_or(0, |info| info.id),
            self.checkpoint.clone(),
        ));
        Ok(tick)
    }
}

struct PendingSave {
    info: CampaignSaveInfo,
    commit: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CampaignSaveInfo {
    pub id: i32,
    /// Script save label; device path construction belongs to the storage backend.
    pub label: String,
    pub description: String,
    pub image: String,
    pub suppress_print: bool,
}

#[derive(bevy_ecs::prelude::Resource, Default)]
pub(crate) struct SaveMemory {
    next_id: i32,
    pending: Option<PendingSave>,
    waiting: Option<(i32, CampaignCheckpoint)>,
    committed: Option<(i32, CampaignCheckpoint)>,
    forced: bool,
    loaded_at: Option<Tick>,
    error: Option<String>,
}

impl SaveMemory {
    pub(crate) fn summary(&self) -> String {
        format!(
            "pending={:?} waiting={:?} committed={:?} loaded_at={:?} forced={} error={:?}",
            self.pending.as_ref().map(|save| save.info.id),
            self.waiting
                .as_ref()
                .map(|(id, checkpoint)| (*id, checkpoint.tick())),
            self.committed.as_ref().map(|(id, checkpoint)| (
                *id,
                checkpoint.tick(),
                checkpoint.save_info()
            )),
            self.loaded_at,
            self.forced,
            self.error,
        )
    }

    pub(crate) fn successful(&self) -> bool {
        self.error.is_none()
    }

    pub(crate) fn recently_loaded(&self, now: Tick) -> bool {
        self.loaded_at
            .is_some_and(|tick| now.0 >= tick.0 && now.0 - tick.0 < 2000 / crate::MATCH_TICK_MS)
    }

    pub(crate) fn request(
        &mut self,
        mut info: CampaignSaveInfo,
        commit: bool,
    ) -> Result<i32, String> {
        self.forced = false;
        if self.pending.is_some() {
            return Err("only one autosave can be requested per frame".into());
        }
        self.next_id = self
            .next_id
            .checked_add(1)
            .ok_or("save identifier exhausted")?;
        let id = self.next_id;
        info.id = id;
        self.pending = Some(PendingSave { info, commit });
        Ok(id)
    }

    pub(crate) fn commit_would_be_valid(&self, id: i32) -> bool {
        !self.forced && self.waiting.as_ref().is_some_and(|(saved, _)| *saved == id)
    }

    pub(crate) fn commit(&mut self, id: i32) -> Result<(), String> {
        if self.forced {
            self.forced = false;
            return Ok(());
        }
        if let Some((saved, _)) = &self.waiting {
            if *saved != id {
                return Err(format!(
                    "save {id} does not match the save waiting for commit ({saved})"
                ));
            }
            self.committed = self.waiting.take();
            return Ok(());
        }
        if self
            .committed
            .as_ref()
            .is_none_or(|(saved, _)| *saved == id)
        {
            return Ok(());
        }
        Err(format!("save {id} is not waiting for commit"))
    }

    pub(crate) fn checkpoint(&self) -> Option<&CampaignCheckpoint> {
        self.committed.as_ref().map(|(_, checkpoint)| checkpoint)
    }
}

pub(crate) fn process_pending(world: &mut World) {
    let Some(pending) = world
        .get_resource_mut::<SaveMemory>()
        .and_then(|mut saves| saves.pending.take())
    else {
        return;
    };
    {
        let mut saves = world.resource_mut::<SaveMemory>();
        saves.loaded_at = None;
        if pending.commit && saves.waiting.is_some() {
            saves.committed = saves.waiting.take();
            saves.forced = true;
        }
    }
    let result = CampaignCheckpoint::capture(world);
    let mut saves = world.resource_mut::<SaveMemory>();
    match result {
        Ok(mut checkpoint) => {
            saves.error = None;
            let id = pending.info.id;
            checkpoint.save_info = Some(pending.info);
            saves.waiting = Some((id, checkpoint));
            if pending.commit {
                saves.committed = saves.waiting.take();
                saves.forced = true;
            }
        }
        Err(error) => {
            diag::warn!(Sim, "campaign checkpoint capture failed: {error}");
            saves.waiting = None;
            saves.error = Some(error);
        }
    }
}

fn quiescent(world: &World) -> Result<(), String> {
    let round = world.resource::<RoundScript>();
    if round.profile != asset_core::RuntimeProfile::Campaign {
        return Err("checkpoints require campaign mode".into());
    }
    if world.contains_resource::<crate::step::StepRequest>()
        || !round.suspended.is_empty()
        || round.suspended_frames != 0
    {
        return Err("checkpoint operation requires a completed simulation frame".into());
    }
    Ok(())
}
