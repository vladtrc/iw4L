pub const GAMETYPE_TOKEN: &str = "dd";

pub const DISPLAY_NAME: &str = "DEMOLITION";

pub const ROUND_LIMIT: u32 = 3;

pub const ROUND_SWITCH: u32 = 1;

pub const TIME_LIMIT_MS: u32 = 180_000;

pub const PLANT_MS: u32 = 5_000;

pub const DEFUSE_MS: u32 = 5_000;

pub const BOMB_FUSE_MS: u32 = 45_000;

pub const BRIEFCASE_PLANT: &str = "briefcase_bomb_mp";

pub const BRIEFCASE_DEFUSE: &str = "briefcase_bomb_defuse_mp";

pub const PLANTED_MODEL: &str = "prop_suitcase_bomb";

pub(crate) const SPAWN_ATTACKER: &str = "mp_dd_spawn_attacker";

pub(crate) const SPAWN_ATTACKER_A: &str = "mp_dd_spawn_attacker_a";

pub(crate) const SPAWN_ATTACKER_B: &str = "mp_dd_spawn_attacker_b";

pub(crate) const START_SPAWN_ATTACKER: &str = "mp_dd_spawn_attacker_start";

pub(crate) const SPAWN_DEFENDER: &str = "mp_dd_spawn_defender";

pub(crate) const SPAWN_DEFENDER_A: &str = "mp_dd_spawn_defender_a";

pub(crate) const SPAWN_DEFENDER_B: &str = "mp_dd_spawn_defender_b";

pub(crate) const START_SPAWN_DEFENDER: &str = "mp_dd_spawn_defender_start";

pub fn fuse_remaining_ms(elapsed_ms: u32) -> u32 {
    BOMB_FUSE_MS.saturating_sub(elapsed_ms)
}

pub const PLANTED_BOMB_EXPLODE_FX_PATH: Option<&str> = Some("explosions/tanker_explosion");
