use sim::{ClientLifecycle, LifeSequence};

use crate::observation::{BotObservation, WeaponAction, WeaponSlot};

/// Ticks a failed decision waits before the same state may commit again. Without
/// it an impossible reload and an impossible switch can alternate every think.
const RETRY_TICKS: u32 = 20;
/// Slack over the simulator's own timing before a commitment is abandoned.
const DEADLINE_SLACK_TICKS: u32 = 8;
const TICK_MS: i32 = 50;
/// Used when the content has no timings for the weapon in hand. Reloading is a
/// legal command either way; only the comparison needs real numbers.
const DEFAULT_ACTION_MS: i32 = 2000;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WeaponAct {
    Switch { to: u16 },
    Reload,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WeaponFailure {
    /// The bot no longer carries the weapon it committed to.
    LostWeapon,
    /// Nothing left to load and nothing loaded to switch to.
    NoAmmo,
    Died,
    /// The simulator did not reach the committed outcome in its own time.
    Timeout,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum WeaponStatus {
    #[default]
    Idle,
    Running,
    Succeeded,
    Failed(WeaponFailure),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Commitment {
    act: WeaponAct,
    held: u16,
    clip_at_start: i32,
    life: LifeSequence,
    deadline_tick: u32,
    /// The simulator was observed entering the transition at least once.
    entered: bool,
}

/// The single owner of the weapon channel: it decides between reloading and
/// switching, then holds one request until the simulation confirms the outcome.
/// Lowering, raising, reload completion and ammunition transfer stay in the
/// weapon simulator.
#[derive(Clone, Copy, Debug, Default)]
pub struct WeaponSkill {
    commitment: Option<Commitment>,
    status: WeaponStatus,
    retry_after_tick: u32,
}

impl WeaponSkill {
    pub fn status(&self) -> WeaponStatus {
        self.status
    }

    pub fn act(&self) -> Option<WeaponAct> {
        self.commitment.map(|c| c.act)
    }

    /// The weapon id the command must keep requesting through the transition.
    /// Requesting the old id while the hand drops settles it back to ready, so a
    /// one-tick pulse is not a switching contract.
    pub fn requested_weapon(&self) -> Option<u16> {
        match self.commitment.map(|c| c.act) {
            Some(WeaponAct::Switch { to }) => Some(to),
            _ => None,
        }
    }

    pub fn wants_reload(&self) -> bool {
        matches!(self.commitment.map(|c| c.act), Some(WeaponAct::Reload))
    }

    pub fn reset(&mut self) {
        *self = Self::default();
    }

    /// One decision slice. `allow_start` is the caller's veto: an interaction
    /// close to completion is not displaced merely because a magazine is empty.
    pub fn update(&mut self, obs: &BotObservation, allow_start: bool) -> WeaponStatus {
        if let Some(mut commitment) = self.commitment {
            let outcome = commitment.progress(obs);
            self.commitment = Some(commitment);
            if outcome == WeaponStatus::Running {
                self.status = WeaponStatus::Running;
                return self.status;
            }
            self.commitment = None;
            self.status = outcome;
            if matches!(outcome, WeaponStatus::Failed(_)) {
                self.retry_after_tick = obs.tick.saturating_add(RETRY_TICKS);
            }
            return self.status;
        }
        self.status = WeaponStatus::Idle;
        if !allow_start || obs.tick < self.retry_after_tick {
            return self.status;
        }
        let Some(act) = choose(obs) else {
            return self.status;
        };
        self.commitment = Some(Commitment::begin(act, obs));
        self.status = WeaponStatus::Running;
        self.status
    }
}

impl Commitment {
    fn begin(act: WeaponAct, obs: &BotObservation) -> Self {
        let me = &obs.self_state;
        let held = held_slot(obs);
        let budget_ms = match act {
            WeaponAct::Switch { to } => obs
                .slot(to)
                .map_or(0, |slot| slot.switch_ms(&held))
                .max(held.facts.drop_time_ms),
            WeaponAct::Reload => held.reload_ms(),
        };
        let budget_ms = if budget_ms > 0 {
            budget_ms
        } else {
            DEFAULT_ACTION_MS
        };
        Self {
            act,
            held: me.weapon,
            clip_at_start: me.ammo_clip,
            life: me.life_sequence,
            deadline_tick: obs
                .tick
                .saturating_add((budget_ms.max(0) / TICK_MS) as u32)
                .saturating_add(DEADLINE_SLACK_TICKS),
            entered: false,
        }
    }

    fn progress(&mut self, obs: &BotObservation) -> WeaponStatus {
        let me = &obs.self_state;
        if me.life_sequence != self.life || me.lifecycle != ClientLifecycle::Alive {
            return WeaponStatus::Failed(WeaponFailure::Died);
        }
        match self.act {
            WeaponAct::Switch { to } => {
                if !obs.owns(to) {
                    return WeaponStatus::Failed(WeaponFailure::LostWeapon);
                }
                if me.weapon == to {
                    self.entered = true;
                    // The id alone is not the outcome: the hand still raises.
                    if me.weapon_action.is_settled() {
                        return WeaponStatus::Succeeded;
                    }
                } else if me.weapon != self.held {
                    return WeaponStatus::Failed(WeaponFailure::LostWeapon);
                } else if me.weapon_action == WeaponAction::Dropping {
                    self.entered = true;
                }
            }
            WeaponAct::Reload => {
                if me.weapon != self.held {
                    return WeaponStatus::Failed(WeaponFailure::LostWeapon);
                }
                if me.weapon_action == WeaponAction::Reloading {
                    self.entered = true;
                }
                if me.ammo_clip > self.clip_at_start {
                    return WeaponStatus::Succeeded;
                }
                if !self.entered && me.ammo_stock <= 0 {
                    return WeaponStatus::Failed(WeaponFailure::NoAmmo);
                }
            }
        }
        if obs.tick >= self.deadline_tick {
            return WeaponStatus::Failed(WeaponFailure::Timeout);
        }
        WeaponStatus::Running
    }
}

/// The weapon in hand. When the content carries no row for it, reloading is
/// still a legal command; only the comparison between actions needs timings.
fn held_slot(obs: &BotObservation) -> WeaponSlot {
    match obs.held() {
        Some(slot) => *slot,
        None => WeaponSlot {
            weapon: obs.self_state.weapon,
            ..WeaponSlot::default()
        },
    }
}

/// Combat equipment needs attention when the held magazine is empty. Reloading
/// and switching are then compared on the simulator's own timings, with a
/// visible threat deciding whether speed outranks keeping the current gun.
fn choose(obs: &BotObservation) -> Option<WeaponAct> {
    let me = &obs.self_state;
    if !me.weapon_action.is_settled() || me.ammo_clip > 0 {
        return None;
    }
    let held = held_slot(obs);
    // An offhand or an item in hand is not a readiness decision.
    if obs.held().is_some_and(|slot| !slot.facts.selectable) {
        return None;
    }
    let reload_ms = (me.ammo_stock > 0).then(|| held.reload_ms());
    let switch = obs
        .inventory
        .iter()
        .filter(|slot| slot.weapon != me.weapon && slot.is_loaded_gun())
        .map(|slot| (slot.switch_ms(&held), slot.weapon))
        .min();
    match (switch, reload_ms) {
        (Some((switch_ms, to)), Some(reload_ms)) => {
            let threatened = !obs.seen.is_empty();
            Some(if threatened && switch_ms < reload_ms {
                WeaponAct::Switch { to }
            } else {
                WeaponAct::Reload
            })
        }
        (Some((_, to)), None) => Some(WeaponAct::Switch { to }),
        (None, Some(_)) => Some(WeaponAct::Reload),
        (None, None) => None,
    }
}
