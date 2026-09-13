use crate::output::{Alias, Team, VOICE_INFIX};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DialogSet {
    Always,

    IfUndefined,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Recipe {
    Music {
        key: &'static str,
        alias: Alias,
    },

    Voice {
        team: Team,
        infix: &'static str,
    },

    SuspenseInit,

    SuspenseTrack {
        alias: Alias,
    },

    Dialog {
        key: &'static str,
        line: &'static str,
        set: DialogSet,
    },
}

pub const INIT: &[Recipe] = &[
    music("spawn_allies", Team::Allies, "spawn_music"),
    music("defeat_allies", Team::Allies, "defeat_music"),
    music("victory_allies", Team::Allies, "victory_music"),
    music("winning_allies", Team::Allies, "winning_music"),
    music("losing_allies", Team::Allies, "losing_music"),
    Recipe::Voice {
        team: Team::Allies,
        infix: VOICE_INFIX,
    },
    music("spawn_axis", Team::Axis, "spawn_music"),
    music("defeat_axis", Team::Axis, "defeat_music"),
    music("victory_axis", Team::Axis, "victory_music"),
    music("winning_axis", Team::Axis, "winning_music"),
    music("losing_axis", Team::Axis, "losing_music"),
    Recipe::Voice {
        team: Team::Axis,
        infix: VOICE_INFIX,
    },
    literal("defeat", "mp_defeat"),
    literal("victory_spectator", "mp_defeat"),
    literal("winning_time", "mp_time_running_out_winning"),
    literal("losing_time", "mp_time_running_out_losing"),
    literal("winning_score", "mp_time_running_out_winning"),
    literal("losing_score", "mp_time_running_out_losing"),
    literal("victory_tie", "mp_defeat"),
    Recipe::SuspenseInit,
    track("mp_suspense_01"),
    track("mp_suspense_02"),
    track("mp_suspense_03"),
    track("mp_suspense_04"),
    track("mp_suspense_05"),
    track("mp_suspense_06"),
    dialog("mission_success", "mission_success"),
    dialog("mission_failure", "mission_fail"),
    dialog("mission_draw", "draw"),
    dialog("round_success", "encourage_win"),
    dialog("round_failure", "encourage_lost"),
    dialog("round_draw", "draw"),
    dialog("timesup", "timesup"),
    dialog("winning_time", "winning"),
    dialog("losing_time", "losing"),
    dialog("winning_score", "winning_fight"),
    dialog("losing_score", "losing_fight"),
    dialog("lead_lost", "lead_lost"),
    dialog("lead_tied", "tied"),
    dialog("lead_taken", "lead_taken"),
    dialog("last_alive", "lastalive"),
    dialog("boost", "boost"),
    Recipe::Dialog {
        key: "offense_obj",
        line: "boost",
        set: DialogSet::IfUndefined,
    },
    Recipe::Dialog {
        key: "defense_obj",
        line: "boost",
        set: DialogSet::IfUndefined,
    },
    dialog("hardcore", "hardcore"),
    dialog("highspeed", "highspeed"),
    dialog("tactical", "tactical"),
    dialog("challenge", "challengecomplete"),
    dialog("promotion", "promotion"),
    dialog("bomb_taken", "acheive_bomb"),
    dialog("bomb_lost", "bomb_taken"),
    dialog("bomb_defused", "bomb_defused"),
    dialog("bomb_planted", "bomb_planted"),
    dialog("obj_taken", "securedobj"),
    dialog("obj_lost", "lostobj"),
    dialog("obj_defend", "obj_defend"),
    dialog("obj_destroy", "obj_destroy"),
    dialog("obj_capture", "capture_obj"),
    dialog("objs_capture", "capture_objs"),
    dialog("hq_located", "hq_located"),
    dialog("hq_enemy_captured", "hq_captured"),
    dialog("hq_enemy_destroyed", "hq_destroyed"),
    dialog("hq_secured", "hq_secured"),
    dialog("hq_offline", "hq_offline"),
    dialog("hq_online", "hq_online"),
    dialog("move_to_new", "new_positions"),
    dialog("push_forward", "pushforward"),
    dialog("attack", "attack"),
    dialog("defend", "defend"),
    dialog("offense", "offense"),
    dialog("defense", "defense"),
    dialog("halftime", "halftime"),
    dialog("overtime", "overtime"),
    dialog("side_switch", "switching"),
    dialog("flag_taken", "ourflag"),
    dialog("flag_dropped", "ourflag_drop"),
    dialog("flag_returned", "ourflag_return"),
    dialog("flag_captured", "ourflag_capt"),
    dialog("flag_getback", "getback_ourflag"),
    dialog("enemy_flag_bringhome", "enemyflag_tobase"),
    dialog("enemy_flag_taken", "enemyflag"),
    dialog("enemy_flag_dropped", "enemyflag_drop"),
    dialog("enemy_flag_returned", "enemyflag_return"),
    dialog("enemy_flag_captured", "enemyflag_capt"),
    dialog("capturing_a", "capturing_a"),
    dialog("capturing_b", "capturing_b"),
    dialog("capturing_c", "capturing_c"),
    dialog("captured_a", "capture_a"),
    dialog("captured_b", "capture_c"),
    dialog("captured_c", "capture_b"),
    dialog("securing_a", "securing_a"),
    dialog("securing_b", "securing_b"),
    dialog("securing_c", "securing_c"),
    dialog("secured_a", "secure_a"),
    dialog("secured_b", "secure_b"),
    dialog("secured_c", "secure_c"),
    dialog("losing_a", "losing_a"),
    dialog("losing_b", "losing_b"),
    dialog("losing_c", "losing_c"),
    dialog("lost_a", "lost_a"),
    dialog("lost_b", "lost_b"),
    dialog("lost_c", "lost_c"),
    dialog("enemy_taking_a", "enemy_take_a"),
    dialog("enemy_taking_b", "enemy_take_b"),
    dialog("enemy_taking_c", "enemy_take_c"),
    dialog("enemy_has_a", "enemy_has_a"),
    dialog("enemy_has_b", "enemy_has_b"),
    dialog("enemy_has_c", "enemy_has_c"),
    dialog("lost_all", "take_positions"),
    dialog("secure_all", "positions_lock"),
    dialog("destroy_sentry", "dest_sentrygun"),
    literal("nuke_music", "nuke_music"),
    dialog("sentry_gone", "sentry_gone"),
    dialog("sentry_destroyed", "sentry_gone"),
    dialog("ti_gone", "ti_cancelled"),
    dialog("ti_destroyed", "ti_blocked"),
];

const fn music(key: &'static str, team: Team, suffix: &'static str) -> Recipe {
    Recipe::Music {
        key,
        alias: Alias::TeamMusic { team, suffix },
    }
}

const fn literal(key: &'static str, value: &'static str) -> Recipe {
    Recipe::Music {
        key,
        alias: Alias::Literal(value),
    }
}

const fn track(value: &'static str) -> Recipe {
    Recipe::SuspenseTrack {
        alias: Alias::Literal(value),
    }
}

const fn dialog(key: &'static str, line: &'static str) -> Recipe {
    Recipe::Dialog {
        key,
        line,
        set: DialogSet::Always,
    }
}

pub fn music_alias(key: &str) -> Option<Alias> {
    INIT.iter().find_map(|r| match r {
        Recipe::Music { key: k, alias } if *k == key => Some(*alias),
        _ => None,
    })
}

pub fn team_music_alias(stem: &str, team: Team) -> Option<Alias> {
    INIT.iter().find_map(|r| match r {
        Recipe::Music {
            key,
            alias: alias @ Alias::TeamMusic { team: t, .. },
        } if *t == team && key.strip_prefix(stem) == Some(team.as_str()) => Some(*alias),
        _ => None,
    })
}

pub fn dialog_line(key: &str) -> Option<&'static str> {
    INIT.iter().find_map(|r| match r {
        Recipe::Dialog { key: k, line, .. } if *k == key => Some(*line),
        _ => None,
    })
}

pub fn suspense_len() -> usize {
    INIT.iter()
        .filter(|r| matches!(r, Recipe::SuspenseTrack { .. }))
        .count()
}

pub fn suspense_track(index: usize) -> Option<Alias> {
    INIT.iter()
        .filter_map(|r| match r {
            Recipe::SuspenseTrack { alias } => Some(*alias),
            _ => None,
        })
        .nth(index)
}
