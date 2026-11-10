use super::{ClientId, Tick};
use crate::bullet_collision::{CollisionHistory, EntityCollisionHistory, ShotSampleProvenance};
use std::collections::HashMap;

#[derive(Clone, Copy)]
pub(super) enum LagcompPlan {
    CurrentAuthority {
        reason: crate::bullet_collision::CurrentAuthorityReason,
    },
    Refused {
        requested: Tick,
        reason: &'static str,
    },
    Rewind {
        requested: Tick,
        used: Tick,
        clamped: bool,
    },
}

#[derive(Clone, Debug)]
pub(super) struct CollisionRuntime {
    pub(super) players: CollisionHistory,
    pub(super) entities: EntityCollisionHistory,
    samples: HashMap<ClientId, ShotSampleProvenance>,
    commands: HashMap<(ClientId, i32), ShotSampleProvenance>,
}

impl Default for CollisionRuntime {
    fn default() -> Self {
        Self {
            players: CollisionHistory::with_capacity(
                crate::bullet_collision::COLLISION_HISTORY_TICKS,
            ),
            entities: EntityCollisionHistory::with_capacity(
                crate::bullet_collision::COLLISION_HISTORY_TICKS,
            ),
            samples: HashMap::new(),
            commands: HashMap::new(),
        }
    }
}

impl CollisionRuntime {
    pub(super) fn reset(&mut self) {
        self.players.clear();
        self.entities.clear();
        self.samples.clear();
        self.commands.clear();
    }
    pub(super) fn forget_client(&mut self, client: ClientId) {
        self.samples.remove(&client);
        self.commands.retain(|(id, _), _| *id != client);
    }
    pub(super) fn set_commands(
        &mut self,
        rows: impl IntoIterator<Item = ((ClientId, i32), ShotSampleProvenance)>,
    ) {
        self.samples.clear();
        self.commands.clear();
        self.commands.extend(rows);
    }
    pub(super) fn select_command(&mut self, client: ClientId, time: i32) {
        let sample = self
            .commands
            .remove(&(client, time))
            .unwrap_or(ShotSampleProvenance::NO_CLAIM);
        self.set_sample(client, sample);
    }
    pub(super) fn set_sample(&mut self, client: ClientId, sample: ShotSampleProvenance) {
        self.samples.insert(client, sample);
    }
    pub(super) fn sample(&self, client: ClientId) -> Option<ShotSampleProvenance> {
        self.samples.get(&client).copied()
    }
    pub(super) fn plan(&self, attacker: ClientId, current: Tick) -> LagcompPlan {
        use crate::bullet_collision::{CurrentAuthorityReason, LAGCOMP_MAX_REWIND_TICKS};

        let Some(sample) = self.sample(attacker) else {
            return LagcompPlan::CurrentAuthority {
                reason: CurrentAuthorityReason::NoSampleClaim,
            };
        };
        if !sample.quality.claims_history() {
            return LagcompPlan::CurrentAuthority {
                reason: CurrentAuthorityReason::NoSampleClaim,
            };
        }
        if sample.right.0 > current.0 {
            return LagcompPlan::Refused {
                requested: sample.right,
                reason: "history sample after shot",
            };
        }
        if !sample.is_valid_for(current) {
            return LagcompPlan::Refused {
                requested: sample.requested_tick(),
                reason: "malformed history sample",
            };
        }
        let requested = sample.requested_tick();
        if requested == current {
            return LagcompPlan::CurrentAuthority {
                reason: CurrentAuthorityReason::SampleAtShot,
            };
        }
        let oldest_allowed = Tick(current.0.saturating_sub(LAGCOMP_MAX_REWIND_TICKS));
        let (used, clamped) = if requested.0 < oldest_allowed.0 {
            (oldest_allowed, true)
        } else {
            (requested, false)
        };
        if self.players.frame_at(used).is_none() {
            return LagcompPlan::CurrentAuthority {
                reason: CurrentAuthorityReason::HistoryUnavailable { requested: used },
            };
        }
        LagcompPlan::Rewind {
            requested,
            used,
            clamped,
        }
    }
}
