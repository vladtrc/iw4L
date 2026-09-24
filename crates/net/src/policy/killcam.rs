use std::collections::{HashMap, HashSet};

use bevy::prelude::*;
use killcam_iw4::{
    CamtimeInput, CancelTick, CleanupEntry, CleanupStep, DeathConfig, DeathOutput, DeathSequence,
    FinalKillcamConfig, FinalKillcamOutput, FinalKillcamSequence, FinalKillcamStart, NothingToShow,
    NotifyKind, Recalc, RoundEndWaitConfig, RoundEndWaitOutput, RoundEndWaitSequence,
    SERVER_FRAME_MS, SkipEdge, StartKillcam, WindowPlan, camtime, end_killcam_if_nothing_to_show,
    killcam_cleanup_steps, plan_window, postdelay, recalc_after_first_frame,
};
use playerstate_iw4::{SeatFocus, UserCmd, buttons};
use sim::{
    ClientAction, ClientId, ClientLifecycle, EventAudience, EventRecord, MatchPhase, SimEvent,
    Snapshot,
};

use crate::policy::seat::{ActiveKillcams, KillcamSession, killcam_seconds_to_ms};
use crate::transport::archive::{ARCHIVE_TICK_MS, FrameArchive};

#[derive(Resource, Debug, Default, Clone)]
pub struct ScriptKillcamEmitStats {
    pub begin_killcam: u32,
    pub abort_killcam: u32,
    pub killcam_ended: u32,
    pub seats_armed: u32,
    pub timelines_started: u32,
    pub timelines_finished_no_cam: u32,

    pub phase_a_cancelled: u32,

    pub phase_b_aborted: u32,

    pub final_killcam_started: u32,

    pub final_seats_armed: u32,

    pub round_end_wait_started: u32,

    pub round_end_finished: u32,

    pub spawned_player: u32,

    pub spawn_client: u32,

    pub seats_refused_already_alive: u32,

    pub final_seats_refused: u32,

    pub final_killcam_done: u32,
}

#[derive(Resource, Debug, Default)]
pub struct ActiveKillcamSkips {
    by_viewer: HashMap<ClientId, SkipEdge>,
}

impl ActiveKillcamSkips {
    pub fn arm(&mut self, viewer: ClientId) {
        self.by_viewer.insert(viewer, SkipEdge::new());
    }

    pub fn clear(&mut self, viewer: ClientId) {
        self.by_viewer.remove(&viewer);
    }

    pub fn get_mut(&mut self, viewer: ClientId) -> Option<&mut SkipEdge> {
        self.by_viewer.get_mut(&viewer)
    }
}

#[derive(Clone, Debug)]
pub struct PendingDeath {
    pub seq: DeathSequence,
    pub focus: ClientId,

    pub weapon: String,

    pub killcam_entity_start_time: i32,
    pub entity_focus: Option<killcam_iw4::focus::Entity>,
}

#[derive(Clone, Debug)]
struct PendingFinal {
    seq: FinalKillcamSequence,
    focus: ClientId,
    victim: ClientId,
    weapon: String,
    killcam_entity_start_time: i32,
    entity_focus: Option<killcam_iw4::focus::Entity>,
}

#[derive(Resource, Debug, Default)]
pub struct PendingDeathTimelines {
    by_victim: HashMap<ClientId, PendingDeath>,
    final_kc: Option<PendingFinal>,
    round_end: Option<RoundEndWaitSequence>,

    spawn_when_seat_ends: HashSet<ClientId>,

    spawn_after: HashSet<ClientId>,
}

impl PendingDeathTimelines {
    pub fn get(&self, victim: ClientId) -> Option<&PendingDeath> {
        self.by_victim.get(&victim)
    }

    pub fn len(&self) -> usize {
        self.by_victim.len()
    }

    pub fn is_empty(&self) -> bool {
        self.by_victim.is_empty()
            && self.final_kc.is_none()
            && self.round_end.is_none()
            && self.spawn_when_seat_ends.is_empty()
            && self.spawn_after.is_empty()
    }

    pub fn final_pending(&self) -> bool {
        self.final_kc.is_some()
    }

    pub fn queue_spawn_client(&mut self, victim: ClientId) {
        self.spawn_when_seat_ends.remove(&victim);
        self.spawn_after.insert(victim);
    }

