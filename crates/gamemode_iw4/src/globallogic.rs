pub const TEAM_BASED_DEFAULT: bool = false;

pub const OBJECTIVE_BASED: bool = false;

pub const END_GAME_ON_TIME_LIMIT: bool = true;

pub const POST_ROUND_TIME_MS: u32 = 5_000;

pub const HALFTIME_TYPE: &str = "halftime";

pub fn ranked_match(online_game: bool, private_match: bool) -> bool {
    !online_game || !private_match
}
