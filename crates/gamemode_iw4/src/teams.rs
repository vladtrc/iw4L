pub const SV_MAXCLIENTS_DVAR: &str = "sv_maxclients";

pub const SCR_TEAMBALANCE_DVAR: &str = "scr_teambalance";

pub const THERMAL_BEACON_FX: &str = "misc/thermal_beacon_inverted";

pub const THERMAL_BEACON_TAG: &str = "J_Spine4";

pub const TEAMS_INIT_WAIT: f32 = 0.15;

pub const SCORES_COLOR_SPECTATOR: &str = ".25 .25 .25";

pub const SCORES_COLOR_FREE: &str = ".76 .78 .10";

pub const TEAM_COLOR_MY_TEAM: &str = ".6 .8 .6";

pub const TEAM_COLOR_ENEMY_TEAM: &str = "1 .45 .5";

pub fn team_limit(maxclients: i32) -> i32 {
    maxclients / 2
}

pub fn get_join_team_permissions(team_count: i32, team_limit: i32) -> bool {
    team_count < team_limit
}

pub fn count_players_inc(allies: &mut u32, axis: &mut u32, pers_team: Option<&str>) {
    match pers_team {
        Some("allies") => *allies += 1,
        Some("axis") => *axis += 1,
        _ => {}
    }
}
