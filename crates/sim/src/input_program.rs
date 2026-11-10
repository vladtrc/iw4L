//! Exclusive authority input tapes. Samples are complete commands, never render-frame deltas.
use bevy_ecs::prelude::Resource;
use playerstate_iw4::{PlayerState, UserCmd};
use sha2::{Digest, Sha256};

use crate::{ClientId, CommandSequence, LifeSequence, PlayerCommand, Tick, TickInput};

#[derive(Clone, Debug)]
pub struct InputActorPreparation {
    pub name: String,
    pub rank: i32,
    pub prestige: i32,
    pub title: u32,
    pub icon: u32,
    pub score: i32,
    pub kills: i32,
    pub deaths: i32,
    pub streak: Option<InputStreakPreparation>,
}

#[derive(Clone, Debug)]
pub struct InputStreakPreparation {
    pub count: i32,
    pub loadout: [String; 3],
    pub last_earned: Option<String>,
    pub owned: Vec<String>,
}

#[derive(Clone, Debug, Default)]
pub struct InputStreakState {
    pub count: i32,
    pub last_earned: Option<String>,
    pub owned: Vec<String>,
}

#[derive(Resource, Default)]
pub(crate) struct PendingInputStreaks(
    pub std::collections::BTreeMap<ClientId, InputStreakPreparation>,
);

#[derive(Debug)]
pub struct InputFinalKillcam {
    pub start: Tick,
    pub last: Tick,
    pub client: ClientId,
    pub target: i32,
    pub length_ms: i32,
    pub closed: Option<Tick>,
}

pub(crate) fn observe_final_killcams(
    cams: &mut Vec<InputFinalKillcam>,
    tick: Tick,
    seats: &[(ClientId, i32, i32)],
) {
    for cam in cams.iter_mut().filter(|c| c.closed.is_none()) {
        if !seats.iter().any(|&(client, target, length)| {
            client == cam.client && target == cam.target && length == cam.length_ms
        }) {
            cam.closed = Some(tick);
        }
    }
    for &(client, target, length_ms) in seats {
        if let Some(cam) = cams.iter_mut().find(|c| {
            c.closed.is_none()
                && c.client == client
                && c.target == target
                && c.length_ms == length_ms
        }) {
            cam.last = tick;
        } else {
            cams.push(InputFinalKillcam {
                start: tick,
                last: tick,
                client,
                target,
                length_ms,
                closed: None,
            });
        }
    }
}

#[derive(Clone, Debug)]
pub struct InputProgram {
    pub client: ClientId,
    pub quantum_ms: i32,
    pub first_sequence: CommandSequence,
    /// server_time is relative to the prepared player's command_time.
    pub samples: Vec<UserCmd>,
    /// Declarations grant no inventory; possession is checked before issuance.
    pub future_weapons: Vec<u16>,
    pub stop_on_death: bool,
    /// UI replies are a separate input channel; opt in explicitly for menu-driven scenarios.
    pub allow_menu_responses: bool,
    pub scoreboard_window_ms: Option<[i32; 2]>,
}

#[derive(Debug, PartialEq, Eq)]
pub struct InputMatchState {
    pub tick: Tick,
    pub ended: bool,
    pub score_limit: i32,
    pub scores: Vec<(ClientId, i32, i32, i32)>,
}

#[derive(Clone, Debug)]
pub struct InputReceipt {
    pub tick: Tick,
    pub source_command: PlayerCommand,
    pub command: PlayerCommand,
    pub player: PlayerState,
}

#[derive(Clone, Debug)]
pub struct InputTargetSample {
    pub client: ClientId,
    pub player: PlayerState,
    pub head: Option<[f32; 3]>,
}

#[derive(Clone, Debug)]
pub struct InputShotScene {
    pub tick: Tick,
    pub shot: crate::ShotId,
    pub targets: Vec<InputTargetSample>,
}

pub(crate) struct InputActorState<'a> {
    pub life: Option<LifeSequence>,
    pub lifecycle: Option<crate::ClientLifecycle>,
    pub player: Option<&'a PlayerState>,
}

