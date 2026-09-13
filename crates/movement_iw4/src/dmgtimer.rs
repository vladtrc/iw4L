use math_iw4::vec_to_yaw;
use playerstate_iw4::PlayerState;

pub const PLAYER_DMGTIMER_TIME_PER_POINT: f32 = 100.0;

pub const PLAYER_DMGTIMER_MAX_TIME: f32 = 750.0;

pub const PLAYER_DMGTIMER_MIN_SCALE: f32 = 0.0;

pub const PLAYER_DMGTIMER_STUMBLE_TIME_MS: i32 = 500;

pub const PLAYER_DMGTIMER_FLINCH_TIME_MS: i32 = 500;

pub const ANIM_MT_FLINCH_FORWARD: u8 = 32;

const TURN_DEGREES: f32 = 360.0;

const FLINCH_SECTOR_DEGREES: [f32; 4] = [45.0, 135.0, 225.0, 315.0];

pub fn pm_update_damage_timer(ps: &mut PlayerState, damage: i32, dir: Option<[f32; 3]>) {
    let added = (PLAYER_DMGTIMER_TIME_PER_POINT * damage as f32) as i32;
    ps.damage_timer = ps.damage_timer.saturating_add(added);
    if PLAYER_DMGTIMER_MAX_TIME < ps.damage_timer as f32 {
        ps.damage_timer = PLAYER_DMGTIMER_MAX_TIME as i32;
    }
    ps.damage_duration = ps.damage_timer;
    ps.flinch_yaw_anim = match dir {
        None => 0,
        Some(dir) => flinch_yaw_anim(ps.viewangles[1], dir),
    };
}

fn flinch_yaw_anim(view_yaw: f32, dir: [f32; 3]) -> i32 {
    let view_yaw = if view_yaw < 0.0 {
        view_yaw + TURN_DEGREES
    } else {
        view_yaw
    };
    let mut bearing = vec_to_yaw(dir[0], dir[1]) - (view_yaw as i32) as f32;
    if bearing < 0.0 {
        bearing += TURN_DEGREES;
    }

    let [edge_45, edge_135, edge_225, edge_315] = FLINCH_SECTOR_DEGREES;
    if !(edge_45..edge_315).contains(&bearing) {
        return 0;
    }
    if (edge_135..edge_225).contains(&bearing) {
        return 1;
    }
    if (edge_45..edge_135).contains(&bearing) {
        return 2;
    }
    3
}

#[must_use]
pub fn pm_damage_scale_walk(damage_timer: i32) -> f32 {
    if damage_timer == 0 || PLAYER_DMGTIMER_MAX_TIME == 0.0 {
        return 1.0;
    }
    (-PLAYER_DMGTIMER_MIN_SCALE / PLAYER_DMGTIMER_MAX_TIME) * damage_timer as f32 + 1.0
}

pub fn pm_walk_move_drop_damage_timer(ps: &mut PlayerState, frametime_seconds: f32) {
    ps.damage_timer -= (frametime_seconds * 1000.0) as i32;
    if ps.damage_timer < 1 {
        ps.damage_timer = 0;
    }
}

#[must_use]
pub fn pm_damage_window_open(damage_timer: i32, damage_duration: i32, window_ms: i32) -> bool {
    damage_duration.saturating_sub(window_ms).max(0) < damage_timer
}