    pub fn mark_spawn_when_seat_ends(&mut self, victim: ClientId) {
        self.spawn_after.remove(&victim);
        self.spawn_when_seat_ends.insert(victim);
    }

    pub fn on_ordinary_seat_finished(&mut self, viewer: ClientId) {
        if self.spawn_when_seat_ends.remove(&viewer) {
            self.spawn_after.insert(viewer);
        }
    }

    pub fn queue_spawn_for_deaths_without_timeline(&mut self, journal: &[EventRecord]) {
        for rec in journal {
            let SimEvent::Died { victim, .. } = &rec.event else {
                continue;
            };
            if self.by_victim.contains_key(victim)
                || self.spawn_when_seat_ends.contains(victim)
                || self.spawn_after.contains(victim)
            {
                continue;
            }
            self.spawn_after.insert(*victim);
        }
    }

    pub fn take_spawn_client(&mut self) -> Vec<ClientId> {
        let mut out: Vec<ClientId> = self.spawn_after.drain().collect();
        out.sort_by_key(|id| id.0);
        out
    }
}

pub fn session_from_start_killcam(
    archive: &FrameArchive,
    now_ms: i32,
    focus: ClientId,
    weapon: &str,
    start: StartKillcam,
    maxtime: Option<f32>,
    killcam_entity_start_time: i32,
) -> Option<KillcamSession> {
    session_from_window_plan(
        archive,
        now_ms,
        focus,
        weapon,
        start.predelay,
        start.time_until_respawn,
        maxtime,
        false,
        killcam_entity_start_time,
    )
}

pub fn session_from_final_killcam(
    archive: &FrameArchive,
    now_ms: i32,
    focus: ClientId,
    weapon: &str,
    start: FinalKillcamStart,
    killcam_entity_start_time: i32,
) -> Option<KillcamSession> {
    session_from_window_plan(
        archive,
        now_ms,
        focus,
        weapon,
        start.predelay,
        start.time_until_respawn,
        Some(start.maxtime),
        true,
        killcam_entity_start_time,
    )
}

pub fn session_from_window_plan(
    archive: &FrameArchive,
    now_ms: i32,
    focus: ClientId,
    weapon: &str,
    predelay_s: f32,
    time_until_respawn: f32,
    maxtime: Option<f32>,
    showing_final_killcam: bool,
    killcam_entity_start_time: i32,
) -> Option<KillcamSession> {
    let (camtime_s, _) = camtime(&CamtimeInput {
        now_ms,
        weapon,
        killcam_entity_start_time,
        predelay: predelay_s,
        showing_final_killcam,
        time_until_respawn,
        scr_killcam_time: None,
    });
    let postdelay_s = postdelay(None);
    let WindowPlan::Show(window) = plan_window(camtime_s, postdelay_s, predelay_s, maxtime) else {
        return None;
    };
    let requested_ms = killcam_seconds_to_ms(window.killcamoffset);
    let lookup = archive.lookup(requested_ms);
    if lookup.nothing_to_show() {
        return None;
    }
    let length_ms = killcam_seconds_to_ms(window.killcamlength);
    Some(KillcamSession {
        archivetime_ms: requested_ms,
        focus_client: focus,
        focus: SeatFocus::hitscan(focus.0 as i32),
        ends_at_ms: now_ms.saturating_add(length_ms),
        kc_info_tus_ms: (time_until_respawn * 1000.0) as i32,
        kc_timer_ends_at_ms: now_ms.saturating_add(killcam_seconds_to_ms(window.camtime)),
        final_kill: showing_final_killcam,
        started_at_ms: now_ms,
        killcamoffset_ms: requested_ms,
        predelay_ms: (predelay_s * 1000.0).round() as i32,
        postdelay_ms: (window.postdelay * 1000.0).round() as i32,
        recalc_pending: true,
        entity_focus: None,
    })
}