/// Ownership lasts until match teardown, including after completion or failure.
/// This prevents a render-frame command from silently becoming part of a take.
#[derive(Resource, Debug)]
pub struct InputProgramRun {
    pub program: InputProgram,
    pub digest: [u8; 32],
    pub initial_player: PlayerState,
    pub initial_meta: crate::ClientSnapshotMeta,
    pub initial_streak: InputStreakState,
    pub final_killcams: Vec<InputFinalKillcam>,
    pub match_states: Vec<InputMatchState>,
    pub scoreboard_samples: Vec<(Tick, [f32; 3])>,
    pub menu_responses: Vec<(Tick, String, String)>,
    pub life: LifeSequence,
    pub start_tick: Option<Tick>,
    pub last_input_tick: Option<Tick>,
    pub issued: usize,
    pub receipts: Vec<InputReceipt>,
    pub shots: Vec<crate::AcceptedShot>,
    pub launches: Vec<crate::ProjectileState>,
    pub shot_verdicts: Vec<crate::ShotCollisionVerdict>,
    pub shot_scenes: Vec<InputShotScene>,
    pub failure: Option<String>,
    pub death_tick: Option<Tick>,
    /// Future weapons the actor has not held yet; once held, a lost weapon is
    /// ignored by weapon selection like any other unowned request.
    pub pending_weapons: Vec<u16>,
    pub events: Vec<crate::EventRecord>,
    pub pickups: Vec<(Tick, crate::ItemPickupRecord)>,
    pub actors: Vec<InputProgramRun>,
}

