//! Exclusive authority input tapes. Samples are complete commands, never render-frame deltas.
use bevy_ecs::prelude::Resource;
use playerstate_iw4::{PlayerState, UserCmd};
use sha2::{Digest, Sha256};

use crate::{ClientId, CommandSequence, LifeSequence, PlayerCommand, Tick, TickInput};

#[derive(Clone, Debug)]
pub struct InputProgram {
    pub client: ClientId,
    pub quantum_ms: i32,
    pub first_sequence: CommandSequence,
    /// server_time is relative to the prepared player's command_time.
    pub samples: Vec<UserCmd>,
}

#[derive(Clone, Debug)]
pub struct InputReceipt {
    pub tick: Tick,
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

/// Ownership lasts until match teardown, including after completion or failure.
/// This prevents a render-frame command from silently becoming part of a take.
#[derive(Resource, Debug)]
pub struct InputProgramRun {
    pub program: InputProgram,
    pub digest: [u8; 32],
    pub initial_player: PlayerState,
    pub life: LifeSequence,
    pub start_tick: Option<Tick>,
    pub issued: usize,
    pub receipts: Vec<InputReceipt>,
    pub shots: Vec<crate::AcceptedShot>,
    pub launches: Vec<crate::ProjectileState>,
    pub shot_verdicts: Vec<crate::ShotCollisionVerdict>,
    pub shot_scenes: Vec<InputShotScene>,
    pub failure: Option<String>,
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
        if program.quantum_ms <= 0 || 50 % program.quantum_ms != 0 {
            return Err("quantum_ms must be a positive divisor of 50".into());
        }
        let duration = i32::try_from(program.samples.len())
            .ok()
            .and_then(|n| n.checked_mul(program.quantum_ms))
            .ok_or("input program duration overflow")?;
        if duration == 0 || duration > 120_000 || duration % 50 != 0 {
            return Err("input program duration must be 50..120000 ms, divisible by 50".into());
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
            if cmd.server_time != (index as i32 + 1) * program.quantum_ms {
                return Err(format!("sample {index} has a noncanonical time"));
            }
            let owned = |weapon: u16| player.weapons.contains(&i32::from(weapon));
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
        let mut hash = Sha256::new();
        hash.update(b"iw4l-input-program-v1\0");
        hash.update(program.client.0.to_le_bytes());
        hash.update(program.quantum_ms.to_le_bytes());
        hash.update(program.first_sequence.0.to_le_bytes());
        for sample in &program.samples {
            hash.update(sample.canonical_bytes());
        }
        let digest = hash.finalize().into();
        Ok(Self {
            digest,
            program,
            initial_player: player,
            life: meta.life_sequence,
            start_tick: None,
            issued: 0,
            receipts: Vec::new(),
            shots: Vec::new(),
            launches: Vec::new(),
            shot_verdicts: Vec::new(),
            shot_scenes: Vec::new(),
            failure: None,
            actors: Vec::new(),
        })
    }

    pub fn complete(&self) -> bool {
        self.failure.is_none()
            && self.issued == self.program.samples.len()
            && self.receipts.len() == self.issued
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
        life: Option<LifeSequence>,
    ) -> TickInput {
        let mut resolved = input.clone();
        let actor = self.program.client;
        resolved.cmds.retain(|cmd| cmd.client != actor);
        resolved.shot_samples.retain(|((id, _), _)| *id != actor);
        if resolved.actions.iter().any(|(id, _)| *id == actor) {
            self.failure
                .get_or_insert("actor action conflicts with exclusive input program".into());
        }
        resolved.actions.retain(|(id, _)| *id != actor);
        if self.failure.is_some() || self.issued == self.program.samples.len() {
            return resolved;
        }
        if life != Some(self.life) || msec != 50 {
            self.failure = Some("actor life or authority step changed".into());
            return resolved;
        }
        let start = *self.start_tick.get_or_insert(tick);
        let expected_tick = start.0 + (self.issued as i32 * self.program.quantum_ms / 50) as u32;
        if tick.0 != expected_tick || self.receipts.len() != self.issued {
            self.failure = Some("authority tick discontinuity or unconsumed sample".into());
            return resolved;
        }
        let end =
            (self.issued + (50 / self.program.quantum_ms) as usize).min(self.program.samples.len());
        for index in self.issued..end {
            let mut command = self.program.samples[index];
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
