#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct TeamCounts {
    pub allies: i32,
    pub axis: i32,
    pub spectator: i32,
}

impl TeamCounts {
    pub fn get(self, team: &str) -> i32 {
        match team {
            "allies" => self.allies,
            "axis" => self.axis,
            "spectator" => self.spectator,
            _ => 0,
        }
    }

    fn slot_mut(&mut self, team: &str) -> Option<&mut i32> {
        match team {
            "allies" => Some(&mut self.allies),
            "axis" => Some(&mut self.axis),
            "spectator" => Some(&mut self.spectator),
            _ => None,
        }
    }
}

pub fn add_to_team_count(counts: &mut TeamCounts, team: &str) {
    if let Some(slot) = counts.slot_mut(team) {
        *slot += 1;
    }
}

pub fn remove_from_team_count(counts: &mut TeamCounts, team: &str) {
    if let Some(slot) = counts.slot_mut(team) {
        *slot -= 1;
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct AliveCounts {
    pub allies: i32,
    pub axis: i32,
    pub spectator: i32,
    pub has_spawned_allies: i32,
    pub has_spawned_axis: i32,
    pub has_spawned_spectator: i32,
    pub max_player_count: i32,
}

impl AliveCounts {
    fn alive_mut(&mut self, team: &str) -> Option<&mut i32> {
        match team {
            "allies" => Some(&mut self.allies),
            "axis" => Some(&mut self.axis),
            "spectator" => Some(&mut self.spectator),
            _ => None,
        }
    }

    fn spawned_mut(&mut self, team: &str) -> Option<&mut i32> {
        match team {
            "allies" => Some(&mut self.has_spawned_allies),
            "axis" => Some(&mut self.has_spawned_axis),
            "spectator" => Some(&mut self.has_spawned_spectator),
            _ => None,
        }
    }
}

pub fn add_to_alive_count(counts: &mut AliveCounts, team: &str) {
    if let Some(slot) = counts.alive_mut(team) {
        *slot += 1;
    }
    if let Some(slot) = counts.spawned_mut(team) {
        *slot += 1;
    }
    let sum = counts.allies + counts.axis;
    if sum > counts.max_player_count {
        counts.max_player_count = sum;
    }
}

pub fn remove_from_alive_count(counts: &mut AliveCounts, team: &str) {
    if let Some(slot) = counts.alive_mut(team) {
        *slot -= 1;
    }
}

pub fn switching_teams_clears_lives() -> i32 {
    0
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct LivesCounts {
    pub allies: i32,
    pub axis: i32,
}

impl LivesCounts {
    fn slot_mut(&mut self, team: &str) -> Option<&mut i32> {
        match team {
            "allies" => Some(&mut self.allies),
            "axis" => Some(&mut self.axis),
            _ => None,
        }
    }
}

pub fn add_to_lives_count(counts: &mut LivesCounts, team: &str, pers_lives: i32) {
    if let Some(slot) = counts.slot_mut(team) {
        *slot += pers_lives;
    }
}

pub fn remove_from_lives_count(counts: &mut LivesCounts, team: &str) {
    if let Some(slot) = counts.slot_mut(team) {
        *slot -= 1;
        if *slot < 0 {
            *slot = 0;
        }
    }
}

pub fn remove_all_from_lives_count(counts: &mut LivesCounts, team: &str, pers_lives: i32) {
    if let Some(slot) = counts.slot_mut(team) {
        *slot -= pers_lives;
        if *slot < 0 {
            *slot = 0;
        }
    }
}

pub const SPECTATOR_SPAWN_Z: f32 = 60.0;

pub const HUD_STATUS_CONNECTING: &str = "hud_status_connecting";

pub const MP_CONNECTED: &str = "MP_CONNECTED";

pub const MATCHDATA_CLIENTID_CAP: i32 = 24;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SpawnClientStart {
    SpectatorLift,
    AlreadyWaiting,
    WaitAndSpawn,
}

pub fn spawn_client_start(may_spawn: bool, waiting_to_spawn: bool) -> SpawnClientStart {
    if !may_spawn {
        SpawnClientStart::SpectatorLift
    } else if waiting_to_spawn {
        SpawnClientStart::AlreadyWaiting
    } else {
        SpawnClientStart::WaitAndSpawn
    }
}

pub fn first_connect_clientid(pers_clientid: Option<i32>, game_clientid: i32) -> (i32, i32, bool) {
    match pers_clientid {
        Some(id) => (id, game_clientid, false),
        None => (game_clientid, game_clientid + 1, true),
    }
}

pub fn matchdata_connect_row(matchmaking: bool, game_clientid: i32) -> bool {
    matchmaking && game_clientid <= MATCHDATA_CLIENTID_CAP
}

pub fn game_has_started(
    team_based: bool,
    has_spawned_axis: bool,
    has_spawned_allies: bool,
    max_player_count: i32,
) -> bool {
    if team_based {
        has_spawned_axis && has_spawned_allies
    } else {
        max_player_count > 1
    }
}

pub fn is_round_based(team_based: bool, winlimit: i32, roundlimit: i32) -> bool {
    if !team_based {
        return false;
    }
    winlimit != 1 && roundlimit != 1
}

pub fn team_kill_delay(max_allowed: i32, teamkills: i32, teamkillspawndelay: i32) -> i32 {
    if max_allowed < 0 || teamkills <= max_allowed {
        0
    } else {
        teamkillspawndelay * (teamkills - max_allowed)
    }
}

pub fn may_spawn(
    num_lives: i32,
    disable_spawning: Option<bool>,
    team_kill_punish: bool,
    pers_lives: i32,
    game_started: bool,
    in_grace: bool,
    has_spawned: bool,
) -> bool {
    if num_lives == 0 && disable_spawning.is_none() {
        return true;
    }
    if disable_spawning == Some(true) {
        return false;
    }
    if team_kill_punish {
        return false;
    }
    if pers_lives == 0 && game_started {
        return false;
    }
    if game_started && !in_grace && !has_spawned {
        return false;
    }
    true
}

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

pub fn wait_and_spawn_needs_use_button(
    forcerespawn: i32,
    has_spawned: bool,
    wave_based: bool,
    want_safe_spawn: bool,
) -> bool {
    forcerespawn == 0 && has_spawned && !wave_based && !want_safe_spawn
}

pub const REMOVE_SPAWN_MESSAGE_DELAY: f32 = 3.0;

pub const WAIT_RESPAWN_POLL: f32 = 0.05;

pub const FORCE_SPAWN_NOTIFY: &str = "force_spawn";

pub const STOPPED_USING_REMOTE: &str = "stopped_using_remote";

pub const ATTEMPTED_SPAWN: &str = "attempted_spawn";

pub const SPAWNED_NOTIFY: &str = "spawned";
pub const END_RESPAWN_NOTIFY: &str = "end_respawn";

pub const HUD_STATUS_DEAD: &str = "hud_status_dead";

pub const MP_GLOBAL_INTERMISSION: &str = "mp_global_intermission";

pub const PREDICT_LEAD_S: f32 = 1.0;

pub const PREDICT_LOOP_COUNT: i32 = 30;
pub const PREDICT_LOOP_WAIT: f32 = 0.4;

pub fn spectator_statusicon(pers_team_spectator: bool) -> &'static str {
    if pers_team_spectator {
        ""
    } else {
        HUD_STATUS_DEAD
    }
}

pub const WAVE_SPAWN_STAGGER_MS: f32 = 50.0;

pub fn time_until_wave_spawn(
    has_spawned: bool,
    now_ms: i32,
    minimum_wait_s: f32,
    last_wave_ms: i32,
    wave_delay_s: f32,
    respawn_timer_start_ms: Option<i32>,
    wave_spawn_index: Option<i32>,
) -> Option<f32> {
    if !has_spawned {
        return Some(0.0);
    }
    if wave_delay_s <= 0.0 {
        return None;
    }
    let wave_delay_ms = wave_delay_s * 1000.0;
    let earliest = now_ms as f32 + minimum_wait_s * 1000.0;
    let num_waves = libm::ceilf((earliest - last_wave_ms as f32) / wave_delay_ms);
    let mut time_of_spawn = last_wave_ms as f32 + num_waves * wave_delay_ms;
    if let Some(start) = respawn_timer_start_ms {
        if start < last_wave_ms {
            return Some(0.0);
        }
    }
    if let Some(idx) = wave_spawn_index {
        time_of_spawn += WAVE_SPAWN_STAGGER_MS * idx as f32;
    }
    Some((time_of_spawn - now_ms as f32) / 1000.0)
}

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

pub fn hit_win_limit(winlimit: i32, team_based: bool, allies_won: i32, axis_won: i32) -> bool {
    if winlimit <= 0 {
        return false;
    }
    if !team_based {
        return true;
    }
    allies_won >= winlimit || axis_won >= winlimit
}

pub fn hit_round_limit(roundlimit: i32, rounds_played: i32) -> bool {
    if roundlimit <= 0 {
        return false;
    }
    rounds_played >= roundlimit
}

pub fn was_only_round(
    team_based: bool,
    winlimit: i32,
    roundlimit: i32,
    allies_won: i32,
    axis_won: i32,
) -> bool {
    if !team_based {
        return true;
    }
    if winlimit == 1 && hit_win_limit(winlimit, true, allies_won, axis_won) {
        return true;
    }
    roundlimit == 1
}

pub fn was_last_round(
    forced_end: bool,
    team_based: bool,
    roundlimit: i32,
    rounds_played: i32,
    winlimit: i32,
    allies_won: i32,
    axis_won: i32,
) -> bool {
    if forced_end || !team_based {
        return true;
    }
    hit_round_limit(roundlimit, rounds_played)
        || hit_win_limit(winlimit, true, allies_won, axis_won)
}

pub const SESSIONSTATE_PLAYING: &str = "playing";

pub const SPAWN_CG_FOV: &str = "65";

pub const HIDE_PERKS_AFTER_S: f32 = 5.0;

pub const SHOW_PERKS_ON_SPAWN_DEFAULT: i32 = 1;

pub const WAS_ALIVE_AT_MATCH_START_S: f32 = 20.0;

pub const SPAWN_MOVE_SPEED_SCALER: i32 = 1;

pub const TACTICAL_SPAWN_ALIAS: &str = "tactical_spawn";

pub const SPAWNED_PLAYER_NOTIFY: &str = "spawned_player";
pub const PLAYER_SPAWNED_NOTIFY: &str = "player_spawned";

pub const KILLEDBY_CARD_HIDE: &str = "killedby_card_hide";

pub const PERK_HIDE_MENU: &str = "perk_hide";
pub const PERKS_HIDDEN_NOTIFY: &str = "perks_hidden";

pub fn spawn_player_add_lives(pers_lives: i32, num_lives: i32) -> bool {
    pers_lives == num_lives
}

pub fn spawn_player_dec_lives(pers_lives: i32) -> i32 {
    if pers_lives != 0 {
        pers_lives - 1
    } else {
        pers_lives
    }
}

pub fn spawn_player_remove_lives(
    had_spawned: bool,
    game_started: bool,
    in_grace: bool,
    has_done_combat: bool,
) -> bool {
    !had_spawned || game_started || (game_started && in_grace && has_done_combat)
}

pub fn was_alive_at_match_start_window_s(time_limit_min: f32) -> f32 {
    let mut acceptable = WAS_ALIVE_AT_MATCH_START_S;
    if time_limit_min > 0.0 {
        let quarter = time_limit_min * 60.0 / 4.0;
        if acceptable < quarter {
            acceptable = quarter;
        }
    }
    acceptable
}

pub fn get_spawn_origin(
    origin: [f32; 3],
    origin_telefrag: bool,
    alternates: &[[f32; 3]],
    alternate_telefrag: &[bool],
) -> [f32; 3] {
    if !origin_telefrag {
        return origin;
    }
    if alternates.is_empty() {
        return origin;
    }
    let n = alternates.len().min(alternate_telefrag.len());
    for i in 0..n {
        if !alternate_telefrag[i] {
            return alternates[i];
        }
    }
    origin
}

pub const TI_CARE_PACKAGE_CLEAR_DIST: f32 = 64.0;

pub fn ti_blocked_by_care_package(spawn_pos: [f32; 3], packages: &[[f32; 3]]) -> bool {
    packages.iter().any(|p| {
        let dx = p[0] - spawn_pos[0];
        let dy = p[1] - spawn_pos[1];
        let dz = p[2] - spawn_pos[2];
        libm::sqrtf(dx * dx + dy * dy + dz * dz) <= TI_CARE_PACKAGE_CLEAR_DIST
    })
}

pub fn get_score_limit(round_based: bool, roundlimit: i32, winlimit: i32, scorelimit: i32) -> i32 {
    if round_based {
        if roundlimit != 0 {
            roundlimit
        } else {
            winlimit
        }
    } else {
        scorelimit
    }
}

pub fn hit_score_limit(
    objective_based: bool,
    scorelimit: i32,
    team_based: bool,
    allies: i32,
    axis: i32,
    any_player: bool,
) -> bool {
    if objective_based || scorelimit <= 0 {
        return false;
    }
    if team_based {
        allies >= scorelimit || axis >= scorelimit
    } else {
        any_player
    }
}
