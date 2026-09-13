pub const HEALTH_OVERLAY_CUTOFF: f32 = 0.55;

pub const REGEN_RATE: f32 = 0.1;

pub const PLAYER_HEALTH_REGULAR_REGEN_DELAY_MS: i32 = 5_000;

pub const VERY_HURT_REGEN_EXTRA_MS: i32 = 3_000;

pub const BREATHING_HURT_HEALTH_FRAC: f32 = 0.35;

pub const BREATHING_BETTER_ALIAS: &str = "breathing_better";

pub const BREATHING_HURT_ALIAS: &str = "breathing_hurt";

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PlayerHealthRegenState {
    pub old_health: i32,

    pub hurt_time_ms: i32,

    pub very_hurt: bool,

    pub last_sound_time_recover_ms: i32,

    pub at_brink_of_death: bool,
}

impl PlayerHealthRegenState {
    pub const fn spawned(max_health: i32) -> Self {
        Self {
            old_health: max_health,
            hurt_time_ms: 0,
            very_hurt: false,
            last_sound_time_recover_ms: 0,
            at_brink_of_death: false,
        }
    }
}

impl Default for PlayerHealthRegenState {
    fn default() -> Self {
        Self::spawned(0)
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum HealthRegenSound {
    #[default]
    None,

    BreathingBetter,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct HealthRegenTick {
    pub health: Option<i32>,
    pub sound: HealthRegenSound,
}

fn gsc_int(value: f32) -> i32 {
    value as i32
}

pub fn set_normal_health(max_health: i32, frac: f32) -> i32 {
    gsc_int(frac * max_health as f32)
}

pub fn player_health_regen_tick(
    health: i32,
    max_health: i32,
    now_ms: i32,
    state: &mut PlayerHealthRegenState,
) -> HealthRegenTick {
    if max_health <= 0 {
        return HealthRegenTick {
            health: None,
            sound: HealthRegenSound::None,
        };
    }
    if health == max_health {
        state.old_health = max_health;
        state.very_hurt = false;
        state.at_brink_of_death = false;
        return HealthRegenTick {
            health: None,
            sound: HealthRegenSound::None,
        };
    }
    if health <= 0 {
        return HealthRegenTick {
            health: None,
            sound: HealthRegenSound::None,
        };
    }

    let ratio = health as f32 / max_health as f32;
    if ratio <= HEALTH_OVERLAY_CUTOFF {
        if !state.very_hurt {
            state.hurt_time_ms = now_ms;
        }
        state.very_hurt = true;
        state.at_brink_of_death = true;
    }

    if health >= state.old_health {
        if now_ms.saturating_sub(state.hurt_time_ms) < PLAYER_HEALTH_REGULAR_REGEN_DELAY_MS {
            return HealthRegenTick {
                health: None,
                sound: HealthRegenSound::None,
            };
        }

        let mut sound = HealthRegenSound::None;
        if now_ms.saturating_sub(state.last_sound_time_recover_ms)
            > PLAYER_HEALTH_REGULAR_REGEN_DELAY_MS
        {
            state.last_sound_time_recover_ms = now_ms;
            sound = HealthRegenSound::BreathingBetter;
        }

        let mut new_health = if state.very_hurt {
            let mut next = ratio;
            if now_ms > state.hurt_time_ms.saturating_add(VERY_HURT_REGEN_EXTRA_MS) {
                next += REGEN_RATE;
            }
            next
        } else {
            1.0
        };
        if new_health >= 1.0 {
            new_health = 1.0;
        }
        if new_health <= 0.0 {
            return HealthRegenTick {
                health: None,
                sound,
            };
        }
        let health_after = set_normal_health(max_health, new_health);
        state.old_health = health_after;
        HealthRegenTick {
            health: Some(health_after),
            sound,
        }
    } else {
        state.old_health = health;
        state.hurt_time_ms = now_ms;
        HealthRegenTick {
            health: None,
            sound: HealthRegenSound::None,
        }
    }
}
