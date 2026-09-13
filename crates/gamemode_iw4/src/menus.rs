pub const MENU_NAMES_PC: &[(&str, &str)] = &[
    ("menu_team", "team_marinesopfor"),
    ("menu_class_allies", "class_marines"),
    ("menu_changeclass_allies", "changeclass_marines"),
    ("menu_initteam_allies", "initteam_marines"),
    ("menu_class_axis", "class_opfor"),
    ("menu_changeclass_axis", "changeclass_opfor"),
    ("menu_initteam_axis", "initteam_opfor"),
    ("menu_class", "class"),
    ("menu_changeclass", "changeclass"),
    ("menu_onemanarmy", "onemanarmy"),
    ("menu_controls", "ingame_controls"),
    ("menu_muteplayer", "muteplayer"),
];

pub const MENU_SCOREBOARD: &str = "scoreboard";
pub const MENU_HOST_ENDED_GAME: &str = "MP_HOST_ENDED_GAME";
pub const MENU_HOST_ENDGAME_RESPONSE: &str = "MP_HOST_ENDGAME_RESPONSE";

pub const WAITTILL_CONNECTED: &str = "connected";

pub const WAITTILL_MENURESPONSE: &str = "menuresponse";

pub const WAITTILL_BEGIN: &str = "begin";

pub fn menu_name_pc(key: &str) -> Option<&'static str> {
    MENU_NAMES_PC
        .iter()
        .find(|(k, _)| *k == key)
        .map(|(_, v)| *v)
}

pub fn is_options_menu(menu: &str, changeclass: &str, team: &str, controls: &str) -> bool {
    menu == changeclass || menu == team || menu == controls || menu.contains("pc_options")
}

pub fn begin_class_choice_menu_key(pers_team: &str) -> Option<&'static str> {
    match pers_team {
        "allies" => Some("menu_changeclass_allies"),
        "axis" => Some("menu_changeclass_axis"),
        _ => None,
    }
}

pub fn options_back_class_menu_key(pers_team: &str) -> Option<&'static str> {
    match pers_team {
        "allies" => Some("menu_class_allies"),
        "axis" => Some("menu_class_axis"),
        _ => None,
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TeamAssignment {
    Allies,
    Axis,
    CoinToss,
}

impl TeamAssignment {
    pub const fn as_session(self) -> Option<&'static str> {
        match self {
            Self::Allies => Some("allies"),
            Self::Axis => Some("axis"),
            Self::CoinToss => None,
        }
    }
}

pub fn sessionteam_sticky(sessionteam: &str, sessionstate: &str) -> bool {
    sessionteam != "none"
        && sessionteam != "spectator"
        && sessionstate != "playing"
        && sessionstate != "dead"
}

pub fn get_team_assignment(
    team_based: bool,
    sessionteam: &str,
    sessionstate: &str,
    allies_n: u32,
    axis_n: u32,
    allies_score: i32,
    axis_score: i32,
) -> TeamAssignment {
    if !team_based {
        return TeamAssignment::CoinToss;
    }
    if sessionteam_sticky(sessionteam, sessionstate) {
        return match sessionteam {
            "allies" => TeamAssignment::Allies,
            "axis" => TeamAssignment::Axis,
            _ => TeamAssignment::CoinToss,
        };
    }
    team_assignment_from_counts(allies_n, axis_n, allies_score, axis_score)
}

pub fn team_assignment_from_counts(
    allies_n: u32,
    axis_n: u32,
    allies_score: i32,
    axis_score: i32,
) -> TeamAssignment {
    if allies_n == axis_n {
        if allies_score == axis_score {
            TeamAssignment::CoinToss
        } else if allies_score < axis_score {
            TeamAssignment::Allies
        } else {
            TeamAssignment::Axis
        }
    } else if allies_n < axis_n {
        TeamAssignment::Allies
    } else {
        TeamAssignment::Axis
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SessionTeamWrite {
    Skip,
    Allies,
    Axis,
    Spectator,
    None,
}

pub fn add_to_team_sessionteam(
    matchmaking: bool,
    is_bot: bool,
    team_based: bool,
    team: &str,
) -> SessionTeamWrite {
    if matchmaking && !is_bot {
        return SessionTeamWrite::Skip;
    }
    if team_based {
        return match team {
            "allies" => SessionTeamWrite::Allies,
            "axis" => SessionTeamWrite::Axis,
            "spectator" => SessionTeamWrite::Spectator,
            _ => SessionTeamWrite::None,
        };
    }
    if team == "spectator" {
        SessionTeamWrite::Spectator
    } else {
        SessionTeamWrite::None
    }
}

pub fn add_to_team_counts(game_state: &str) -> bool {
    game_state != "postgame"
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum JoinedNotify {
    Spectators,
    Team,
}

pub fn add_to_team_notify(team: &str) -> JoinedNotify {
    if team == "spectator" {
        JoinedNotify::Spectators
    } else {
        JoinedNotify::Team
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MenuAlliesAxis {
    ReopenTeamMenu,
    BeginClassChoice {
        add_team: bool,
        suicide_if_playing: bool,
        clear_has_spawned: bool,
    },
}

pub fn menu_allies_or_axis(
    pers_team: &str,
    want: &str,
    team_based: bool,
    join_allowed: bool,
    in_grace: bool,
    has_done_combat: bool,
    sessionstate_playing: bool,
) -> MenuAlliesAxis {
    if pers_team == want {
        return MenuAlliesAxis::BeginClassChoice {
            add_team: false,
            suicide_if_playing: false,
            clear_has_spawned: false,
        };
    }
    if team_based && !join_allowed {
        return MenuAlliesAxis::ReopenTeamMenu;
    }
    MenuAlliesAxis::BeginClassChoice {
        add_team: true,
        suicide_if_playing: sessionstate_playing,
        clear_has_spawned: in_grace && !has_done_combat,
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MenuClass {
    Ignored,
    BeginClassChoice,
    Unchanged,
    StoredPostgame,
    GiveLoadout,
    ChangeClassPrint,
    SpawnClient,
    StoreSpectate,
}

pub fn menu_class(
    pers_team: Option<&str>,
    class: &str,
    primary: &str,
    prev_class: Option<&str>,
    prev_primary: Option<&str>,
    sessionstate_playing: bool,
    game_state: &str,
    in_grace: bool,
    has_done_combat: bool,
    in_killcam: bool,
) -> MenuClass {
    match pers_team {
        Some("allies" | "axis") => {}
        _ => return MenuClass::Ignored,
    }
    if class == "restricted" {
        return MenuClass::BeginClassChoice;
    }
    if prev_class == Some(class) && prev_primary == Some(primary) {
        return MenuClass::Unchanged;
    }
    if sessionstate_playing {
        if game_state == "postgame" {
            return MenuClass::StoredPostgame;
        }
        if in_grace && !has_done_combat {
            MenuClass::GiveLoadout
        } else {
            MenuClass::ChangeClassPrint
        }
    } else if game_state == "postgame" {
        MenuClass::StoredPostgame
    } else if game_state == "playing" && !in_killcam {
        MenuClass::SpawnClient
    } else {
        MenuClass::StoreSpectate
    }
}