impl InputProgramRun {
    pub(crate) fn prepare(world: &crate::SimWorld, program: InputProgram) -> Result<Self, String> {
        if !world.cheats_enabled() || !world.is_running() {
            return Err("input program needs an active authority with cheats".into());
        }
        let player = *world
            .player(program.client)
            .ok_or("input program actor is absent")?;
        let meta = world
            .client_meta(program.client)
            .ok_or("input program actor has no seat")?;
        if meta.lifecycle != crate::ClientLifecycle::Alive {
            return Err("input program actor must be alive".into());
        }
        if world.old_buttons(program.client).unwrap_or(0) != 0 {
            return Err("release all buttons before starting an input program".into());
        }
        if program.quantum_ms < 0 || (program.quantum_ms > 0 && 50 % program.quantum_ms != 0) {
            return Err(
                "quantum_ms must be zero (recorded timing) or a positive divisor of 50".into(),
            );
        }
        let duration = program.samples.last().map_or(0, |cmd| cmd.server_time);
        if duration == 0 || duration > 120_000 || duration % 50 != 0 {
            return Err("input program duration must be 50..120000 ms, divisible by 50".into());
        }
        if program
            .scoreboard_window_ms
            .is_some_and(|[start, end]| start < 0 || start >= end || end > duration)
        {
            return Err("scoreboard window must be inside the input tape".into());
        }
        if program.first_sequence.0 == 0
            || program
                .first_sequence
                .0
                .checked_add(program.samples.len() as u32)
                .is_none()
        {
            return Err("input program sequence range is invalid".into());
        }
        player
            .command_time
            .checked_add(duration)
            .ok_or("input program clock overflow")?;
        for (index, cmd) in program.samples.iter().enumerate() {
            let previous = index
                .checked_sub(1)
                .map_or(0, |i| program.samples[i].server_time);
            if cmd.server_time <= previous
                || (program.quantum_ms > 0
                    && i32::try_from(index + 1)
                        .ok()
                        .and_then(|n| n.checked_mul(program.quantum_ms))
                        != Some(cmd.server_time))
            {
                return Err(format!("sample {index} has a noncanonical time"));
            }
            let owned = |weapon: u16| {
                player.weapons.contains(&i32::from(weapon))
                    || program.future_weapons.contains(&weapon)
            };
            let alternate = owned(cmd.weapon_mapped)
                && world
                    .combat_facts_for(u32::from(cmd.weapon_mapped))
                    .is_some_and(|facts| facts.alternate_weapon == u32::from(cmd.weapon));
            if cmd.weapon != 0 && !owned(cmd.weapon) && !alternate {
                return Err(format!(
                    "sample {index} selects unowned weapon {}",
                    cmd.weapon
                ));
            }
            if cmd.off_hand_index != 0 && !owned(cmd.off_hand_index) {
                return Err(format!(
                    "sample {index} selects unowned offhand {}",
                    cmd.off_hand_index
                ));
            }
            if !cmd.melee_charge_yaw.is_finite()
                || cmd.gun_angle_offset.iter().any(|v| !v.is_finite())
            {
                return Err(format!("sample {index} contains a nonfinite angle"));
            }
        }
        for &weapon in &program.future_weapons {
            if weapon == 0 || world.combat_facts_for(u32::from(weapon)).is_none() {
                return Err(format!("unknown future weapon {weapon}"));
            }
        }
        let mut hash = Sha256::new();
        hash.update(b"iw4l-input-program-v1\0");
        hash.update(program.client.0.to_le_bytes());
        hash.update(program.quantum_ms.to_le_bytes());
        hash.update(program.first_sequence.0.to_le_bytes());
        if !program.future_weapons.is_empty() || program.stop_on_death {
            hash.update(b"lifecycle-v1\0");
            hash.update([u8::from(program.stop_on_death)]);
            hash.update((program.future_weapons.len() as u32).to_le_bytes());
            for weapon in &program.future_weapons {
                hash.update(weapon.to_le_bytes());
            }
        }
        if program.allow_menu_responses {
            hash.update(b"menu-responses-v1\0");
        }
        if let Some([start, end]) = program.scoreboard_window_ms {
            hash.update(b"scoreboard-v1\0");
            hash.update(start.to_le_bytes());
            hash.update(end.to_le_bytes());
        }
        for sample in &program.samples {
            hash.update(sample.canonical_bytes());
        }
        let digest = hash.finalize().into();
        let initial_streak = world.input_actor_streak(program.client);
        let pending_weapons = program
            .future_weapons
            .iter()
            .copied()
            .filter(|&weapon| !player.weapons.contains(&i32::from(weapon)))
            .collect();
        Ok(Self {
            digest,
            program,
            initial_player: player,
            initial_meta: meta.to_snapshot_meta(),
            initial_streak,
            final_killcams: Vec::new(),
            match_states: Vec::new(),
            scoreboard_samples: Vec::new(),
            menu_responses: Vec::new(),
            life: meta.life_sequence,
            start_tick: None,
            last_input_tick: None,
            issued: 0,
            receipts: Vec::new(),
            shots: Vec::new(),
            launches: Vec::new(),
            shot_verdicts: Vec::new(),
            shot_scenes: Vec::new(),
            failure: None,
            death_tick: None,
            pending_weapons,
            events: Vec::new(),
            pickups: Vec::new(),
            actors: Vec::new(),
        })
    }

    pub fn complete(&self) -> bool {
        self.failure.is_none()
            && (self.death_tick.is_some()
                || (self.issued == self.program.samples.len()
                    && self.receipts.len() == self.issued))
            && self.actors.iter().all(Self::complete)
    }

    pub fn actor(&self, client: ClientId) -> Option<&Self> {
        if self.program.client == client {
            Some(self)
        } else {
            self.actors
                .iter()
                .find(|actor| actor.program.client == client)
        }
    }

    pub fn actor_mut(&mut self, client: ClientId) -> Option<&mut Self> {
        if self.program.client == client {
            Some(self)
        } else {
            self.actors
                .iter_mut()
                .find(|actor| actor.program.client == client)
        }
    }

    pub fn ensemble_digest(&self) -> [u8; 32] {
        let mut hash = Sha256::new();
        hash.update(b"iw4l-input-ensemble-v1\0");
        hash.update(self.digest);
        for actor in &self.actors {
            hash.update(actor.digest);
        }
        hash.finalize().into()
    }