fn arm_seat(
    seats: &mut ActiveKillcams,
    skips: &mut ActiveKillcamSkips,
    archive: &FrameArchive,
    now_ms: i32,
    victim: ClientId,
    focus: ClientId,
    weapon: &str,
    killcam_entity_start_time: i32,
    entity_focus: Option<killcam_iw4::focus::Entity>,
    start: StartKillcam,
    maxtime: Option<f32>,
    install_phase_b: bool,
    stats: &mut ScriptKillcamEmitStats,
    begin: &mut Vec<ClientId>,
) -> bool {
    let Some(mut session) = session_from_start_killcam(
        archive,
        now_ms,
        focus,
        weapon,
        start,
        maxtime,
        killcam_entity_start_time,
    ) else {
        return false;
    };
    session.entity_focus = entity_focus;
    update_entity_focus(&mut session, now_ms);
    seats.arm(victim, session);
    if install_phase_b {
        skips.arm(victim);
    }
    begin.push(victim);
    stats.begin_killcam = stats.begin_killcam.saturating_add(1);
    stats.seats_armed = stats.seats_armed.saturating_add(1);
    true
}

pub fn time_until_round_end(snapshot: &Snapshot) -> Option<f32> {
    let meta = &snapshot.meta;
    if meta.time_limit_ms == 0 {
        return None;
    }
    let left_ms = meta.time_limit_ms as i64 - meta.match_elapsed_ms as i64;
    Some(left_ms as f32 / 1000.0 + POST_ROUND_TIME_SECONDS)
}

const POST_ROUND_TIME_SECONDS: f32 = 5.0;

pub fn time_until_spawn_seconds(respawn_delay_ticks: u32, tick_ms: u32) -> f32 {
    respawn_delay_ticks as f32 * (tick_ms as f32 / 1000.0)
}

pub fn level_notifies_from_journal(journal: &[EventRecord]) -> Vec<NotifyKind> {
    let mut out = Vec::new();
    for rec in journal {
        if matches!(rec.event, SimEvent::MatchEnded { .. }) {
            out.push(NotifyKind::GameEnded);
        }
    }
    out
}

pub fn spawned_clients_from_journal(journal: &[EventRecord]) -> Vec<ClientId> {
    let mut out = Vec::new();
    for rec in journal {
        if !matches!(rec.event, SimEvent::Spawned { .. }) {
            continue;
        }
        match rec.audience {
            EventAudience::Client(id) => out.push(id),
            EventAudience::All | EventAudience::AllExcept(_) | EventAudience::Clients(_) => {}
        }
    }
    out
}

pub fn emit_spawned_player(
    seats: &mut ActiveKillcams,
    skips: &mut ActiveKillcamSkips,
    journal: &[EventRecord],
    stats: &mut ScriptKillcamEmitStats,
    spawned: &mut Vec<ClientId>,
    ended: &mut Vec<ClientId>,
) {
    for client in spawned_clients_from_journal(journal) {
        spawned.push(client);
        stats.spawned_player = stats.spawned_player.saturating_add(1);
        if seats.get(client).is_some() {
            seats.clear(client);
            skips.clear(client);
            finish_cleanup(
                CleanupEntry::Spawned { clear_state: false },
                client,
                stats,
                ended,
            );
        }
    }
}

pub fn start_round_end_wait_from_journal(
    pending: &mut PendingDeathTimelines,
    now_ms: i32,
    journal: &[EventRecord],
    stats: &mut ScriptKillcamEmitStats,
) {
    if !journal
        .iter()
        .any(|r| matches!(r.event, SimEvent::MatchEnded { .. }))
    {
        return;
    }
    if pending.round_end.is_some() {
        return;
    }
    pending.round_end = Some(RoundEndWaitSequence::start(
        now_ms,
        RoundEndWaitConfig::ffa_match_end(),
    ));
    stats.round_end_wait_started = stats.round_end_wait_started.saturating_add(1);
}

pub fn tick_round_end_wait(
    pending: &mut PendingDeathTimelines,
    now_ms: i32,
    stats: &mut ScriptKillcamEmitStats,
) -> Vec<NotifyKind> {
    let Some(mut seq) = pending.round_end.take() else {
        return Vec::new();
    };
    let log = seq.advance(now_ms);
    let mut out = Vec::new();
    for o in log.iter() {
        match o {
            RoundEndWaitOutput::GiveMatchBonus => {}
            RoundEndWaitOutput::RoundEndFinished => {
                out.push(NotifyKind::RoundEndFinished);
                stats.round_end_finished = stats.round_end_finished.saturating_add(1);
            }
        }
    }
    if !seq.is_done() {
        pending.round_end = Some(seq);
    }
    out
}

pub fn weapon_script_name_of(names: &[String], weapon: u32) -> &str {
    names.get(weapon as usize).map(String::as_str).unwrap_or("")
}

