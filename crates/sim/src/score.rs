use crate::frame::FrameWorld;
use crate::identities::MatchPhase;
use crate::match_state::{ClientLifecycle, EventAudience, SimEvent};
use crate::world::{ClientId, Tick};
use gamemode_iw4::ffa::{SCORE_KILL_POINTS, SCORE_LIMIT, TIME_LIMIT_MS};
use gamemode_iw4::sound_emit::{ScoreLimitSoonInput, check_player_score_limit_soon};
use gamemode_iw4::{FIRSTBLOOD_SCORE_INFO, FIRSTBLOOD_SPLASH_KEY};

pub const MATCH_TICK_MS: u32 = 50;

#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct KillFacts {
    pub one_shot: bool,
    pub headshot: bool,
    pub execution: bool,
    pub posthumous: bool,
    pub longshot: bool,
    pub throwing_knife: bool,
}

pub(crate) fn apply_death_score(
    world: &mut FrameWorld,
    tick: Tick,
    victim: ClientId,
    attacker: Option<ClientId>,
    facts: Option<KillFacts>,
) {
    let had_copycat = {
        let meta = world.client_meta_mut(victim);
        meta.deaths = meta.deaths.saturating_add(1);
        meta.combathigh_until_ms = None;
        meta.pistoldeath_this_life = false;
        meta.laststand_until_ms = None;
        meta.attackers_this_life.clear();
        let had = meta.copycat_this_life;
        meta.copycat_this_life = false;
        meta.copycat_class_this_life = false;
        had
    };

    let points = world.bootstrap_ref().score_kill_points;
    if let Some(attacker) = attacker
        && attacker != victim
    {
        {
            let meta = world.client_meta_mut(victim);
            meta.copycat_loadout = None;
        }
        if had_copycat {
            let skip = world
                .client_meta(attacker)
                .is_some_and(|m| m.copycat_class_this_life);
            let spec = world.client_meta(attacker).and_then(|m| m.loadout.clone());
            if !skip {
                if let Some(spec) = spec {
                    world.client_meta_mut(victim).copycat_loadout =
                        Some(crate::match_state::CopyCatLoadout {
                            spec,
                            in_use: false,
                            owner: attacker,
                        });
                }
            }
        }
        let comeback = world
            .client_meta(attacker)
            .is_some_and(|m| m.cur_death_streak > 3);
        let now = crate::hudelem::hud_level_time_ms(tick);
        let avenger = world.bootstrap_ref().kind.is_team()
            && world
                .client_meta(victim)
                .and_then(|m| m.last_kill)
                .is_some_and(|(killed, at)| killed != attacker && now - at <= 500);
        let defended = if world.bootstrap_ref().kind.is_team() {
            world.client_meta(victim).map_or(0, |m| {
                m.damaged_players
                    .iter()
                    .filter(|&&(damaged, at)| damaged != attacker && now - at < 500)
                    .count()
            })
        } else {
            0
        };
        let revenge = world
            .client_meta(attacker)
            .is_some_and(|m| m.last_killed_by == Some(victim));
        {
            let meta = world.client_meta_mut(attacker);
            meta.last_kill = Some((victim, now));
            meta.damaged_players
                .retain(|&(damaged, _)| damaged != victim);
            if revenge {
                meta.last_killed_by = None;
            }
        }
        world.client_meta_mut(victim).last_killed_by = Some(attacker);
        {
            let meta = world.client_meta_mut(attacker);
            meta.kills = meta.kills.saturating_add(1);
            meta.score = meta.score.saturating_add(points);
            meta.cur_death_streak = 0;
        }
        {
            let meta = world.client_meta_mut(victim);
            meta.cur_death_streak = meta.cur_death_streak.saturating_add(1);
        }
        crate::killstreaks::on_kill(world, victim, attacker);
        if let Some(row) = world.recent_kills.iter_mut().find(|row| row.0 == attacker) {
            row.1 = now;
            row.2 += 1;
        } else {
            world.recent_kills.push((attacker, now, 1));
        }
        let num_kills = world.bump_num_kills();
        world.record_score_popup(attacker, points as f32, now);
        let facts = facts.unwrap_or_default();
        if facts.one_shot {
            world.push_hud_splash(attacker, "one_shot_kill", 0, 0);
        }
        if num_kills == 1 {
            award_splash(
                world,
                attacker,
                FIRSTBLOOD_SPLASH_KEY,
                FIRSTBLOOD_SCORE_INFO,
                now,
            );
            broadcast_card(world, attacker, "callout_firstblood");
        }
        if comeback {
            award_splash(world, attacker, "comeback", 100, now);
        }
        if facts.headshot {
            award_splash(world, attacker, "headshot", points, now);
        } else if facts.execution {
            award_splash(world, attacker, "execution", 100, now);
        }
        if facts.posthumous {
            award_splash(world, attacker, "posthumous", 25, now);
        }
        if avenger {
            award_splash(world, attacker, "avenger", 50, now);
        }
        for _ in 0..defended {
            award_splash(world, attacker, "defender", 50, now);
        }
        if facts.longshot {
            award_splash(world, attacker, "longshot", 50, now);
        }
        if revenge {
            award_splash(world, attacker, "revenge", 50, now);
        }
        if facts.throwing_knife {
            world.push_hud_splash(attacker, "knifethrow", 0, 100);
        }
        if world.bootstrap_ref().kind == gamemode_iw4::GameModeKind::Domination {
            award_dom_kill(world, victim, attacker, now);
        }
        let (score, kills, deaths) = world
            .client_meta(attacker)
            .map(|m| (m.score, m.kills, m.deaths))
            .unwrap_or((0, 0, 0));

        let score_limit = world.bootstrap_ref().score_limit;
        if score_limit > 0 && score >= score_limit {
            world.set_pending_final_kill(Some((victim, attacker)));
        }

        evaluate_player_score_limit_soon(world, score);
        world.push_event(
            tick,
            EventAudience::All,
            SimEvent::ScoreChanged {
                client: attacker,
                score,
                kills,
                deaths,
            },
        );
    }

    crate::killstreaks::on_death(world, victim);

    let (score, kills, deaths) = world
        .client_meta(victim)
        .map(|m| (m.score, m.kills, m.deaths))
        .unwrap_or((0, 0, 0));
    world.push_event(
        tick,
        EventAudience::All,
        SimEvent::ScoreChanged {
            client: victim,
            score,
            kills,
            deaths,
        },
    );
}

