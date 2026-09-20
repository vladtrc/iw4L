pub const SCRIPT_TICK_MS: u32 = 50;

pub const DESTROY_PERIOD_MS: u32 = 2_000;

pub const DROP_PERIOD_MS: u32 = 100;

pub const DROP_ORIGIN: [f32; 3] = [0.0, 0.0, 128.0];

pub const DROP_RADIUS: f32 = 181.0;

pub fn is_weapon_equipment(script_name: &str) -> bool {
    matches!(script_name, "claymore_mp" | "c4_mp")
}

pub fn sweep_active(elapsed_ms: u32, completed: bool) -> bool {
    !completed && elapsed_ms < super::radiation_doors::DOOR_TIME_MS
}

pub fn pulse_due(elapsed_ms: u32, period_ms: u32) -> bool {
    elapsed_ms >= period_ms && elapsed_ms % period_ms < SCRIPT_TICK_MS
}

pub fn destroy_due(elapsed_ms: u32, completed: bool) -> bool {
    sweep_active(elapsed_ms, completed) && pulse_due(elapsed_ms, DESTROY_PERIOD_MS)
}

pub fn drop_due(elapsed_ms: u32, completed: bool) -> bool {
    let _ = DROP_ORIGIN;
    let _ = DROP_RADIUS;
    sweep_active(elapsed_ms, completed) && pulse_due(elapsed_ms, DROP_PERIOD_MS)
}

pub fn first_drop_pulse(elapsed_ms: u32, completed: bool) -> bool {
    drop_due(elapsed_ms, completed) && elapsed_ms < DROP_PERIOD_MS + SCRIPT_TICK_MS
}

pub fn touching_door(point: [f32; 3], origin: [f32; 3], mins: [f32; 3], maxs: [f32; 3]) -> bool {
    (0..3).all(|i| point[i] >= origin[i] + mins[i] && point[i] <= origin[i] + maxs[i])
}
