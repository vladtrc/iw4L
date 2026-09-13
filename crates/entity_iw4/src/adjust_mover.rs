use crate::trajectory::{Trajectory, bg_evaluate_trajectory};

pub const ET_GENERAL: i32 = 0;

pub const ET_PLAYER: i32 = 1;

pub const ET_PLAYER_CORPSE: i32 = 2;

pub const ET_ITEM: i32 = 3;

pub const ET_MISSILE: i32 = 4;

pub const ET_SCRIPTMOVER: i32 = 6;

pub const ET_PLANE: i32 = 0xd;

pub const ET_PRIMARY_LIGHT: i32 = 10;

pub fn mover_num_in_adjust_range(mover_num: i32) -> bool {
    (mover_num as u32).wrapping_sub(1) < 0x7fd
}

pub fn adjust_position_for_mover_evaluates(mover_num: i32, e_type: Option<i32>) -> bool {
    mover_num_in_adjust_range(mover_num) && matches!(e_type, Some(ET_SCRIPTMOVER) | Some(ET_PLANE))
}

pub fn cg_adjust_position_for_mover(
    input: [f32; 3],
    mover_num: i32,
    e_type: Option<i32>,
    pos: Option<&Trajectory>,
    from_time: i32,
    to_time: i32,
) -> [f32; 3] {
    if !adjust_position_for_mover_evaluates(mover_num, e_type) {
        return input;
    }
    let Some(pos) = pos else {
        panic!("CG_AdjustPositionForMover eType 6/0xd needs lerp.pos trajectory");
    };
    let old = bg_evaluate_trajectory(pos, from_time);
    let new = bg_evaluate_trajectory(pos, to_time);
    [
        input[0] + (new[0] - old[0]),
        input[1] + (new[1] - old[1]),
        input[2] + (new[2] - old[2]),
    ]
}
