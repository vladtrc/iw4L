use bevy::ecs::system::SystemParam;
use bevy::prelude::*;
use frame::RuntimeRole;

use crate::client::runtime::LastAdoptedSnapshot;
use crate::{AuthorityClock, AuthorityWorld, FrameClock, GameActive};

#[derive(SystemParam)]
pub(crate) struct TimeScaleInputs<'w> {
    authority: Option<Res<'w, AuthorityWorld>>,
    authority_clock: Option<Res<'w, AuthorityClock>>,
    adopted: Option<Res<'w, LastAdoptedSnapshot>>,
    clock: Option<Res<'w, FrameClock>>,
    active: Option<Res<'w, GameActive>>,
}

pub(crate) fn apply_time_scale(
    role: Res<RuntimeRole>,
    inputs: TimeScaleInputs,
    fixed: Res<Time<Fixed>>,
    mut time: ResMut<Time<Virtual>>,
    paused: Option<Res<frame::GamePaused>>,
) {
    match (paused.is_some(), time.is_paused()) {
        (true, false) => time.pause(),
        (false, true) => time.unpause(),
        _ => {}
    }
    let TimeScaleInputs {
        authority,
        authority_clock,
        adopted,
        clock,
        active,
    } = inputs;
    let (plan, game_ms) = if role.runs_authority() {
        (
            authority.as_ref().and_then(|world| world.0.slow_motion()),
            authority_clock
                .as_ref()
                .map_or(0.0, |clock| f64::from(clock.time_ms))
                + fixed.overstep().as_secs_f64() * 1000.0,
        )
    } else {
        let snapshot = adopted
            .as_ref()
            .and_then(|a| a.next().or(a.snap.as_deref()))
            .filter(|_| active.as_ref().is_none_or(|active| active.0));
        let game_ms = clock.as_ref().filter(|clock| clock.started()).map_or_else(
            || snapshot.map_or(0.0, |s| f64::from(s.tick.0) * f64::from(sim::MATCH_TICK_MS)),
            |clock| f64::from(clock.time()),
        );
        (
            snapshot.and_then(|s| s.meta.objectives.slow_motion),
            game_ms,
        )
    };
    time.set_relative_speed(
        plan.filter(|p| p.valid())
            .map_or(1.0, |p| p.sample(game_ms)),
    );
}
