pub const MP_CONNECTED: &str = "MP_CONNECTED";

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum TimeUntilSpawn {
    Seconds(f32),
    WaveBased,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RespawnStatus {
    pub in_grace: bool,
    pub has_spawned: bool,
    pub game_ended: bool,
    pub on_respawn_delay: Option<f32>,
    pub team_kill_punish: bool,
    pub elapsed_since_timer_s: Option<f32>,
    pub tactical_insertion: bool,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RespawnRules {
    pub player_respawn_delay: f32,
    pub teamkill_delay: f32,
    pub tactical_insertion_delay: f32,
    pub wave_based: bool,
}

pub fn time_until_spawn(
    status: RespawnStatus,
    rules: RespawnRules,
    include_teamkill_delay: bool,
) -> TimeUntilSpawn {
    let RespawnStatus {
        in_grace,
        has_spawned,
        game_ended,
        on_respawn_delay,
        team_kill_punish,
        elapsed_since_timer_s,
        tactical_insertion,
    } = status;
    let RespawnRules {
        player_respawn_delay: playerrespawndelay,
        teamkill_delay,
        tactical_insertion_delay: ti_spawn_delay,
        wave_based,
    } = rules;
    if (in_grace && !has_spawned) || game_ended {
        return TimeUntilSpawn::Seconds(0.0);
    }
    let mut delay = 0.0;
    if has_spawned {
        delay = on_respawn_delay.unwrap_or(playerrespawndelay);
        if include_teamkill_delay && team_kill_punish {
            delay += teamkill_delay;
        }
        if let Some(passed) = elapsed_since_timer_s {
            delay -= passed;
            if delay < 0.0 {
                delay = 0.0;
            }
        }
        if tactical_insertion {
            delay += ti_spawn_delay;
        }
    }
    if wave_based {
        TimeUntilSpawn::WaveBased
    } else {
        TimeUntilSpawn::Seconds(delay)
    }
}

pub const HUD_STATUS_DEAD: &str = "hud_status_dead";

pub const MP_GLOBAL_INTERMISSION: &str = "mp_global_intermission";

pub fn is_last_round(
    team_based: bool,
    roundlimit: i32,
    rounds_played: i32,
    winlimit: i32,
    allies_won: i32,
    axis_won: i32,
) -> bool {
    if !team_based {
        return true;
    }
    if roundlimit > 1 && rounds_played >= roundlimit - 1 {
        return true;
    }
    if winlimit > 1 && allies_won >= winlimit - 1 && axis_won >= winlimit - 1 {
        return true;
    }
    false
}

pub const KILLEDBY_CARD_HIDE: &str = "killedby_card_hide";
