use std::collections::HashMap;

use bevy::prelude::Resource;

const CAPACITY: usize = 8192;
const RETENTION_MS: i32 = 5000;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum FireFxOccurrence {
    Muzzle,
    Brass,
    Tracer { pellet: u16, segment: u16 },
    Impact { pellet: u16, segment: u16 },
    Glass { pellet: u16, segment: u16 },
    Marks { pellet: u16, segment: u16 },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FireFxOutcome {
    Created(u32),
    Scheduled(u32),
    Applied(u32),
    NotApplicable(&'static str),
    PolicySkipped(&'static str),
    DependencyRefused(&'static str),
    RetryableFailure(fx::SpawnFail),
    Duplicate,
    HistoryCapacityRefused,
}

impl FireFxOutcome {
    fn completed(self) -> bool {
        matches!(
            self,
            Self::Created(_) | Self::Scheduled(_) | Self::Applied(_) | Self::PolicySkipped(_)
        )
    }
}

#[derive(Clone, Copy)]
pub struct FireFxRequest {
    pub cause: Option<sim::FireCause>,
    pub occurrence: FireFxOccurrence,
    pub now: i32,
}

impl FireFxRequest {
    pub fn event(
        event: &net::DispatchedEntityEvent,
        occurrence: FireFxOccurrence,
        now: i32,
    ) -> Self {
        Self {
            cause: event.payload.fire_cause,
            occurrence,
            now,
        }
    }
}

struct Receipt {
    at: i32,
    outcome: Option<FireFxOutcome>,
}

#[derive(Resource, Default)]
pub struct PresentedFireFx {
    receipts: HashMap<(sim::FireCause, FireFxOccurrence), Receipt>,
}

impl PresentedFireFx {
    pub fn maintain(&mut self, now: i32) {
        self.receipts
            .retain(|_, receipt| (0..RETENTION_MS).contains(&now.wrapping_sub(receipt.at)));
    }

    pub fn execute(
        &mut self,
        request: FireFxRequest,
        play: impl FnOnce() -> FireFxOutcome,
    ) -> FireFxOutcome {
        let key = request.cause.map(|cause| (cause, request.occurrence));
        if let Some(key) = key {
            if self.receipts.get(&key).is_some_and(|receipt| {
                !(0..RETENTION_MS).contains(&request.now.wrapping_sub(receipt.at))
            }) {
                self.receipts.remove(&key);
            }
            if let Some(receipt) = self.receipts.get_mut(&key) {
                if receipt.outcome.is_some() {
                    return FireFxOutcome::Duplicate;
                }
            } else if self.receipts.len() >= CAPACITY {
                return FireFxOutcome::HistoryCapacityRefused;
            }
        }
        let outcome = play();
        if let Some(key) = key {
            let receipt = self.receipts.entry(key).or_insert(Receipt {
                at: request.now,
                outcome: None,
            });
            if outcome.completed() {
                receipt.outcome = Some(outcome);
                receipt.at = request.now;
            }
        }
        outcome
    }
}
