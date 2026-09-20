pub const DOOR_TIME_MS: u32 = 8_000;

pub const DOOR_COOLDOWN_MS: u32 = 20_000;

pub const UNAVAILABLE_MS: u32 = DOOR_TIME_MS + DOOR_COOLDOWN_MS;

pub const STARTUP_DELAY_MS: u32 = 300;

pub const D1_NEW_ANGLE: f32 = 123.0;

pub const D2_NEW_ANGLE: f32 = -123.0;

pub const OPEN_D1_ACCEL_S: f32 = 4.8;

pub const OPEN_D2_ACCEL_S: f32 = 5.6;

pub const CLOSE_D1_ACCEL_S: f32 = 5.6;

pub const CLOSE_D2_ACCEL_S: f32 = 4.8;

pub const KILL_EDGE_DELAY_MS: u32 = 4_000;

pub const ALARM_TIMES: u8 = 5;

pub fn door_accel_s(leaf: usize, opening: bool) -> f32 {
    match (leaf, opening) {
        (0, true) => OPEN_D1_ACCEL_S,
        (1, true) => OPEN_D2_ACCEL_S,
        (0, false) => CLOSE_D1_ACCEL_S,
        _ => CLOSE_D2_ACCEL_S,
    }
}

pub fn leaf_roll_deg(leaf: usize) -> f32 {
    if leaf == 0 {
        D1_NEW_ANGLE
    } else {
        D2_NEW_ANGLE
    }
}

pub fn unavailable(elapsed_since_start_ms: u32) -> bool {
    elapsed_since_start_ms < UNAVAILABLE_MS
}

pub fn kill_edge_active(closing: bool, elapsed_ms: u32, completed: bool) -> bool {
    closing && !completed && elapsed_ms >= KILL_EDGE_DELAY_MS
}

pub fn alarm_due(alarm_count: u8, elapsed_ms: u32) -> bool {
    alarm_count < ALARM_TIMES && elapsed_ms >= 500 + u32::from(alarm_count) * 2000
}

pub fn startup_ready(now_ms: u32, playing_since_ms: u32) -> bool {
    now_ms >= playing_since_ms.saturating_add(STARTUP_DELAY_MS)
}
