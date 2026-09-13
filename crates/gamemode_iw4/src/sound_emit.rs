use crate::phase::Team;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum MatchEndingReason {
    Time,

    Score,
}

impl MatchEndingReason {
    pub const fn gsc_argument(self) -> &'static str {
        match self {
            Self::Time => "time",
            Self::Score => "score",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum MatchSoundNotify {
    MatchEndingSoon(MatchEndingReason),

    MatchEndingVerySoon,
}

impl MatchSoundNotify {
    pub const fn gsc_name(self) -> &'static str {
        match self {
            Self::MatchEndingSoon(_) => "match_ending_soon",
            Self::MatchEndingVerySoon => "match_ending_very_soon",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum MatchSoundEmit {
    Notify(MatchSoundNotify),

    EntitySound {
        alias: &'static str,
        origin: [f32; 3],
    },
}

pub const SCORE_SETTLE_MS: i32 = 60_000;

pub const SCORE_SOON_MINUTES: f32 = 2.0;

pub const NO_SCORE_PACE_ESTIMATE: f32 = 999_999.0;

pub const SCORE_PACE_EPSILON: f32 = 0.0001;

pub fn score_per_minute(score: i32, time_passed_ms: i32) -> f32 {
    let minutes_passed = (time_passed_ms as f32 / 60_000.0) + SCORE_PACE_EPSILON;
    score as f32 / minutes_passed
}

pub const fn score_remaining(score: i32, score_limit: i32) -> i32 {
    score_limit - score
}

pub fn estimated_time_till_score_limit(score: i32, score_limit: i32, time_passed_ms: i32) -> f32 {
    let per_minute = score_per_minute(score, time_passed_ms);
    if per_minute != 0.0 {
        score_remaining(score, score_limit) as f32 / per_minute
    } else {
        NO_SCORE_PACE_ESTIMATE
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ScoreLimitSoonInput {
    pub score_limit: i32,

    pub objective_based: bool,

    pub score_limit_override: bool,

    pub team_based: bool,

    pub time_passed_ms: i32,

    pub score: i32,
}

pub fn check_team_score_limit_soon(input: ScoreLimitSoonInput) -> Option<MatchSoundNotify> {
    if input.score_limit <= 0 || input.objective_based {
        return None;
    }
    if input.score_limit_override {
        return None;
    }
    if !input.team_based {
        return None;
    }
    score_pace_notify(input)
}

pub fn check_player_score_limit_soon(input: ScoreLimitSoonInput) -> Option<MatchSoundNotify> {
    if input.score_limit <= 0 || input.objective_based {
        return None;
    }
    if input.team_based {
        return None;
    }
    score_pace_notify(input)
}

fn score_pace_notify(input: ScoreLimitSoonInput) -> Option<MatchSoundNotify> {
    if input.time_passed_ms < SCORE_SETTLE_MS {
        return None;
    }
    let projected =
        estimated_time_till_score_limit(input.score, input.score_limit, input.time_passed_ms);
    if projected < SCORE_SOON_MINUTES {
        Some(MatchSoundNotify::MatchEndingSoon(MatchEndingReason::Score))
    } else {
        None
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ScoringTeam {
    Allies,

    Axis,
}

impl ScoringTeam {
    pub const fn other(self) -> Self {
        match self {
            Self::Allies => Self::Axis,
            Self::Axis => Self::Allies,
        }
    }

    pub const fn gsc_key(self) -> &'static str {
        match self {
            Self::Allies => "allies",
            Self::Axis => "axis",
        }
    }
}

impl From<ScoringTeam> for Team {
    fn from(side: ScoringTeam) -> Self {
        match side {
            ScoringTeam::Allies => Team::Allies,
            ScoringTeam::Axis => Team::Axis,
        }
    }
}
