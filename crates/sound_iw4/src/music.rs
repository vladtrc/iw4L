use killcam_iw4::log::Log;
use killcam_iw4::task::Millis;

use crate::dialog::{self, Broadcast};
use crate::level::{Fleet, Level, play_sound_on_players};
use crate::notify::{MatchEndingReason, Notify};
use crate::output::{Alias, ClientId, Output, PersTeam, Team};
use crate::recipes;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum MusicStep {
    #[default]
    WaitingForSoon,

    WaitingForVerySoon,

    Done,
}

#[derive(Clone, Copy, Debug, Default)]
pub struct MusicController {
    step: MusicStep,
}

impl MusicController {
    pub const fn new() -> Self {
        Self {
            step: MusicStep::WaitingForSoon,
        }
    }

    pub const fn step(&self) -> MusicStep {
        self.step
    }

    pub const fn starts_suspense(level: &Level) -> bool {
        !level.hardcore_mode
    }

    pub fn on_notify<const N: usize>(
        &mut self,
        now_ms: Millis,
        notify: Notify,
        level: &Level,
        fleet: &mut Fleet,
        out: &mut Log<Output, N>,
    ) {
        fleet.check();

        if notify == Notify::GameEnded {
            self.step = MusicStep::Done;
            return;
        }
        match (self.step, notify) {
            (MusicStep::WaitingForSoon, Notify::MatchEndingSoon(reason)) => {
                self.on_match_ending_soon(now_ms, reason, level, fleet, out);
            }
            (MusicStep::WaitingForVerySoon, Notify::MatchEndingVerySoon) => {
                dialog::leader_dialog(now_ms, level, fleet, Broadcast::everyone("timesup"), out);
                self.step = MusicStep::Done;
            }
            _ => {}
        }
    }

    fn on_match_ending_soon<const N: usize>(
        &mut self,
        now_ms: Millis,
        reason: MatchEndingReason,
        level: &Level,
        fleet: &mut Fleet,
        out: &mut Log<Output, N>,
    ) {
        if !level.is_last_round() {
            if !level.hardcore_mode {
                let alias = recipes::music_alias("losing_allies")
                    .expect("game[music][losing_allies] is in the table");
                play_sound_on_players(level, fleet.players, alias, None, &[], out);
            }
            dialog::leader_dialog(now_ms, level, fleet, Broadcast::everyone("timesup"), out);
            self.step = MusicStep::Done;
            return;
        }

        if level.splitscreen {
            self.step = MusicStep::Done;
            return;
        }
        branch(now_ms, reason, level, fleet, out);
        self.step = MusicStep::WaitingForVerySoon;
    }
}

fn branch<const N: usize>(
    now_ms: Millis,
    reason: MatchEndingReason,
    level: &Level,
    fleet: &mut Fleet,
    out: &mut Log<Output, N>,
) {
    let (winning_key, losing_key) = match reason {
        MatchEndingReason::Time => ("winning_time", "losing_time"),
        MatchEndingReason::Score => ("winning_score", "losing_score"),
    };
    if level.team_based {
        let Some(leader) = level.team_scores.leader() else {
            return;
        };
        if !level.hardcore_mode {
            let winning = team_music("winning_", leader);
            let losing = team_music("losing_", leader.other());
            play_sound_on_players(level, fleet.players, winning, Some(leader), &[], out);
            play_sound_on_players(level, fleet.players, losing, Some(leader.other()), &[], out);
        }
        dialog::leader_dialog(
            now_ms,
            level,
            fleet,
            Broadcast::to_team(winning_key, leader),
            out,
        );
        dialog::leader_dialog(
            now_ms,
            level,
            fleet,
            Broadcast::to_team(losing_key, leader.other()),
            out,
        );
        return;
    }

    match reason {
        MatchEndingReason::Time => {
            if !level.hardcore_mode {
                let alias = recipes::music_alias("losing_time")
                    .expect("game[music][losing_time] is in the table");
                play_sound_on_players(level, fleet.players, alias, None, &[], out);
            }
            dialog::leader_dialog(now_ms, level, fleet, Broadcast::everyone("timesup"), out);
        }

        MatchEndingReason::Score => {
            let winner = level
                .highest_scoring_player
                .expect("a score limit was approached, so somebody is scoring");
            if !level.hardcore_mode {
                let winner_index = fleet
                    .index_of(winner)
                    .expect("the leading player is connected");
                play_one(winner, "winning_", pers_team_of(fleet, winner_index), out);
                for i in 0..fleet.players.len() {
                    let client = fleet.players[i].client;
                    if client == winner {
                        continue;
                    }
                    play_one(client, "losing_", pers_team_of(fleet, i), out);
                }
            }
            dialog::leader_dialog_on_one(now_ms, level, fleet, winner, "winning_score", out);
            dialog::leader_dialog_on_players(
                now_ms,
                level,
                fleet,
                level.losing_players,
                "losing_score",
                out,
            );
        }
    }
}

fn team_music(stem: &str, team: Team) -> Alias {
    recipes::team_music_alias(stem, team).expect("the five per-team music keys are in the table")
}

fn play_one<const N: usize>(
    client: ClientId,
    stem: &str,
    team: Option<Team>,
    out: &mut Log<Output, N>,
) {
    let team = team.expect(
        "game[music] has no spectator entry; #L433/#L440 index it with pers[team] regardless",
    );
    out.push(Output::PlayLocalSound {
        client,
        alias: team_music(stem, team),
    });
}

fn pers_team_of(fleet: &Fleet, i: usize) -> Option<Team> {
    match fleet.players[i].pers_team {
        Some(PersTeam::Allies) => Some(Team::Allies),
        Some(PersTeam::Axis) => Some(Team::Axis),
        Some(PersTeam::Spectator) | None => None,
    }
}

pub fn play_spawn_music<const N: usize>(client: ClientId, team: Team, out: &mut Log<Output, N>) {
    out.push(Output::PlayLocalSound {
        client,
        alias: team_music("spawn_", team),
    });
}

pub fn play_ffa_game_win<const N: usize>(
    winner: Option<ClientId>,
    fleet: &Fleet,
    out: &mut Log<Output, N>,
) {
    for player in fleet.players {
        let alias = match player.pers_team {
            Some(PersTeam::Allies) | Some(PersTeam::Axis) => {
                let team = match player.pers_team {
                    Some(PersTeam::Allies) => Team::Allies,
                    _ => Team::Axis,
                };
                if Some(player.client) == winner {
                    team_music("victory_", team)
                } else {
                    team_music("defeat_", team)
                }
            }
            Some(PersTeam::Spectator) | None => {
                recipes::music_alias("nuke_music").expect("game[music][nuke_music] is in the table")
            }
        };
        out.push(Output::PlayLocalSound {
            client: player.client,
            alias,
        });
    }
}
