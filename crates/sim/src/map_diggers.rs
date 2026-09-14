use crate::frame::FrameWorld;
use crate::{ScriptModelId, Tick};

#[derive(Clone, Debug, PartialEq)]
pub struct RadiationDigger {
    pub body: ScriptModelId,
    pub arm: ScriptModelId,
    pub blade: ScriptModelId,
    pub pieces: Vec<(ScriptModelId, [f32; 3])>,
    pub body_angles: [f32; 3],
    pub arm_angles: [f32; 3],
    pub blade_angles: [f32; 3],
}

fn set_yaw_pitch(rest: [f32; 3], yaw: f32, pitch: f32) -> [f32; 3] {
    [rest[0] + pitch, rest[1] + yaw, rest[2]]
}

fn apply_angles(world: &mut FrameWorld, id: ScriptModelId, angles: [f32; 3]) {
    let Some(number) = world.gentity_number(id) else {
        return;
    };
    let Some(mover) = world.script_mover_mut_by_number(number) else {
        return;
    };
    mover.state.apos_tr_base = angles;
}

pub(crate) fn advance(world: &mut FrameWorld, tick: Tick) {
    if world.radiation_diggers.is_empty() {
        return;
    }
    let now = tick.0.saturating_mul(50);
    let mut fx = false;
    for (i, digger) in world.radiation_diggers.clone().into_iter().enumerate() {
        let pose = gamemode_iw4::radiation_digger_pose_at(now, i);
        fx |= pose.fx;
        apply_angles(
            world,
            digger.body,
            set_yaw_pitch(digger.body_angles, pose.body_yaw, 0.0),
        );
        apply_angles(
            world,
            digger.arm,
            set_yaw_pitch(digger.arm_angles, pose.body_yaw, pose.arm_pitch),
        );
        let blade = set_yaw_pitch(
            digger.blade_angles,
            pose.body_yaw,
            pose.arm_pitch + pose.blade_pitch,
        );
        apply_angles(world, digger.blade, blade);
        for (id, rest) in &digger.pieces {
            apply_angles(
                world,
                *id,
                set_yaw_pitch(*rest, pose.body_yaw, pose.arm_pitch + pose.blade_pitch),
            );
        }
    }
    if fx {
        world
            .script_gaps_mut()
            .raise(gamemode_iw4::ScriptGapCause::RadiationDiggerFx);
    }
}