pub fn start_timelines_from_deaths(
    pending: &mut PendingDeathTimelines,
    now_ms: i32,
    journal: &[EventRecord],
    snapshot: &Snapshot,
    archive: &FrameArchive,
    time_until_spawn: f32,
    weapon_script_names: &[String],
    pending_final_kill: Option<(ClientId, ClientId)>,
    copycat_victims: &HashSet<ClientId>,
    stats: &mut ScriptKillcamEmitStats,
) {
    for rec in journal {
        let SimEvent::Died {
            victim,
            attacker,
            weapon,
            killcam_entity_start_time,
            source,
            ..
        } = &rec.event
        else {
            continue;
        };
        if pending.by_victim.contains_key(victim) {
            continue;
        }

        let do_killcam = attacker.is_some_and(|a| a != *victim);
        let focus = attacker.unwrap_or(*victim);
        let final_kill = do_killcam && pending_final_kill == Some((*victim, focus));
        let cfg = DeathConfig {
            time_until_spawn,
            final_kill,
            do_killcam,
            victim_has_copycat: copycat_victims.contains(victim),
            ..DeathConfig::default()
        };
        let weapon_s = weapon_script_name_of(weapon_script_names, *weapon).to_owned();
        let entity_focus = match source {
            Some(sim::DamageSource::Projectile(id)) => snapshot
                .projectiles
                .iter()
                .find(|p| p.id == *id)
                .or_else(|| {
                    archive
                        .newest()
                        .and_then(|f| f.snapshot.projectiles.iter().find(|p| p.id == *id))
                })
                .and_then(|p| {
                    killcam_iw4::focus::get_killcam_entity(
                        Some(killcam_iw4::focus::Inflictor {
                            entity: killcam_iw4::focus::Entity {
                                entity_number: p.entnum,
                                birthtime: Some(p.spawn_time_ms),
                            },
                            is_attacker: false,
                            classname: "missile",
                            script_gameobjectname: None,
                            kill_cam_ent: None,
                        }),
                        &weapon_s,
                    )
                    .0
                }),
            _ => None,
        };
        let (seq, first) = DeathSequence::start(now_ms, cfg);

        for out in first.iter() {
            if matches!(out, DeathOutput::ThreadFinalKillcam) {
                start_final_killcam(
                    pending,
                    now_ms,
                    *victim,
                    focus,
                    &weapon_s,
                    *killcam_entity_start_time,
                    entity_focus,
                    stats,
                );
            }
        }
        pending.by_victim.insert(
            *victim,
            PendingDeath {
                seq,
                focus,
                weapon: weapon_s,
                killcam_entity_start_time: *killcam_entity_start_time,
                entity_focus,
            },
        );
        stats.timelines_started = stats.timelines_started.saturating_add(1);
    }
}

pub fn use_button_pressed(cmds: &[(ClientId, UserCmd)], client: ClientId) -> bool {
    cmds.iter()
        .find(|(id, _)| *id == client)
        .is_some_and(|(_, cmd)| cmd.buttons & (buttons::USE | buttons::USE_RELOAD) != 0)
}

fn snapshot_client_is_alive(snapshot: &Snapshot, id: ClientId) -> bool {
    snapshot
        .meta
        .for_client(id)
        .is_some_and(|m| m.lifecycle == ClientLifecycle::Alive)
}

