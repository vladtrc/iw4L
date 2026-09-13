use killcam_iw4::log::Log;

use crate::dialog::PlayerDialog;
use crate::output::{Alias, ClientId, Output, PersTeam, Team};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Player {
    pub client: ClientId,

    pub pers_team: Option<PersTeam>,
}

impl Player {
    pub const fn new(client: u8, pers_team: PersTeam) -> Self {
        Self {
            client: ClientId(client),
            pers_team: Some(pers_team),
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct TeamScores {
    pub allies: i32,
    pub axis: i32,
}

impl TeamScores {
    pub const fn leader(self) -> Option<Team> {
        if self.allies > self.axis {
            Some(Team::Allies)
        } else if self.axis > self.allies {
            Some(Team::Axis)
        } else {
            None
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct Level<'a> {
    pub splitscreen: bool,

    pub team_based: bool,

    pub hardcore_mode: bool,

    pub roundlimit: i32,

    pub rounds_played: i32,

    pub team_scores: TeamScores,

    pub highest_scoring_player: Option<ClientId>,

    pub losing_players: &'a [ClientId],
}

impl Level<'_> {
    pub const fn is_last_round(&self) -> bool {
        self.roundlimit == 1 || self.rounds_played == self.roundlimit - 1
    }
}

pub struct Fleet<'a> {
    pub players: &'a [Player],
    pub queues: &'a mut [PlayerDialog],
}

impl Fleet<'_> {
    pub fn check(&self) {
        assert_eq!(
            self.players.len(),
            self.queues.len(),
            "one queue per player"
        );
    }

    pub fn index_of(&self, client: ClientId) -> Option<usize> {
        self.players.iter().position(|p| p.client == client)
    }
}

pub fn is_excluded(client: ClientId, exclude: &[ClientId]) -> bool {
    exclude.contains(&client)
}

pub fn play_sound_on_players<const N: usize>(
    level: &Level,
    players: &[Player],
    alias: Alias,
    team: Option<Team>,
    exclude: &[ClientId],
    out: &mut Log<Output, N>,
) {
    if level.splitscreen {
        if let Some(first) = players.first() {
            out.push(Output::PlayLocalSound {
                client: first.client,
                alias,
            });
        }
        return;
    }
    for player in players {
        if is_excluded(player.client, exclude) {
            continue;
        }
        if let Some(team) = team {
            let on_team = matches!(
                (player.pers_team, team),
                (Some(PersTeam::Allies), Team::Allies) | (Some(PersTeam::Axis), Team::Axis)
            );
            if !on_team {
                continue;
            }
        }
        out.push(Output::PlayLocalSound {
            client: player.client,
            alias,
        });
    }
}
