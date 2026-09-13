use crate::camtime::SERVER_FRAME_SECONDS;

pub const DOUBLE_TAP_LIMIT_SECONDS: f32 = 0.5;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CancelTick {
    Waiting,

    Fired,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum TapState {
    Idle,

    Pressing,

    Gap,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DoubleTap {
    state: TapState,
    button_time: f32,
    fired: bool,
}

impl Default for DoubleTap {
    fn default() -> Self {
        Self::new()
    }
}

impl DoubleTap {
    pub const fn new() -> Self {
        Self {
            state: TapState::Idle,
            button_time: 0.0,
            fired: false,
        }
    }

    pub const fn cancelled(&self) -> bool {
        self.fired
    }

    pub fn tick(&mut self, pressed: bool) -> CancelTick {
        if self.fired {
            return CancelTick::Waiting;
        }
        match self.state {
            TapState::Idle => {
                if pressed {
                    self.state = TapState::Pressing;
                    self.button_time = SERVER_FRAME_SECONDS;
                }
                CancelTick::Waiting
            }
            TapState::Pressing => {
                if pressed {
                    self.button_time += SERVER_FRAME_SECONDS;
                    return CancelTick::Waiting;
                }

                if self.button_time >= DOUBLE_TAP_LIMIT_SECONDS {
                    self.state = TapState::Idle;
                    self.button_time = 0.0;
                    return CancelTick::Waiting;
                }
                self.state = TapState::Gap;
                self.button_time = SERVER_FRAME_SECONDS;
                CancelTick::Waiting
            }
            TapState::Gap => {
                if !pressed && self.button_time < DOUBLE_TAP_LIMIT_SECONDS {
                    self.button_time += SERVER_FRAME_SECONDS;
                    return CancelTick::Waiting;
                }
                if self.button_time >= DOUBLE_TAP_LIMIT_SECONDS {
                    self.state = TapState::Idle;
                    self.button_time = 0.0;
                    return CancelTick::Waiting;
                }

                self.fired = true;
                CancelTick::Fired
            }
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum SkipState {
    Drain,

    WaitEdge,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SkipEdge {
    state: SkipState,
    fired: bool,
}

impl Default for SkipEdge {
    fn default() -> Self {
        Self::new()
    }
}

impl SkipEdge {
    pub const fn new() -> Self {
        Self {
            state: SkipState::Drain,
            fired: false,
        }
    }

    pub const fn fired(&self) -> bool {
        self.fired
    }

    pub fn tick(&mut self, pressed: bool) -> CancelTick {
        if self.fired {
            return CancelTick::Waiting;
        }
        match self.state {
            SkipState::Drain => {
                if pressed {
                    return CancelTick::Waiting;
                }

                self.state = SkipState::WaitEdge;
                CancelTick::Waiting
            }
            SkipState::WaitEdge => {
                if pressed {
                    self.fired = true;
                    return CancelTick::Fired;
                }
                CancelTick::Waiting
            }
        }
    }
}
