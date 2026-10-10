//! What the stock IW4 `_gamelogic` scripts mean to the match. Everything here
//! depends on the stock script layout, not on the VM.

use crate::frame::FrameWorld;
use crate::identities::MatchPhase;
use crate::script::runtime::{level_endon_armed, raise, return_from};
use crate::script::{Runtime, Value};
use bevy_ecs::prelude::World;

pub(crate) fn apply_level_notify(frame: &mut FrameWorld, name: &str) {
    let Some(mode) = frame.bootstrap_ref().mode else {
        return;
    };
    if name == mode.play_starts_on && frame.phase() == MatchPhase::Warmup {
        crate::score::finish_prematch(frame);
    }
}

const WAIT_FOR_PLAYERS: &str = "maps/mp/gametypes/_gamelogic::waitforplayers";
const START_TIMER_BEGINNING: &str = "match_start_timer_beginning";

pub(crate) fn force_match_start(world: &mut World, tick: crate::Tick) -> bool {
    let runtime = world.resource::<Runtime>();
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
        .resource_mut::<Runtime>()
        .set_object_field(0, "prematchperiodend", Value::Int(0));
    raise(world, Value::level(), START_TIMER_BEGINNING, Vec::new());
    true
}
