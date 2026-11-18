const LINES: &[(&str, &str)] = &[
    ("acheive_bomb", "sd_bomb_taken_taken"),
    ("bomb_defused", "sd_bomb_defused"),
    ("bomb_planted", "sd_bomb_planted"),
    ("bomb_taken", "sd_bomb_drop"),
    ("boost", "generic_boost"),
    ("capture_a", "dom_capture_a"),
    ("capture_b", "dom_capture_c"),
    ("capture_c", "dom_capture_b"),
    ("capturing_a", "dom_capturing_a"),
    ("capturing_b", "dom_capturing_b"),
    ("capturing_c", "dom_capturing_c"),
    ("captureflag", "ctf_start"),
    ("demolition", "demo_start"),
    ("dest_sentrygun", "dest_sentry"),
    ("domination", "dom_start"),
    ("freeforall", "ffa_start"),
    ("headquarters", "hq_start"),
    ("hq_captured", "hq_capture"),
    ("hq_destroyed", "hq_defend"),
    ("losing_a", "dom_losing_a"),
    ("losing_b", "dom_losing_b"),
    ("losing_c", "dom_losing_c"),
    ("lost_a", "dom_lost_a"),
    ("lost_b", "dom_lost_b"),
    ("lost_c", "dom_lost_c"),
    ("obj_defend", "defend_start"),
    ("obj_destroy", "destroy_start"),
    ("positions_lock", "dom_lock_wetake"),
    ("sabotage", "sab_start"),
    ("searchdestroy", "sd_start"),
    ("secure_a", "dom_secured_a"),
    ("secure_b", "dom_secured_b"),
    ("secure_c", "dom_secured_c"),
    ("securing_a", "dom_securing_a"),
    ("securing_b", "dom_securing_b"),
    ("securing_c", "dom_securing_c"),
    ("switching", "switchingsides"),
    ("take_positions", "dom_lock_theytake"),
    ("tm_death", "tdm_start"),
];

pub fn route(voice: &str, line: &str) -> String {
    let line = LINES
        .iter()
        .find(|(from, _)| *from == line)
        .map_or(line, |(_, to)| *to);
    format!("{voice}{line}")
}
