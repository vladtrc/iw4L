pub const GAMETYPE_TOKEN: &str = "dm";

pub const GAMETYPE_DIALOG_LINE: &str = "freeforall";

pub const DISPLAY_NAME: &str = "FREE FOR ALL";

pub const SCORE_KILL_POINTS: i32 = 50;

pub const SCORE_LIMIT: i32 = 1000;

pub const TIME_LIMIT_MS: u32 = 600_000;

pub const ROUND_LIMIT: i32 = 1;

pub const MATCH_END_MS: u32 = crate::dom::SCORE_MATCH_END_MS;

pub const SPAWN_CLASSNAME: &str = "mp_dm_spawn";

pub const START_SPAWN_CLASSNAME: &str = "mp_dm_spawn_start";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MatchEndCause {
    ScoreLimit,

    TimeLimit,
}

pub const fn match_end_cause(
    highest_score: i32,
    score_limit: i32,
    elapsed_ms: u32,
    time_limit_ms: u32,
) -> Option<MatchEndCause> {
    if score_limit > 0 && highest_score >= score_limit {
        return Some(MatchEndCause::ScoreLimit);
    }
    if time_limit_ms > 0 && elapsed_ms >= time_limit_ms {
        return Some(MatchEndCause::TimeLimit);
    }
    None
}

pub fn score_from_kills(kill_count: u32) -> i32 {
    (kill_count as i32).saturating_mul(SCORE_KILL_POINTS)
}

pub fn score_limit_reached(score: i32) -> bool {
    score >= SCORE_LIMIT
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FfaOutcomeTitle {
    Victory,

    Defeat,

    Tie,
}

pub const fn ffa_player_is_better(
    challenger_score: i32,
    challenger_deaths: i32,
    incumbent_score: i32,
    incumbent_deaths: i32,
) -> bool {
    if challenger_score > incumbent_score {
        return true;
    }
    if incumbent_score > challenger_score {
        return false;
    }
    challenger_deaths < incumbent_deaths
}

pub fn ffa_update_placement<T>(items: &mut [T], score_deaths: impl Fn(&T) -> (i32, i32)) {
    for i in 1..items.len() {
        let mut j = i;
        while j > 0 {
            let (cs, cd) = score_deaths(&items[j]);
            let (is_, id_) = score_deaths(&items[j - 1]);
            if ffa_player_is_better(cs, cd, is_, id_) {
                items.swap(j, j - 1);
                j -= 1;
            } else {
                break;
            }
        }
    }
}

pub const fn ffa_highest_scoring_index(ranked_len: usize) -> Option<usize> {
    if ranked_len == 0 { None } else { Some(0) }
}

pub fn ffa_outcome_title(ranked: &[(i32, i32)], self_rank: Option<usize>) -> FfaOutcomeTitle {
    let p0 = ranked.first().copied();
    let p1 = ranked.get(1).copied();
    let p2 = ranked.get(2).copied();
    if let (Some((s0, d0)), Some((s1, d1)), Some(i)) = (p0, p1, self_rank)
        && s0 == s1
        && d0 == d1
        && (i == 0 || i == 1)
    {
        return FfaOutcomeTitle::Tie;
    }
    if let (Some((s0, d0)), Some((s2, d2)), Some(2)) = (p0, p2, self_rank)
        && s0 == s2
        && d0 == d2
    {
        return FfaOutcomeTitle::Tie;
    }
    if self_rank == Some(0) && p0.is_some() {
        FfaOutcomeTitle::Victory
    } else {
        FfaOutcomeTitle::Defeat
    }
}