pub fn tick_death_timelines(
    pending: &mut PendingDeathTimelines,
    seats: &mut ActiveKillcams,
    skips: &mut ActiveKillcamSkips,
    archive: &FrameArchive,
    now_ms: i32,
    cmds: &[(ClientId, UserCmd)],
    level_notifies: &[NotifyKind],
    spawned_this_tick: &[ClientId],
    stats: &mut ScriptKillcamEmitStats,
    begin: &mut Vec<ClientId>,
    snapshot: &Snapshot,
) {
    let victims: Vec<ClientId> = pending.by_victim.keys().copied().collect();
    for victim in victims {
        let Some(mut entry) = pending.by_victim.remove(&victim) else {
            continue;
        };
        let use_pressed = use_button_pressed(cmds, victim);
        let mut incoming: Vec<NotifyKind> = level_notifies.to_vec();
        if spawned_this_tick.contains(&victim) {
            incoming.push(NotifyKind::SpawnedPlayer);
            incoming.push(NotifyKind::Spawned);
        }
        let log = entry.seq.advance(now_ms, use_pressed, &incoming);
        let mut finished = entry.seq.is_done();
        for out in log.iter() {
            match out {
                DeathOutput::ThreadFinalKillcam => {
                    start_final_killcam(
                        pending,
                        now_ms,
                        victim,
                        entry.focus,
                        &entry.weapon,
                        entry.killcam_entity_start_time,
                        entry.entity_focus,
                        stats,
                    );
                }
                DeathOutput::StartKillcam(start) => {
                    if !matches!(
                        snapshot.meta.phase,
                        MatchPhase::Warmup | MatchPhase::Playing
                    ) || pending.final_pending()
                    {
                        stats.timelines_finished_no_cam =
                            stats.timelines_finished_no_cam.saturating_add(1);
                        pending.queue_spawn_client(victim);
                        finished = true;
                    } else if snapshot_client_is_alive(snapshot, victim) {
                        stats.seats_refused_already_alive =
                            stats.seats_refused_already_alive.saturating_add(1);
                        finished = true;
                    } else if arm_seat(
                        seats,
                        skips,
                        archive,
                        now_ms,
                        victim,
                        entry.focus,
                        &entry.weapon,
                        entry.killcam_entity_start_time,
                        entry.entity_focus,
                        start,
                        time_until_round_end(snapshot),
                        true,
                        stats,
                        begin,
                    ) {
                        pending.mark_spawn_when_seat_ends(victim);
                        finished = true;
                    } else {
                        pending.queue_spawn_client(victim);
                        finished = true;
                    }
                }
                DeathOutput::CancelKillcamPressed => {
                    stats.phase_a_cancelled = stats.phase_a_cancelled.saturating_add(1);
                }
                DeathOutput::NoKillcam(_) => {
                    stats.timelines_finished_no_cam =
                        stats.timelines_finished_no_cam.saturating_add(1);
                    pending.queue_spawn_client(victim);
                    finished = true;
                }
                DeathOutput::ThreadDeathCopyCatButton => {}
                _ => {}
            }
        }
        if !finished {
            pending.by_victim.insert(victim, entry);
        }
    }
}

fn start_final_killcam(
    pending: &mut PendingDeathTimelines,
    death_time_ms: i32,
    victim: ClientId,
    focus: ClientId,
    weapon: &str,
    killcam_entity_start_time: i32,
    entity_focus: Option<killcam_iw4::focus::Entity>,
    stats: &mut ScriptKillcamEmitStats,
) {
    let (seq, _log) = FinalKillcamSequence::start(FinalKillcamConfig {
        death_time_ms,
        death_time_offset: 0.0,
    });
    pending.final_kc = Some(PendingFinal {
        seq,
        focus,
        victim,
        weapon: weapon.to_owned(),
        killcam_entity_start_time,
        entity_focus,
    });
    stats.final_killcam_started = stats.final_killcam_started.saturating_add(1);
}

pub fn tick_final_killcam(
    pending: &mut PendingDeathTimelines,
    seats: &mut ActiveKillcams,
    archive: &FrameArchive,
    now_ms: i32,
    viewers: &[ClientId],
    level_notifies: &[NotifyKind],
    stats: &mut ScriptKillcamEmitStats,
    begin: &mut Vec<ClientId>,
    killedby_cards: &mut Vec<(ClientId, ClientId)>,
) -> bool {
    let Some(mut final_pending) = pending.final_kc.take() else {
        return false;
    };

    let any_players_in_killcam = !seats.is_empty();
    let log = final_pending
        .seq
        .advance(now_ms, level_notifies, any_players_in_killcam);
    for out in log.iter() {
        match out {
            FinalKillcamOutput::Start(start) => {
                for &viewer in viewers {
                    if arm_final_seat(
                        seats,
                        archive,
                        now_ms,
                        viewer,
                        final_pending.focus,
                        final_pending.victim,
                        &final_pending.weapon,
                        final_pending.killcam_entity_start_time,
                        final_pending.entity_focus,
                        start,
                        stats,
                        begin,
                    ) {
                        stats.final_seats_armed = stats.final_seats_armed.saturating_add(1);
                        killedby_cards.push((viewer, final_pending.focus));
                    } else {
                        stats.final_seats_refused = stats.final_seats_refused.saturating_add(1);
                    }
                }
            }
            FinalKillcamOutput::Done => {
                stats.final_killcam_done = stats.final_killcam_done.saturating_add(1);
            }
            FinalKillcamOutput::ShowingFinalKillcam => {}
        }
    }
    let showing = final_pending.seq.showing();
    if !final_pending.seq.is_done() {
        pending.final_kc = Some(final_pending);
    }
    showing
}