    pub(crate) fn input(
        &mut self,
        tick: Tick,
        input: &TickInput,
        msec: i32,
        state: InputActorState<'_>,
        alternate: impl Fn(u16, u16) -> bool,
    ) -> TickInput {
        let InputActorState {
            life,
            lifecycle,
            player,
        } = state;
        let mut resolved = input.clone();
        let actor = self.program.client;
        resolved.cmds.retain(|cmd| cmd.client != actor);
        resolved.shot_samples.retain(|((id, _), _)| *id != actor);
        let allow_menu_responses = self.program.allow_menu_responses;
        let independent_action = |action: &crate::ClientAction| {
            matches!(action, crate::ClientAction::SoundDone { .. })
                || (allow_menu_responses
                    && matches!(action, crate::ClientAction::MenuResponse { .. }))
        };
        if let Some((_, action)) = resolved
            .actions
            .iter()
            .find(|(id, action)| *id == actor && !independent_action(action))
        {
            self.failure.get_or_insert_with(|| {
                format!("actor action conflicts with exclusive input program: {action:?}")
            });
        }
        // Sound completion and explicitly enabled UI replies belong to their
        // presentation channels. Actor movement and inventory remain tape-owned.
        resolved
            .actions
            .retain(|(id, action)| *id != actor || independent_action(action));
        for (_, action) in resolved.actions.iter().filter(|(id, _)| *id == actor) {
            if let crate::ClientAction::MenuResponse { menu, response } = action {
                self.menu_responses.push((
                    tick,
                    crate::menu_response_text(menu).to_owned(),
                    crate::menu_response_text(response).to_owned(),
                ));
            }
        }
        if self.failure.is_some() {
            return resolved;
        }
        // A terminal track can stay complete when its dead player leaves the match.
        if self.death_tick.is_some() {
            if lifecycle == Some(crate::ClientLifecycle::Alive) {
                self.failure = Some("actor returned after terminal death".into());
            }
            return resolved;
        }
        if life != Some(self.life) || msec != 50 {
            self.failure = Some("actor life or authority step changed".into());
            return resolved;
        }
        if lifecycle == Some(crate::ClientLifecycle::Dead) && self.program.stop_on_death {
            self.death_tick.get_or_insert(tick);
            return resolved;
        }
        if self.issued == self.program.samples.len() {
            return resolved;
        }
        let start = *self.start_tick.get_or_insert(tick);
        let expected_tick = self.last_input_tick.map_or(start.0, |last| last.0 + 1);
        if tick.0 != expected_tick || self.receipts.len() != self.issued {
            self.failure = Some("authority tick discontinuity or unconsumed sample".into());
            return resolved;
        }
        self.last_input_tick = Some(tick);
        let through_ms = i64::from(tick.0 - start.0 + 1) * 50;
        let end = self
            .program
            .samples
            .partition_point(|cmd| i64::from(cmd.server_time) <= through_ms);
        if let Some(ps) = player {
            self.pending_weapons
                .retain(|&weapon| !ps.weapons.contains(&i32::from(weapon)));
        }
        for index in self.issued..end {
            let mut command = self.program.samples[index];
            let pending = |weapon| self.pending_weapons.contains(&weapon);
            if (pending(command.weapon)
                && !(command.weapon_mapped != 0
                    && !pending(command.weapon_mapped)
                    && alternate(command.weapon_mapped, command.weapon)))
                || pending(command.off_hand_index)
            {
                self.failure = Some(format!("sample {index} selects a weapon not yet acquired"));
                return resolved;
            }
            command.server_time += self.initial_player.command_time;
            resolved.cmds.push(PlayerCommand::sequenced(
                actor,
                command,
                CommandSequence(self.program.first_sequence.0 + index as u32),
            ));
        }
        self.issued = end;
        resolved.canonicalize();
        resolved
    }
}
