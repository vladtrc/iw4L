pub const MP_CONNECTED: &str = "MP_CONNECTED";

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum TimeUntilSpawn {
    Seconds(f32),
    WaveBased,
}

pub fn time_until_spawn(
    in_grace: bool,
    has_spawned: bool,
    game_ended: bool,
    on_respawn_delay: Option<f32>,
    playerrespawndelay: f32,
    include_teamkill_delay: bool,
    team_kill_punish: bool,
    teamkill_delay: f32,
    elapsed_since_timer_s: Option<f32>,
    tactical_insertion: bool,
    ti_spawn_delay: f32,
    wave_based: bool,
) -> TimeUntilSpawn {
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
