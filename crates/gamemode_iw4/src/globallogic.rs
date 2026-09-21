pub const TEAM_BASED_DEFAULT: bool = false;

pub const OBJECTIVE_BASED: bool = false;

pub const END_GAME_ON_TIME_LIMIT: bool = true;

pub const POST_ROUND_TIME_MS: u32 = 5_000;

pub const HALFTIME_TYPE: &str = "halftime";

/// Halftime only when the round that just ended is the midpoint of the round
/// limit, or one win short of the win limit; every other switch is a side
/// switch.
pub fn round_switch_is_halftime(rounds_played: u32, round_limit: u32, win_limit: u32) -> bool {
    if round_limit != 0 {
        return rounds_played * 2 == round_limit;
    }
    if win_limit != 0 {
        return rounds_played == win_limit.saturating_sub(1);
    }
    false
}

pub fn ranked_match(online_game: bool, private_match: bool) -> bool {
    !online_game || !private_match
}