pub(crate) fn finish_prematch(world: &mut FrameWorld, tick: Tick) {
    world.set_phase(MatchPhase::Playing);
    world.mark_prematch_done();
    if world.bootstrap_ref().kind == gamemode_iw4::GameModeKind::Demolition {
        world.set_use_start_spawns(false);
    }
    let now_ms = crate::hudelem::hud_level_time_ms(tick);
    crate::hudelem::sync_match_start_elems(world.hud_elem_slots_mut(), None, now_ms);
    if world.bootstrap_ref().kind.is_team() {
        return;
    }
    let alive: Vec<ClientId> = world
        .clients_scoreboard()
        .into_iter()
        .filter(|(_, row)| row.lifecycle == ClientLifecycle::Alive)
        .map(|(id, _)| id)
        .collect();
    let mut sound_ids = world.hud_elem_sound_ids();
    for client in alive {
        crate::hudelem::sync_hint_elems(world.hud_elem_slots_mut(), client, now_ms, &mut sound_ids);
    }
    world.set_hud_elem_sound_ids(sound_ids);
}

/// Runs on every team score change outside overtime.
pub(crate) fn evaluate_team_score_limit_soon(world: &mut FrameWorld, score: i32) {
    world.set_pending_score_limit_soon(None);
    let boot = world.bootstrap_ref();
    let notify = gamemode_iw4::sound_emit::check_team_score_limit_soon(ScoreLimitSoonInput {
        score_limit: boot.score_limit,
        objective_based: gamemode_iw4::OBJECTIVE_BASED,
        score_limit_override: false,
        team_based: true,
        time_passed_ms: world.match_elapsed_ms() as i32,
        score,
    });
    world.set_pending_score_limit_soon(notify);
}

fn evaluate_player_score_limit_soon(world: &mut FrameWorld, score: i32) {
    world.set_pending_score_limit_soon(None);
    let boot = world.bootstrap_ref();
    let notify = check_player_score_limit_soon(ScoreLimitSoonInput {
        score_limit: boot.score_limit,
        objective_based: false,
        score_limit_override: false,
        team_based: false,
        time_passed_ms: world.match_elapsed_ms() as i32,
        score,
    });
    world.set_pending_score_limit_soon(notify);
}

pub fn bootstrap_score_defaults() -> (i32, i32, u32) {
    (SCORE_LIMIT, SCORE_KILL_POINTS, TIME_LIMIT_MS)
}

pub(crate) fn award_splash(
    world: &mut FrameWorld,
    client: ClientId,
    key: &'static str,
    xp: i32,
    now: i32,
) {
    world.push_hud_splash(client, key, 0, xp);
    world.record_score_popup(client, xp as f32, now);
}

const DOM_ASSAULT_DEFEND_POINTS: i32 = 50;

// Both flag contacts award points; a repeated splash of the same kind stays hidden.
fn award_dom_kill(world: &mut FrameWorld, victim: ClientId, attacker: ClientId, now: i32) {
    let team_of = |id: ClientId| world.client_meta(id).map(|m| m.client_state_team);
    if team_of(victim) == team_of(attacker) {
        return;
    }
    let touched =
        |id: ClientId| -> Vec<(gamemode_iw4::ProxClaimTeam, gamemode_iw4::ProxClaimTeam)> {
            world
                .use_objects()
                .iter()
                .filter_map(|object| {
                    let owner = gamemode_iw4::claim_team_from_owner(object.owner_team)?;
                    let (_, team, _) = object.touching.iter().find(|row| row.0 == id)?;
                    Some((owner, *team))
                })
                .collect()
        };
    let mut awards = Vec::new();
    let (mut assaulted, mut defended) = (false, false);
    for (owner, team) in touched(victim) {
        let assault = team == owner;
        assaulted |= assault;
        defended |= !assault;
        awards.push((assault, true));
    }
    for (owner, team) in touched(attacker) {
        let assault = team != owner;
        awards.push((assault, if assault { !assaulted } else { !defended }));
    }
    for (assault, splash) in awards {
        let key = if assault { "assault" } else { "defend" };
        if splash {
            world.push_hud_splash(attacker, key, 0, DOM_ASSAULT_DEFEND_POINTS);
        }
        world.record_score_popup(attacker, DOM_ASSAULT_DEFEND_POINTS as f32, now);
        let meta = world.client_meta_mut(attacker);
        meta.score = meta.score.saturating_add(DOM_ASSAULT_DEFEND_POINTS);
    }
}

pub(crate) fn broadcast_card(world: &mut FrameWorld, source: ClientId, key: &'static str) {
    for recipient in world.client_ids_sorted() {
        player_card_splash(world, recipient, source, key);
    }
}

fn player_card_splash(
    world: &mut FrameWorld,
    recipient: ClientId,
    source: ClientId,
    key: &'static str,
) {
    world.push_player_card_slot(recipient, source, 5);
    world.push_hud_splash(recipient, key, 1, 0);
}