fn arm_final_seat(
    seats: &mut ActiveKillcams,
    archive: &FrameArchive,
    now_ms: i32,
    viewer: ClientId,
    focus: ClientId,
    victim: ClientId,
    weapon: &str,
    killcam_entity_start_time: i32,
    entity_focus: Option<killcam_iw4::focus::Entity>,
    start: FinalKillcamStart,
    stats: &mut ScriptKillcamEmitStats,
    begin: &mut Vec<ClientId>,
) -> bool {
    let Some(mut session) = session_from_final_killcam(
        archive,
        now_ms,
        focus,
        weapon,
        start,
        killcam_entity_start_time,
    ) else {
        return false;
    };
    session.focus.kill_cam_look_at_entity = victim.0 as i32;
    session.entity_focus = entity_focus;
    update_entity_focus(&mut session, now_ms);
    seats.arm(viewer, session);

    begin.push(viewer);
    stats.begin_killcam = stats.begin_killcam.saturating_add(1);
    stats.seats_armed = stats.seats_armed.saturating_add(1);
    true
}

pub fn tick_phase_b_skips(
    seats: &mut ActiveKillcams,
    skips: &mut ActiveKillcamSkips,
    cmds: &[(ClientId, UserCmd)],
    stats: &mut ScriptKillcamEmitStats,
    abort: &mut Vec<ClientId>,
    ended: &mut Vec<ClientId>,
) {
    let viewers: Vec<ClientId> = seats.viewers();
    for viewer in viewers {
        if seats.get(viewer).is_some_and(|s| s.final_kill) {
            continue;
        }
        let pressed = use_button_pressed(cmds, viewer);
        let Some(skip) = skips.get_mut(viewer) else {
            skips.arm(viewer);
            continue;
        };
        if skip.tick(pressed) == CancelTick::Fired {
            abort_seat(seats, skips, viewer, stats, abort, ended);
            stats.phase_b_aborted = stats.phase_b_aborted.saturating_add(1);
        }
    }
}

pub fn abort_killcam_on_use_copycat(
    seats: &mut ActiveKillcams,
    skips: &mut ActiveKillcamSkips,
    actions: &[(ClientId, ClientAction)],
    stats: &mut ScriptKillcamEmitStats,
    abort: &mut Vec<ClientId>,
    ended: &mut Vec<ClientId>,
) {
    for (id, action) in actions {
        if !matches!(action, ClientAction::UseCopycat { .. }) {
            continue;
        }
        if seats.get(*id).is_none_or(|seat| seat.final_kill) {
            continue;
        }
        abort_seat(seats, skips, *id, stats, abort, ended);
    }
}

pub fn follow_archived_focus(archive: &FrameArchive, session: &mut KillcamSession) {
    loop {
        let lookup = archive.lookup(session.archivetime_ms.max(0));
        let found = lookup.tick.and_then(|tick| archive.frame(tick));
        session.archivetime_ms = if found.is_some() {
            lookup.attained_ms
        } else {
            0
        };
        if found.is_some_and(|f| f.player_state_exists(session.focus_client)) {
            return;
        }
        if session.archivetime_ms <= 0 {
            session.archivetime_ms = 0;
            return;
        }
        session.archivetime_ms = (session.archivetime_ms - ARCHIVE_TICK_MS).max(0);
    }
}

