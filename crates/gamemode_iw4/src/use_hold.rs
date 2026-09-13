pub const USE_HOLD_TICK_MS: i32 = 50;

pub const USE_HOLD_WEAPON_WAIT_MAX_MS: i32 = 1500;

pub const OBJECTIVE_SCALER_IDENTITY: f32 = 1.0;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct UseHoldLoopState {
    pub cur_progress: i32,

    pub use_rate: f32,

    pub wait_for_weapon: bool,

    pub timed_out_ms: i32,

    pub in_use: bool,
}

impl UseHoldLoopState {
    pub const fn begin() -> Self {
        Self {
            cur_progress: 0,
            use_rate: 0.0,
            wait_for_weapon: true,
            timed_out_ms: 0,
            in_use: true,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct UseHoldLoopInput {
    pub alive: bool,

    pub touching: bool,

    pub use_pressed: bool,

    pub throwing_grenade: bool,

    pub melee_pressed: bool,

    pub weapon_ready: bool,

    pub use_time: i32,

    pub objective_scaler: f32,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum UseHoldLoopTick {
    Continue(UseHoldLoopState),
    Completed(UseHoldLoopState),
    Cancelled,
}

pub fn use_hold_loop_continues(state: &UseHoldLoopState, input: &UseHoldLoopInput) -> bool {
    input.alive
        && input.touching
        && input.use_pressed
        && !input.throwing_grenade
        && !input.melee_pressed
        && state.cur_progress < input.use_time
        && (state.use_rate != 0.0 || state.wait_for_weapon)
        && !(state.wait_for_weapon && state.timed_out_ms > USE_HOLD_WEAPON_WAIT_MAX_MS)
}

pub fn use_hold_loop_body(
    mut state: UseHoldLoopState,
    input: &UseHoldLoopInput,
) -> UseHoldLoopTick {
    state.timed_out_ms = state.timed_out_ms.saturating_add(USE_HOLD_TICK_MS);
    if input.weapon_ready {
        state.cur_progress = state
            .cur_progress
            .saturating_add((50.0 * state.use_rate) as i32);
        state.use_rate = 1.0 * input.objective_scaler;
        state.wait_for_weapon = false;
    } else {
        state.use_rate = 0.0;
    }
    if state.cur_progress >= input.use_time {
        state.in_use = false;
        return UseHoldLoopTick::Completed(state);
    }
    UseHoldLoopTick::Continue(state)
}

pub fn use_hold_loop_tick(state: UseHoldLoopState, input: &UseHoldLoopInput) -> UseHoldLoopTick {
    if !use_hold_loop_continues(&state, input) {
        return UseHoldLoopTick::Cancelled;
    }
    use_hold_loop_body(state, input)
}
