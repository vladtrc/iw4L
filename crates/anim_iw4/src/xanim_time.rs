pub const XANIM_NONLOOP_END_PARK: f32 = f32::from_bits(0x3f7ffffe);

pub const XANIM_WEIGHT_FLOOR: f32 = f32::from_bits(0x358637be);

pub const XANIM_WEIGHT_FLOOR_SCALE: f32 = 0.001;

pub fn xanim_advance_leaf_time(
    old_time: f32,
    cycle_count: i16,
    rate: f32,
    frequency: f32,
    dtime: f32,
    looping: bool,
) -> (f32, i16) {
    let delta = rate * frequency * dtime;
    if delta == 0.0 {
        return (old_time, cycle_count);
    }
    let mut time = old_time + delta;
    let mut cycle = cycle_count;
    if time >= 1.0 {
        if looping {
            while time >= 1.0 {
                time -= 1.0;
                cycle = cycle.wrapping_add(1);
            }
        } else if old_time - XANIM_NONLOOP_END_PARK < 0.0 {
            time = XANIM_NONLOOP_END_PARK;
        } else {
            time = 1.0;
        }
    }
    (time, cycle)
}

pub fn xanim_advance_goal_weight(
    weight: f32,
    goal_weight: f32,
    goal_time: f32,
    dtime: f32,
    parent_has_weight: bool,
) -> (f32, f32) {
    if !parent_has_weight || goal_time <= dtime {
        return (goal_weight, 0.0);
    }
    let mut weight = (goal_weight - weight) * dtime / goal_time + weight;
    if weight < XANIM_WEIGHT_FLOOR {
        weight = goal_weight * XANIM_WEIGHT_FLOOR_SCALE;
    }
    (weight, goal_time - dtime)
}
