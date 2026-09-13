use crate::phase::Team;
use crate::use_prox::GameObjectTeam;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DomOnUseError {
    NeutralPlayer,
}

pub fn owner_team_from_capturer(pers: Team) -> Result<GameObjectTeam, DomOnUseError> {
    match pers {
        Team::Axis => Ok(GameObjectTeam::Axis),
        Team::Allies => Ok(GameObjectTeam::Allies),
        Team::Free => Err(DomOnUseError::NeutralPlayer),
    }
}

pub fn team_flag_count(owners: &[GameObjectTeam], team: GameObjectTeam) -> i32 {
    owners.iter().filter(|owner| **owner == team).count() as i32
}
