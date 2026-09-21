use crate::frame::FrameWorld;
use crate::identities::MatchPhase;
use crate::match_state::{ClientLifecycle, EventAudience, MatchEndReason, SimEvent};
use crate::world::{ClientId, Tick};
use gamemode_iw4::ffa::{
    MatchEndCause, SCORE_KILL_POINTS, SCORE_LIMIT, TIME_LIMIT_MS, match_end_cause,
};
use gamemode_iw4::match_clock::{self, ClockTick};
use gamemode_iw4::sound_emit::{ScoreLimitSoonInput, check_player_score_limit_soon};
use gamemode_iw4::{FIRSTBLOOD_SCORE_INFO, FIRSTBLOOD_SPLASH_KEY, PrematchStep};

pub const MATCH_TICK_MS: u32 = 50;

pub(crate) fn apply_death_score(
    world: &mut FrameWorld,
    tick: Tick,
    victim: ClientId,
    attacker: Option<ClientId>,
) {
    let had_copycat = {
        let meta = world.client_meta_mut(victim);
        meta.deaths = meta.deaths.saturating_add(1);
        meta.combathigh_until_ms = None;
        meta.pistoldeath_this_life = false;
        meta.laststand_until_ms = None;
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
        let (score, kills, deaths) = {
            let meta = world.client_meta_mut(attacker);
            meta.kills = meta.kills.saturating_add(1);
            meta.score = meta.score.saturating_add(points);
            meta.cur_death_streak = 0;
            (meta.score, meta.kills, meta.deaths)
        };
        {
            let meta = world.client_meta_mut(victim);
            meta.cur_death_streak = meta.cur_death_streak.saturating_add(1);
        }
        let now = crate::hudelem::hud_level_time_ms(tick);
        if let Some(row) = world.recent_kills.iter_mut().find(|row| row.0 == attacker) {
            row.1 = now;
            row.2 += 1;
        } else {
            world.recent_kills.push((attacker, now, 1));
        }
        let num_kills = world.bump_num_kills();
        let now_ms = crate::hudelem::hud_level_time_ms(tick);
        world.record_score_popup(attacker, points as f32, now_ms);
        if num_kills == 1 {
            world.push_hud_splash(attacker, FIRSTBLOOD_SPLASH_KEY, 0, FIRSTBLOOD_SCORE_INFO);
            broadcast_card(world, attacker, "callout_firstblood");
        }

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

pub(crate) fn advance_match_clock(world: &mut FrameWorld, tick: Tick) {
    advance_prematch(world, tick);
    if world.bootstrap_ref().kind == gamemode_iw4::GameModeKind::Demolition {
        return;
    }
    if world.phase() != MatchPhase::Playing {
        return;
    }
    world.set_pending_match_clock(None);
    let before = world.match_elapsed_ms();
    world.add_match_elapsed_ms(MATCH_TICK_MS);
    let after = world.match_elapsed_ms();
    if before / 1000 != after / 1000 {
        evaluate_time_limit_clock(world, tick);
    }
    apply_match_end(world, tick);
}

fn advance_prematch(world: &mut FrameWorld, tick: Tick) {
    if world.phase() != MatchPhase::Warmup {
        return;
    }
    let alive = world
        .clients_scoreboard()
        .iter()
        .filter(|(_, row)| row.lifecycle == ClientLifecycle::Alive)
        .count() as u32;
    world.bump_max_alive_seen(alive);
    let next = world
        .prematch()
        .advance(MATCH_TICK_MS, world.max_alive_seen());
    world.set_prematch(next);
    if world.prematch() == PrematchStep::Done {
        finish_prematch(world, tick);
    }
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

fn evaluate_time_limit_clock(world: &mut FrameWorld, tick: Tick) {
    let time_limit_ms = world.bootstrap_ref().time_limit_ms;
    if time_limit_ms == 0 {
        return;
    }
    let elapsed = world.match_elapsed_ms();
    let time_remaining_ms = time_limit_ms as i32 - elapsed as i32;
    let time_limit_minutes = time_limit_ms as f32 / 60_000.0;
    let Some(emit) = match_clock::clock_tick(ClockTick {
        time_remaining_ms,
        time_limit_minutes,
        half_time: false,
        timer_stopped: false,
    }) else {
        return;
    };
    if emit.countdown_tick {
        push_countdown_tick_event(world, tick);
    }
    world.set_pending_match_clock(Some(emit));
}

pub(crate) fn push_countdown_tick_event(world: &mut FrameWorld, tick: Tick) {
    let event_parm = i32::from(world.sound_alias_index(match_clock::COUNTDOWN_TICK_ALIAS));
    world.push_entity_event(
        tick,
        EventAudience::All,
        entity_iw4::EntityEventKind::SOUND_ALIAS,
        crate::EntityEventPayload {
            number: i32::from(trace_iw4::ENTITYNUM_WORLD),
            event_parm,
            origin: match_clock::CLOCK_OBJECT_ORIGIN,
            ..Default::default()
        },
    );
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

pub(crate) fn apply_match_end(world: &mut FrameWorld, tick: Tick) {
    if world.phase() != MatchPhase::Playing {
        return;
    }
    let score_limit = world.bootstrap_ref().score_limit;
    let time_limit_ms = world.bootstrap_ref().time_limit_ms;
    let highest = world
        .clients_scoreboard()
        .iter()
        .map(|(_, row)| row.score)
        .max()
        .unwrap_or(0);
    let highest = if world.bootstrap_ref().kind == gamemode_iw4::GameModeKind::Domination {
        world.team_scores().axis.max(world.team_scores().allies)
    } else {
        highest
    };
    let Some(cause) = match_end_cause(
        highest,
        score_limit,
        world.match_elapsed_ms(),
        time_limit_ms,
    ) else {
        return;
    };
    world.set_phase(MatchPhase::Intermission);
    let reason = match cause {
        MatchEndCause::ScoreLimit => MatchEndReason::ScoreLimit,
        MatchEndCause::TimeLimit => MatchEndReason::TimeLimit,
    };
    if world.bootstrap_ref().kind == gamemode_iw4::GameModeKind::FreeForAll {
        latch_ffa_outcome(world, tick, reason);
    } else {
        let scores = world.team_scores();
        world.objectives.match_over = true;
        world.objectives.winner = if scores.axis > scores.allies {
            Some(gamemode_iw4::Team::Axis)
        } else if scores.allies > scores.axis {
            Some(gamemode_iw4::Team::Allies)
        } else {
            None
        };
        let winner = world.objectives.winner;
        world.set_pending_team_game_win(winner);
    }
    world.push_event(tick, EventAudience::All, SimEvent::MatchEnded { reason });
    diag::info!(
        Sim,
        "match: Intermission ({cause:?} elapsed_ms={} score_limit={})",
        world.match_elapsed_ms(),
        score_limit
    );
}

fn latch_ffa_outcome(world: &mut FrameWorld, tick: Tick, reason: MatchEndReason) {
    if world.outcome_hud_latched() {
        return;
    }
    world.set_outcome_hud_latched(true);
    let rows = world.clients_scoreboard();
    let mut ranked = rows.clone();
    gamemode_iw4::ffa_update_placement(&mut ranked, |(_, r)| (r.score, r.deaths));
    let mut tied_pairs: u32 = 0;
    for pair in ranked.windows(2) {
        if pair[0].1.score == pair[1].1.score && pair[0].1.deaths == pair[1].1.deaths {
            tied_pairs = tied_pairs.saturating_add(1);
        }
    }
    world.add_placement_cointoss_unwired(tied_pairs);
    let winner = ranked.first().map(|(id, _)| *id);
    world.set_pending_game_win(winner);
    let place_meta = [
        (
            gamemode_iw4::LABEL_FIRSTPLACE_NAME,
            gamemode_iw4::OUTCOME_FIRST_Y,
            gamemode_iw4::OUTCOME_FIRST_FONT_SCALE,
        ),
        (
            gamemode_iw4::LABEL_SECONDPLACE_NAME,
            gamemode_iw4::OUTCOME_SECOND_Y,
            gamemode_iw4::OUTCOME_OTHER_FONT_SCALE,
        ),
        (
            gamemode_iw4::LABEL_THIRDPLACE_NAME,
            gamemode_iw4::OUTCOME_THIRD_Y,
            gamemode_iw4::OUTCOME_OTHER_FONT_SCALE,
        ),
    ];
    let placement: Vec<(ClientId, i32, f32, f32)> = ranked
        .iter()
        .zip(place_meta)
        .map(|((id, _), (label, y, scale))| (*id, label, y, scale))
        .collect();
    let now_ms = crate::hudelem::hud_level_time_ms(tick);
    let reason_label = match reason {
        MatchEndReason::TimeLimit => gamemode_iw4::LABEL_TIME_LIMIT_REACHED,
        MatchEndReason::ScoreLimit => gamemode_iw4::LABEL_SCORE_LIMIT_REACHED,
    };
    let ranked_sd: Vec<(i32, i32)> = ranked.iter().map(|(_, r)| (r.score, r.deaths)).collect();
    let mut sound_ids = world.hud_elem_sound_ids();
    for (id, _) in &rows {
        let self_rank = ranked.iter().position(|(cid, _)| cid == id);
        let title_label = match gamemode_iw4::ffa_outcome_title(&ranked_sd, self_rank) {
            gamemode_iw4::FfaOutcomeTitle::Victory => gamemode_iw4::LABEL_VICTORY,
            gamemode_iw4::FfaOutcomeTitle::Defeat => gamemode_iw4::LABEL_DEFEAT,
            gamemode_iw4::FfaOutcomeTitle::Tie => gamemode_iw4::LABEL_MATCH_TIE,
        };
        crate::hudelem::sync_outcome_elems(
            world.hud_elem_slots_mut(),
            *id,
            title_label,
            reason_label,
            &placement,
            now_ms,
            &mut sound_ids,
        );
    }
    world.set_hud_elem_sound_ids(sound_ids);
}

pub fn bootstrap_score_defaults() -> (i32, i32, u32) {
    (SCORE_LIMIT, SCORE_KILL_POINTS, TIME_LIMIT_MS)
}

fn broadcast_card(world: &mut FrameWorld, source: ClientId, key: &'static str) {
    for recipient in world.client_ids_sorted() {
        world.push_player_card_slot(recipient, source, 5);
        world.push_hud_splash(recipient, key, 1, 0);
    }
}

pub(crate) fn finish_recent_kills(world: &mut FrameWorld, tick: Tick) {
    if world.phase() != crate::MatchPhase::Playing {
        world.recent_kills.clear();
        return;
    }
    let now = crate::hudelem::hud_level_time_ms(tick);
    let mut due = Vec::new();
    world.recent_kills.retain(|&(client, last, count)| {
        if now - last < 1000 {
            return true;
        }
        due.push((client, count));
        false
    });
    for (client, count) in due {
        if world.client_meta(client).is_none() {
            continue;
        }

        let splash = match count {
            2 => Some(("doublekill", 50)),
            3 => Some(("triplekill", 75)),
            4.. => Some(("multikill", 100)),
            _ => None,
        };
        if let Some((key, points)) = splash {
            world.push_hud_splash(client, key, 0, points);
        }
        if count == 3 {
            broadcast_card(world, client, "callout_3xkill");
        } else if count > 3 {
            broadcast_card(world, client, "callout_3xpluskill");
        }
    }
}
