pub const USE_START_SPAWNS_AT_START: bool = true;

pub const OBJECTIVE_POINTS_MOD: i32 = 1;

pub const GAME_STATE_PLAYING: &str = "playing";

pub fn matchmaking_game(online_game: bool, private_match: bool) -> bool {
    online_game && !private_match
}

pub fn max_allowed_team_kills(matchmaking: bool) -> i32 {
    if matchmaking { 2 } else { -1 }
}

pub const START_GAME_TYPE_THREADS: &[&str] = &[
    "maps/mp/gametypes/_persistence::init",
    "maps/mp/gametypes/_menus::init",
    "maps/mp/gametypes/_hud::init",
    "maps/mp/gametypes/_serversettings::init",
    "maps/mp/gametypes/_teams::init",
    "maps/mp/gametypes/_weapons::init",
    "maps/mp/gametypes/_killcam::init",
    "maps/mp/gametypes/_shellshock::init",
    "maps/mp/gametypes/_deathicons::init",
    "maps/mp/gametypes/_damagefeedback::init",
    "maps/mp/gametypes/_healthoverlay::init",
    "maps/mp/gametypes/_spectating::init",
    "maps/mp/gametypes/_objpoints::init",
    "maps/mp/gametypes/_gameobjects::init",
    "maps/mp/gametypes/_spawnlogic::init",
    "maps/mp/gametypes/_battlechatter_mp::init",
    "maps/mp/gametypes/_music_and_dialog::init",
    "maps/mp/_matchdata::init",
    "maps/mp/_awards::init",
    "maps/mp/_skill::init",
    "maps/mp/_areas::init",
    "maps/mp/killstreaks/_killstreaks::init",
    "maps/mp/perks/_perks::init",
    "maps/mp/_events::init",
    "maps/mp/_defcon::init",
    "maps/mp/gametypes/_hud_message::init",
];

pub const FRIENDICONS_INIT: &str = "maps/mp/gametypes/_friendicons::init";

pub const QUICKMESSAGES_INIT: &str = "maps/mp/gametypes/_quickmessages::init";

pub fn start_game_type_thread_count(team_based: bool, console: bool) -> usize {
    START_GAME_TYPE_THREADS.len() + usize::from(team_based) + usize::from(!console)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GameEventForfeit {
    Allies,
    Axis,
    FfaLastPlayer,
}

pub fn update_game_events_forfeit(
    matchmaking: bool,
    in_grace: bool,
    team_based: bool,
    game_playing: bool,
    allies_n: i32,
    axis_n: i32,
    max_player_count: i32,
) -> Option<GameEventForfeit> {
    if !matchmaking || in_grace {
        return None;
    }
    if team_based {
        if !game_playing {
            return None;
        }
        if allies_n < 1 && axis_n > 0 {
            return Some(GameEventForfeit::Allies);
        }
        if axis_n < 1 && allies_n > 0 {
            return Some(GameEventForfeit::Axis);
        }
        None
    } else if allies_n + axis_n == 1 && max_player_count > 1 {
        Some(GameEventForfeit::FfaLastPlayer)
    } else {
        None
    }
}

pub const FORFEIT_FFA_WAIT: f32 = 10.0;

pub const FORFEIT_DELAY: f32 = 20.0;

pub const FORFEIT_LOWER_Y: i32 = 100;
pub const FORFEIT_WARNING: &str = "forfeit_warning";
pub const STR_OPPONENT_FORFEITING_IN: &str = "MP_OPPONENT_FORFEITING_IN";
pub const STR_PLAYERS_FORFEITED: &str = "MP_PLAYERS_FORFEITED";

pub fn on_forfeit_ffa_wait(team_based: bool, player_count: u32) -> f32 {
    if !team_based && player_count > 1 {
        FORFEIT_FFA_WAIT
    } else {
        0.0
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ForfeitWinner {
    SkipInProgress,
    FfaFirstPlayer,
    Axis,
    Allies,
    Tie,
}

pub fn on_forfeit_winner(already_in_progress: bool, team: Option<&str>) -> ForfeitWinner {
    if already_in_progress {
        return ForfeitWinner::SkipInProgress;
    }
    match team {
        None => ForfeitWinner::FfaFirstPlayer,
        Some("allies") => ForfeitWinner::Axis,
        Some("axis") => ForfeitWinner::Allies,
        _ => ForfeitWinner::Tie,
    }
}