pub fn watch_nothing_to_show(
    seats: &mut ActiveKillcams,
    skips: &mut ActiveKillcamSkips,
    archive: &FrameArchive,
    now_ms: i32,
    stats: &mut ScriptKillcamEmitStats,
    abort: &mut Vec<ClientId>,
    ended: &mut Vec<ClientId>,
) {
    for viewer in seats.viewers() {
        let Some(mut session) = seats.get(viewer).cloned() else {
            continue;
        };
        follow_archived_focus(archive, &mut session);
        if session.recalc_pending && now_ms >= session.started_at_ms + SERVER_FRAME_MS {
            session.recalc_pending = false;
            match recalc_after_first_frame(
                session.archivetime_ms as f32 / 1000.0,
                session.killcamoffset_ms as f32 / 1000.0,
                session.predelay_ms as f32 / 1000.0,
                session.postdelay_ms as f32 / 1000.0,
            ) {
                Recalc::ArchiveGrew => {}
                Recalc::Cancel => {
                    seats.clear(viewer);
                    skips.clear(viewer);
                    ended.push(viewer);
                    stats.killcam_ended = stats.killcam_ended.saturating_add(1);
                    continue;
                }
                Recalc::Continue {
                    camtime,
                    killcamlength,
                    ..
                } => {
                    session.ends_at_ms = session
                        .started_at_ms
                        .saturating_add(killcam_seconds_to_ms(killcamlength));
                    session.kc_timer_ends_at_ms =
                        now_ms.saturating_add(killcam_seconds_to_ms(camtime));
                }
            }
        }
        let archivetime_s = session.archivetime_ms as f32 / 1000.0;
        if end_killcam_if_nothing_to_show(archivetime_s) == NothingToShow::Abort {
            abort_seat(seats, skips, viewer, stats, abort, ended);
            continue;
        }
        update_entity_focus(&mut session, now_ms);
        seats.arm(viewer, session);
    }
}

pub fn expire_with_notify(
    seats: &mut ActiveKillcams,
    skips: &mut ActiveKillcamSkips,
    now_ms: i32,
    stats: &mut ScriptKillcamEmitStats,
    ended: &mut Vec<ClientId>,
) {
    for client in seats.expire_clients(now_ms) {
        skips.clear(client);
        finish_cleanup(
            CleanupEntry::NormalEnd { clear_state: true },
            client,
            stats,
            ended,
        );
    }
}

fn abort_seat(
    seats: &mut ActiveKillcams,
    skips: &mut ActiveKillcamSkips,
    viewer: ClientId,
    stats: &mut ScriptKillcamEmitStats,
    abort: &mut Vec<ClientId>,
    ended: &mut Vec<ClientId>,
) {
    let final_kill = seats.get(viewer).is_some_and(|s| s.final_kill);
    seats.clear(viewer);
    skips.clear(viewer);
    abort.push(viewer);
    stats.abort_killcam = stats.abort_killcam.saturating_add(1);
    if !final_kill {
        finish_cleanup(
            CleanupEntry::NormalEnd { clear_state: true },
            viewer,
            stats,
            ended,
        );
    }
}

pub fn end_ordinary_killcams_on_game_ended(
    seats: &mut ActiveKillcams,
    skips: &mut ActiveKillcamSkips,
    level_notifies: &[NotifyKind],
    stats: &mut ScriptKillcamEmitStats,
    ended: &mut Vec<ClientId>,
) {
    if !level_notifies.contains(&NotifyKind::GameEnded) {
        return;
    }
    for viewer in seats.viewers() {
        if seats.get(viewer).is_some_and(|s| s.final_kill) {
            continue;
        }
        seats.clear(viewer);
        skips.clear(viewer);
        finish_cleanup(
            CleanupEntry::GameEnded { clear_state: true },
            viewer,
            stats,
            ended,
        );
    }
}

fn finish_cleanup(
    entry: CleanupEntry,
    client: ClientId,
    stats: &mut ScriptKillcamEmitStats,
    ended: &mut Vec<ClientId>,
) {
    let mut buf = [CleanupStep::HideHud; 6];
    let n = killcam_cleanup_steps(entry, &mut buf);
    for step in buf.iter().take(n) {
        match step {
            CleanupStep::NotifyKillcamEnded => {
                ended.push(client);
                stats.killcam_ended = stats.killcam_ended.saturating_add(1);
            }
            CleanupStep::HideHud
            | CleanupStep::ClearKillcamFlag
            | CleanupStep::ClearLowerMessage
            | CleanupStep::RestoreSpectatePermissions
            | CleanupStep::ClearKillcamState => {}
        }
    }
}

fn update_entity_focus(session: &mut KillcamSession, now_ms: i32) {
    let Some(entity) = session.entity_focus else {
        return;
    };
    let offset = if session.recalc_pending {
        session.killcamoffset_ms
    } else {
        session.archivetime_ms
    };
    if entity.birthtime.unwrap_or(0) <= now_ms.saturating_sub(offset) {
        session.focus.kill_cam_entity = entity.entity_number;
    }
}
