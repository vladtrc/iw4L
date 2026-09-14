use crate::frame::FrameWorld;
use crate::{ScriptModelId, Tick};

#[derive(Clone, Debug, PartialEq)]
pub struct RadiationMovingDigger {
    pub id: ScriptModelId,
    pub start: [f32; 3],
    pub legs: Vec<([f32; 3], u32)>,
}

fn apply_origin(world: &mut FrameWorld, id: ScriptModelId, origin: [f32; 3]) {
    let Some(number) = world.gentity_number(id) else {
        return;
    };
    let Some(mover) = world.script_mover_mut_by_number(number) else {
        return;
    };
    mover.state.tr_base = origin;
}

pub(crate) fn advance(world: &mut FrameWorld, tick: Tick) {
    if world.radiation_moving_diggers.is_empty() {
        return;
    }
    let now = tick.0.saturating_mul(50);
    let diggers = world.radiation_moving_diggers.clone();
    for digger in diggers {
        let origin =
            gamemode_iw4::radiation_moving_digger_origin_at(now, digger.start, &digger.legs);
        apply_origin(world, digger.id, origin);
    }
}
