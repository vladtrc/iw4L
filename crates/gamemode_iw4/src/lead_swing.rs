use crate::{kind::GameModeKind, phase::Team, sound_emit::ScoringTeam};

pub const LEAD_STATUS_DEBOUNCE_MS: i32 = 5_000;

pub const STATUS_GROUP: &str = "status";

pub const NO_LEAD_DIALOG_SCORE_LIMIT: i32 = 1;

pub const OBJECTIVE_POINTS_MOD_DEFAULT: i32 = 1;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum StatusDialog {
    LeadTaken,

    LeadLost,
}

impl StatusDialog {
    pub const fn gsc_key(self) -> &'static str {
        match self {
            Self::LeadTaken => "lead_taken",
            Self::LeadLost => "lead_lost",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LeaderDialogRequest {
    pub dialog: StatusDialog,

    pub team: ScoringTeam,

    pub group: &'static str,
}

impl LeaderDialogRequest {
    pub fn team(self) -> Team {
        self.team.into()
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct TeamScores {
    pub allies: i32,
    pub axis: i32,
}

impl TeamScores {
    pub const fn get(self, team: ScoringTeam) -> i32 {
        match team {
            ScoringTeam::Allies => self.allies,
            ScoringTeam::Axis => self.axis,
        }
    }

    pub const fn set(&mut self, team: ScoringTeam, score: i32) {
        match team {
            ScoringTeam::Allies => self.allies = score,
            ScoringTeam::Axis => self.axis = score,
        }
    }

    pub const fn leader(self) -> Option<ScoringTeam> {
        if self.allies > self.axis {
            Some(ScoringTeam::Allies)
        } else if self.axis > self.allies {
            Some(ScoringTeam::Axis)
        } else {
            None
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LeadSwing {
    pub was_winning: Option<ScoringTeam>,

    pub last_status_time_ms: i32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ScoreFollowUp {
    CheckScoreLimitSoon,

    OvertimeScoreLimit,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ObjectiveGrant {
    pub kind: GameModeKind,

    pub team: ScoringTeam,

    pub points: i32,

    pub now_ms: i32,

    pub splitscreen: bool,

    pub nuke_incoming: bool,

    pub score_limit: i32,

    pub overtime: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ObjectiveGrantOutcome {
    NukeIncoming,

    NotTeamBased,

    Granted {
        dialogs: usize,

        follow_up: Option<ScoreFollowUp>,
    },
}

pub const MAX_STATUS_DIALOGS: usize = 2;

pub fn give_team_score_for_objective(
    grant: ObjectiveGrant,
    scores: &mut TeamScores,
    swing: &mut LeadSwing,
    out: &mut [LeaderDialogRequest; MAX_STATUS_DIALOGS],
) -> ObjectiveGrantOutcome {
    if grant.nuke_incoming {
        return ObjectiveGrantOutcome::NukeIncoming;
    }
    if !grant.kind.is_team() {
        return ObjectiveGrantOutcome::NotTeamBased;
    }

    if let Some(leader) = scores.leader() {
        swing.was_winning = Some(leader);
    }

    let team_score = scores.get(grant.team) + grant.points;
    let follow_up = if team_score == scores.get(grant.team) {
        None
    } else {
        scores.set(grant.team, team_score);
        Some(if grant.overtime {
            ScoreFollowUp::OvertimeScoreLimit
        } else {
            ScoreFollowUp::CheckScoreLimitSoon
        })
    };

    let is_winning = scores.leader();
    let mut dialogs = 0;
    if let Some(winning) = is_winning
        && !grant.splitscreen
        && Some(winning) != swing.was_winning
        && grant.now_ms - swing.last_status_time_ms > LEAD_STATUS_DEBOUNCE_MS
        && grant.score_limit != NO_LEAD_DIALOG_SCORE_LIMIT
    {
        swing.last_status_time_ms = grant.now_ms;
        out[dialogs] = LeaderDialogRequest {
            dialog: StatusDialog::LeadTaken,
            team: winning,
            group: STATUS_GROUP,
        };
        dialogs += 1;
        if let Some(previous) = swing.was_winning {
            out[dialogs] = LeaderDialogRequest {
                dialog: StatusDialog::LeadLost,
                team: previous,
                group: STATUS_GROUP,
            };
            dialogs += 1;
        }
    }

    if let Some(winning) = is_winning {
        swing.was_winning = Some(winning);
    }

    ObjectiveGrantOutcome::Granted { dialogs, follow_up }
}
