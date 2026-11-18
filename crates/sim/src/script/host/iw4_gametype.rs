//! What the stock IW4 `_gamelogic` scripts mean to the match. Everything here
//! depends on the stock script layout, not on the VM.

use crate::frame::FrameWorld;
use crate::identities::MatchPhase;
use crate::script::runtime::{level_endon_armed, raise, return_from};
use crate::script::{RoundScript, Value};
use bevy_ecs::prelude::World;

pub(crate) fn apply_level_notify(frame: &mut FrameWorld, name: &str) {
    if name == "prematch_over" && frame.phase() == MatchPhase::Warmup {
        crate::score::finish_prematch(frame);
    }
}

const WAIT_FOR_PLAYERS: &str = "maps/mp/gametypes/_gamelogic::waitforplayers";
const START_TIMER_BEGINNING: &str = "match_start_timer_beginning";

pub(crate) fn force_match_start(world: &mut World, tick: crate::Tick) -> bool {
    let runtime = world.resource::<RoundScript>();
    if runtime.fault.is_some() || runtime.program.is_none() {
        return false;
    }
    let counting = level_endon_armed(world, START_TIMER_BEGINNING);
    let now = i64::from(tick.0) * i64::from(crate::MATCH_TICK_MS);
    let waiting = return_from(world, WAIT_FOR_PLAYERS, now);
    if !counting && waiting == 0 {
        return false;
    }
    world
        .resource_mut::<RoundScript>()
        .set_object_field(0, "prematchperiodend", Value::Int(0));
    raise(world, Value::level(), START_TIMER_BEGINNING, Vec::new());
    true
}

/// Cuts the stock `gracePeriod()` count to its last second, so the script
/// ends the grace period itself: `grace_period_ending`, `graceperiod_done`,
/// and from then on a dead team ends the round.
pub(crate) fn end_grace_period(world: &mut World) -> bool {
    let mut runtime = world.resource_mut::<RoundScript>();
    if runtime.fault.is_some() || runtime.program.is_none() {
        return false;
    }
    match runtime.object_field(0, "ingraceperiod") {
        Value::Int(left) if left > 1 => {
            runtime.set_object_field(0, "ingraceperiod", Value::Int(1));
            true
        }
        _ => false,
    }
}
