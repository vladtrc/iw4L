use crate::Team;

pub const LAST_CLAIM_GRACE_MS: i32 = 1000;

pub const PROX_USE_RATE_CAP: f32 = 4.0;

pub const PROX_THINK_TICK_MS: i32 = 50;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ProxClaimTeam {
    #[default]
    None,
    Axis,
    Allies,
}

impl ProxClaimTeam {
    pub const fn from_pers(team: Team) -> Option<Self> {
        match team {
            Team::Axis => Some(Self::Axis),
            Team::Allies => Some(Self::Allies),
            Team::Free => None,
        }
    }

    pub const fn as_team(self) -> Option<Team> {
        match self {
            Self::None => None,
            Self::Axis => Some(Team::Axis),
            Self::Allies => Some(Team::Allies),
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum GameObjectTeam {
    #[default]
    Neutral,
    Axis,
    Allies,
    None,
}

impl GameObjectTeam {
    pub const fn from_pers(team: Team) -> Self {
        match team {
            Team::Free => Self::None,
            Team::Axis => Self::Axis,
            Team::Allies => Self::Allies,
        }
    }

    pub const fn matches_pers(self, team: Team) -> bool {
        match (self, team) {
            (Self::Axis, Team::Axis) | (Self::Allies, Team::Allies) => true,
            (Self::None, Team::Free) => true,
            _ => false,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum InteractTeam {
    #[default]
    None,
    Any,
    Friendly,
    Enemy,
}

pub const fn can_interact_with(interact: InteractTeam, owner: GameObjectTeam, pers: Team) -> bool {
    match interact {
        InteractTeam::None => false,
        InteractTeam::Any => true,
        InteractTeam::Friendly => owner.matches_pers(pers),
        InteractTeam::Enemy => !owner.matches_pers(pers),
    }
}

pub fn set_claim_team_resets_progress(
    current: ProxClaimTeam,
    last_claim_team: ProxClaimTeam,
    last_claim_time_ms: i32,
    now_ms: i32,
    new_team: ProxClaimTeam,
) -> bool {
    if current == ProxClaimTeam::None
        && now_ms.saturating_sub(last_claim_time_ms) > LAST_CLAIM_GRACE_MS
    {
        return true;
    }
    if new_team != ProxClaimTeam::None && new_team != last_claim_team {
        return true;
    }
    false
}

pub fn update_use_rate(
    claim: ProxClaimTeam,
    touching_axis: i32,
    touching_allies: i32,
    claim_scalers: &[f32],
) -> f32 {
    let mut num_claimants = match claim {
        ProxClaimTeam::None => 0.0,
        ProxClaimTeam::Axis => touching_axis as f32,
        ProxClaimTeam::Allies => touching_allies as f32,
    };
    let mut num_other = 0i32;
    if claim != ProxClaimTeam::Axis {
        num_other += touching_axis;
    }
    if claim != ProxClaimTeam::Allies {
        num_other += touching_allies;
    }
    for scaler in claim_scalers {
        if *scaler == 1.0 {
            continue;
        }
        num_claimants *= *scaler;
    }
    if num_claimants != 0.0 && num_other == 0 {
        libm::fminf(num_claimants, PROX_USE_RATE_CAP)
    } else {
        0.0
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum ProxThinkOutcome {
    Idle,
    Progress { cur_progress: i32, use_rate: f32 },
    Unclaimed,
    Completed,
}

pub struct ProxThinkInput {
    pub use_time: i32,
    pub cur_progress: i32,
    pub use_rate: f32,
    pub claim: ProxClaimTeam,
    pub touching_claim: i32,
}

pub fn use_object_prox_think_body(input: &ProxThinkInput) -> ProxThinkOutcome {
    if input.use_time != 0 && input.cur_progress >= input.use_time {
        return ProxThinkOutcome::Completed;
    }
    if input.claim == ProxClaimTeam::None {
        return ProxThinkOutcome::Idle;
    }
    if input.use_time != 0 {
        if input.touching_claim == 0 {
            return ProxThinkOutcome::Unclaimed;
        }
        let cur_progress = input
            .cur_progress
            .saturating_add((50.0 * input.use_rate) as i32);
        return ProxThinkOutcome::Progress {
            cur_progress,
            use_rate: input.use_rate,
        };
    }
    ProxThinkOutcome::Completed
}
