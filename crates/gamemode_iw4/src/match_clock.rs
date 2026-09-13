use crate::sound_emit::{MatchEndingReason, MatchSoundEmit, MatchSoundNotify};

pub const COUNTDOWN_TICK_ALIAS: &str = "ui_mp_timer_countdown";

pub const CLOCK_OBJECT_CLASSNAME: &str = "script_origin";

pub const CLOCK_OBJECT_ORIGIN: [f32; 3] = [0.0, 0.0, 0.0];

pub const ENDING_SOON_SECONDS_MIN: i32 = 30;

pub const ENDING_SOON_SECONDS_MAX: i32 = 60;

pub const VERY_SOON_EVERY_SECOND_AT_OR_BELOW: i32 = 10;

pub const VERY_SOON_EVEN_SECONDS_AT_OR_BELOW: i32 = 30;

pub const TICK_PERIOD_SECONDS: f32 = 1.0;

pub const RESYNC_MIN_SECONDS: f32 = 0.05;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ClockTick {
    pub time_remaining_ms: i32,

    pub time_limit_minutes: f32,

    pub half_time: bool,

    pub timer_stopped: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ClockTickEmit {
    pub time_left_int: i32,

    pub ending_soon: bool,

    pub ending_very_soon: bool,

    pub countdown_tick: bool,

    pub clock_stops: bool,
}

impl ClockTickEmit {
    pub const MAX_EMITS: usize = 3;

    pub fn emits(&self, out: &mut [MatchSoundEmit; Self::MAX_EMITS]) -> usize {
        let mut n = 0;
        if self.ending_soon {
            out[n] =
                MatchSoundEmit::Notify(MatchSoundNotify::MatchEndingSoon(MatchEndingReason::Time));
            n += 1;
        }
        if self.ending_very_soon {
            out[n] = MatchSoundEmit::Notify(MatchSoundNotify::MatchEndingVerySoon);
            n += 1;
        }
        if self.countdown_tick {
            out[n] = MatchSoundEmit::EntitySound {
                alias: COUNTDOWN_TICK_ALIAS,
                origin: CLOCK_OBJECT_ORIGIN,
            };
            n += 1;
        }
        n
    }
}

fn gsc_int(value: f32) -> i32 {
    value as i32
}

pub fn time_left_int(time_remaining_ms: i32, time_limit_minutes: f32, half_time: bool) -> i32 {
    let time_left = time_remaining_ms as f32 / 1000.0;
    let mut time_left_int = gsc_int(time_left + 0.5);
    let half_limit_seconds = (time_limit_minutes * 60.0) * 0.5;
    if half_time && (time_left_int as f32) > half_limit_seconds {
        time_left_int -= gsc_int(half_limit_seconds);
    }
    time_left_int
}

pub const fn is_ending_soon_second(time_left_int: i32) -> bool {
    time_left_int >= ENDING_SOON_SECONDS_MIN && time_left_int <= ENDING_SOON_SECONDS_MAX
}

pub const fn is_ending_very_soon_second(time_left_int: i32) -> bool {
    time_left_int <= VERY_SOON_EVERY_SECOND_AT_OR_BELOW
        || (time_left_int <= VERY_SOON_EVEN_SECONDS_AT_OR_BELOW && time_left_int % 2 == 0)
}

pub fn clock_tick(tick: ClockTick) -> Option<ClockTickEmit> {
    if tick.timer_stopped || tick.time_limit_minutes == 0.0 {
        return None;
    }
    let time_left_int = time_left_int(
        tick.time_remaining_ms,
        tick.time_limit_minutes,
        tick.half_time,
    );
    let ending_very_soon = is_ending_very_soon_second(time_left_int);
    let clock_stops = ending_very_soon && time_left_int == 0;
    Some(ClockTickEmit {
        time_left_int,
        ending_soon: is_ending_soon_second(time_left_int),
        ending_very_soon,
        countdown_tick: ending_very_soon && !clock_stops,
        clock_stops,
    })
}

fn floor(value: f32) -> f32 {
    let truncated = value as i32 as f32;
    if value < truncated {
        truncated - 1.0
    } else {
        truncated
    }
}

pub fn resync_wait_seconds(time_remaining_ms: i32) -> Option<f32> {
    let time_left = time_remaining_ms as f32 / 1000.0;
    let fraction = time_left - floor(time_left);
    if fraction >= RESYNC_MIN_SECONDS {
        Some(fraction)
    } else {
        None
    }
}
