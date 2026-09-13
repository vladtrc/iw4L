use crate::sound_emit::ScoringTeam;
use crate::use_prox::GameObjectTeam;

pub const UPDATE_DOM_SCORES_WAIT_MS: i32 = 5_000;

pub const DOM_FLAG_SCORE_POINTS: i32 = 1;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct OwnedDomFlag {
    pub object: u32,
    pub owner: GameObjectTeam,
    pub capture_time_ms: i32,
}

pub fn scoring_team(owner: GameObjectTeam) -> Option<ScoringTeam> {
    match owner {
        GameObjectTeam::Axis => Some(ScoringTeam::Axis),
        GameObjectTeam::Allies => Some(ScoringTeam::Allies),
        GameObjectTeam::Neutral | GameObjectTeam::None => None,
    }
}

pub fn is_owned_dom_flag(owner: GameObjectTeam, capture_time_ms: Option<i32>) -> bool {
    scoring_team(owner).is_some() && capture_time_ms.is_some()
}

pub fn sort_owned_oldest_first(flags: &mut [OwnedDomFlag], now_ms: i32) {
    for i in 1..flags.len() {
        let pulled = flags[i];
        let flag_score = now_ms.saturating_sub(pulled.capture_time_ms);
        let mut j = i as i32 - 1;
        while j >= 0 {
            let older = now_ms.saturating_sub(flags[j as usize].capture_time_ms);
            if flag_score <= older {
                break;
            }
            flags[(j + 1) as usize] = flags[j as usize];
            j -= 1;
        }
        flags[(j + 1) as usize] = pulled;
    }
}
