pub const TARGETNAME: &str = "destructable";

pub const DEFAULT_ACCUMULATE: i32 = 40;

pub const DEFAULT_THRESHOLD: i32 = 0;

pub const SPAWN_TDM: &str = "mp_tdm_spawn";

pub const SPAWN_DM: &str = "mp_dm_spawn";

pub fn is_destructable_targetname(targetname: &str) -> bool {
    targetname.eq_ignore_ascii_case(TARGETNAME)
}

pub fn init_keeps_ents(scr_destructables: &str) -> bool {
    scr_destructables != "0"
}

pub fn accumulate_of(script_accumulate: Option<i32>) -> i32 {
    script_accumulate.unwrap_or(DEFAULT_ACCUMULATE)
}

pub fn threshold_of(script_threshold: Option<i32>) -> i32 {
    script_threshold.unwrap_or(DEFAULT_THRESHOLD)
}

pub fn damage_applies(amount: i32, threshold: i32) -> bool {
    amount >= threshold
}

pub fn should_destruct(dmg: i32, accumulate: i32) -> bool {
    dmg >= accumulate
}

pub fn areas_from_script(script_destructable_area: &str) -> impl Iterator<Item = &str> {
    script_destructable_area
        .split_whitespace()
        .filter(|part| !part.is_empty())
}

pub fn is_blockable_spawn_classname(classname: &str) -> bool {
    classname == SPAWN_TDM || classname == SPAWN_DM
}

pub fn block_ents_in_area(ent_area: &str, area: &str) -> bool {
    !ent_area.is_empty() && ent_area == area
}

pub fn spawn_blocked_off(spawn_area: &str, blocked_areas: &[impl AsRef<str>]) -> bool {
    if spawn_area.is_empty() {
        return false;
    }
    blocked_areas.iter().any(|area| area.as_ref() == spawn_area)
}
