use gamemode_iw4::Team;
use sim::ObjectiveMatch;
use std::collections::HashSet;

pub(crate) const EFFECTS: &[&str] = &[
    "mp_bomb_plant",
    "mp_bomb_defuse",
    "ui_mp_suitcasebomb_timer",
    "exp_suitcase_bomb_main",
    "mp_war_objective_taken",
    "mp_war_objective_lost",
];
const SECURING: [&str; 3] = ["securing_a", "securing_b", "securing_c"];
const LOSING: [&str; 3] = ["losing_a", "losing_b", "losing_c"];
const SECURED: [&str; 3] = ["secured_a", "secured_b", "secured_c"];
const ENEMY_HAS: [&str; 3] = ["enemy_has_a", "enemy_has_b", "enemy_has_c"];
const LOST: [&str; 3] = ["lost_a", "lost_b", "lost_c"];

pub(crate) enum Cue {
    Sound(&'static str),
    Dialog(&'static str),
}

#[derive(Default)]
pub(crate) struct ObjectiveAudio {
    previous: Option<ObjectiveMatch>,
    last_ms: u32,
    status_ms: [u32; 3],
    notified: HashSet<u32>,
    delayed: Vec<(u32, Team, &'static str)>,
}

impl ObjectiveAudio {
    fn status(&mut self, now: u32, team: Team, line: &'static str, force: bool) {
        if !matches!(team, Team::Allies | Team::Axis) {
            return;
        }
        if !force && now < self.status_ms[team as usize].saturating_add(5000) {
            return;
        }
        self.status_ms[team as usize] = now;

        self.delayed.push((now.saturating_add(100), team, line));
    }

    pub(crate) fn collect(&mut self, state: &ObjectiveMatch, now: u32) -> Vec<(Option<Team>, Cue)> {
        if now < self.last_ms || (state.flags.is_empty() && state.bombs.is_empty()) {
            *self = Self::default();
        }
        self.last_ms = now;
        let mut out = Vec::new();
        if let Some(old) = self.previous.take() {
            for flag in &state.flags {
                let Some(before) = old.flags.iter().find(|f| f.id == flag.id) else {
                    continue;
                };
                let index = match flag.label.as_str() {
                    "A" => 0,
                    "B" => 1,
                    "C" => 2,
                    _ => {
                        diag::warn!(
                            Audio,
                            "DOM dialog: unsupported authored label {}",
                            flag.label
                        );
                        continue;
                    }
                };
                if flag.capturing == Team::Free || flag.capturing != before.capturing {
                    self.notified.remove(&flag.id);
                }
                if flag.progress > 0.05
                    && flag.progress != before.progress
                    && !flag.contested
                    && self.notified.insert(flag.id)
                {
                    self.status(now, flag.capturing, SECURING[index], false);
                    self.status(now, flag.owner, LOSING[index], false);
                }
                if flag.owner != before.owner && matches!(flag.owner, Team::Allies | Team::Axis) {
                    self.notified.remove(&flag.id);
                    out.push((Some(flag.owner), Cue::Sound("mp_war_objective_taken")));
                    let other = if flag.owner == Team::Allies {
                        Team::Axis
                    } else {
                        Team::Allies
                    };
                    if before.owner != Team::Free {
                        out.push((Some(before.owner), Cue::Sound("mp_war_objective_lost")));
                    }
                    if before.owner != Team::Free
                        && state.flags.iter().all(|f| f.owner == flag.owner)
                    {
                        self.status(now, flag.owner, "secure_all", false);
                        self.status(now, other, "lost_all", false);
                    } else {
                        self.status(now, flag.owner, SECURED[index], true);
                        self.status(
                            now,
                            other,
                            if before.owner == Team::Free {
                                ENEMY_HAS[index]
                            } else {
                                LOST[index]
                            },
                            true,
                        );
                    }
                }
            }
            if old.round == state.round {
                for site in &state.bombs {
                    let Some(before) = old.bombs.iter().find(|b| b.view.id == site.view.id) else {
                        continue;
                    };
                    match (before.planted_at_ms, site.planted_at_ms) {
                        (None, Some(_)) => out.push((None, Cue::Dialog("bomb_planted"))),
                        (Some(_), None) if !site.destroyed => {
                            out.push((None, Cue::Dialog("bomb_defused")))
                        }
                        _ => {}
                    }
                }
            }
        }
        self.delayed.retain(|&(at, team, line)| {
            if at <= now {
                out.push((Some(team), Cue::Dialog(line)));
                false
            } else {
                true
            }
        });
        self.previous = Some(state.clone());
        out
    }
}
