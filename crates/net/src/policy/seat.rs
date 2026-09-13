use std::collections::HashMap;

use bevy::prelude::Resource;
use playerstate_iw4::{PlayerState, SeatFocus, apply_killcam_seat, rebase_archived_timers};
use sim::{ClientId, ClientLifecycle, Snapshot};

use crate::authority::inbox::AUTHORITY_MS;
use crate::player_state_to_entity_state;
use crate::transport::archive::{ARCHIVE_TICK_MS, ArchiveLookup, FrameArchive};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct KillcamSession {
    pub archivetime_ms: i32,

    pub focus_client: ClientId,

    pub focus: SeatFocus,

    pub ends_at_ms: i32,

    pub kc_info_tus_ms: i32,

    pub final_kill: bool,
}

#[derive(Resource, Clone, Debug, Default)]
pub struct ActiveKillcams {
    sessions: HashMap<ClientId, KillcamSession>,
}

impl ActiveKillcams {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn get(&self, viewer: ClientId) -> Option<&KillcamSession> {
        self.sessions.get(&viewer)
    }

    pub fn arm(&mut self, viewer: ClientId, session: KillcamSession) {
        self.sessions.insert(viewer, session);
    }

    pub fn clear(&mut self, viewer: ClientId) {
        self.sessions.remove(&viewer);
    }

    pub fn is_empty(&self) -> bool {
        self.sessions.is_empty()
    }

    pub fn expire_clients(&mut self, now_ms: i32) -> Vec<ClientId> {
        let mut expired = Vec::new();
        self.sessions.retain(|id, s| {
            if now_ms < s.ends_at_ms {
                true
            } else {
                expired.push(*id);
                false
            }
        });
        expired
    }

    pub fn viewers(&self) -> Vec<ClientId> {
        self.sessions.keys().copied().collect()
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct SeatSample {
    pub player_state: PlayerState,
    pub lookup: ArchiveLookup,

    pub rebase_ms: i32,
}

pub fn sample_killcam_seat(
    archive: &FrameArchive,
    snapshot_now: &Snapshot,
    viewer: ClientId,
    session: &KillcamSession,
    now_ms: i32,
) -> Option<SeatSample> {
    let lookup = archive.lookup(session.archivetime_ms);
    if lookup.nothing_to_show() {
        return None;
    }
    let tick = lookup.tick?;
    let frame = archive.frame(tick)?;
    let archived = frame
        .snapshot
        .players
        .iter()
        .find(|(id, _)| *id == session.focus_client)
        .map(|(_, ps)| *ps)?;

    let viewer_ps = snapshot_now
        .players
        .iter()
        .find(|(id, _)| *id == viewer)
        .map(|(_, ps)| *ps)
        .unwrap_or(PlayerState::ZERO);

    let frame_base_ms = i32::try_from(tick.0).ok()?.saturating_mul(ARCHIVE_TICK_MS);
    let rebase_ms = now_ms.saturating_sub(frame_base_ms);

    let mut remapped = archived;
    rebase_archived_timers(&mut remapped, rebase_ms);
    let player_state = apply_killcam_seat(&viewer_ps, &remapped, session.focus);

    Some(SeatSample {
        player_state,
        lookup,
        rebase_ms,
    })
}

pub fn apply_seat_to_snapshot(
    archive: &FrameArchive,
    snapshot: &mut Snapshot,
    viewer: ClientId,
    session: &KillcamSession,
    now_ms: i32,
) -> Option<SeatSample> {
    let sample = sample_killcam_seat(archive, snapshot, viewer, session, now_ms)?;
    if let Some((_, ps)) = snapshot.players.iter_mut().find(|(id, _)| *id == viewer) {
        *ps = sample.player_state;
    } else {
        snapshot.players.push((viewer, sample.player_state));
    }
    Some(sample)
}

pub fn snapshot_for_viewer(
    archive: &FrameArchive,
    seats: &ActiveKillcams,
    live: &Snapshot,
    viewer: ClientId,
    now_ms: i32,
) -> Snapshot {
    snapshot_and_sample_for_viewer(archive, seats, live, viewer, now_ms).0
}

pub fn snapshot_and_sample_for_viewer(
    archive: &FrameArchive,
    seats: &ActiveKillcams,
    live: &Snapshot,
    viewer: ClientId,
    now_ms: i32,
) -> (Snapshot, Option<SeatSample>) {
    let Some(session) = seats.get(viewer) else {
        return (live.clone(), None);
    };

    if !session.final_kill
        && live
            .meta
            .for_client(viewer)
            .is_some_and(|m| m.lifecycle == ClientLifecycle::Alive)
    {
        return (live.clone(), None);
    }
    let lookup = archive.lookup(session.archivetime_ms);
    let mut out = match lookup.tick.and_then(|tick| archive.frame(tick)) {
        Some(frame) => overlay_archived_world(live, &frame.snapshot, viewer),
        None => live.clone(),
    };
    let sample = apply_seat_to_snapshot(archive, &mut out, viewer, session, now_ms);
    let archived = lookup.tick.and_then(|tick| archive.frame(tick));
    overlay_killcam_hud(
        &mut out,
        live,
        archived.map(|f| &f.snapshot),
        viewer,
        session,
        sample.as_ref().map(|s| s.rebase_ms).unwrap_or(0),
    );
    (out, sample)
}

fn overlay_archived_world(live: &Snapshot, archived: &Snapshot, viewer: ClientId) -> Snapshot {
    let mut out = archived.clone();
    out.tick = live.tick;
    out.meta.phase = live.meta.phase;
    out.meta.match_elapsed_ms = live.meta.match_elapsed_ms;
    out.meta.score_limit = live.meta.score_limit;
    out.meta.time_limit_ms = live.meta.time_limit_ms;
    out.meta.kind = live.meta.kind;

    let archived_viewer = archived
        .players
        .iter()
        .find(|(id, _)| *id == viewer)
        .map(|(_, ps)| *ps);
    if let Some(ps) = archived_viewer {
        let es = player_state_to_entity_state(viewer, &ps);
        if !out.meta.entities.iter().any(|e| e.number == es.number) {
            out.meta.entities.push(es);
        }
    }

    let viewer_row = live.players.iter().find(|(id, _)| *id == viewer).cloned();
    out.players.retain(|(id, _)| *id != viewer);
    if let Some(row) = viewer_row {
        out.players.push(row);
    }

    let viewer_meta = live
        .meta
        .clients
        .iter()
        .find(|(id, _)| *id == viewer)
        .cloned();
    out.meta.clients.retain(|(id, _)| *id != viewer);
    if let Some(row) = viewer_meta {
        out.meta.clients.push(row);
    }
    out
}

fn overlay_killcam_hud(
    out: &mut Snapshot,
    live: &Snapshot,
    archived: Option<&Snapshot>,
    viewer: ClientId,
    session: &KillcamSession,
    rebase_ms: i32,
) {
    let current = live
        .meta
        .for_client(viewer)
        .map(|m| m.hud_current.clone())
        .unwrap_or_default();
    let wipe = session.focus.kill_cam_entity != playerstate_iw4::HITSCAN_KILL_CAM_ENTITY;
    let mut archival = if wipe {
        Vec::new()
    } else {
        archived
            .and_then(|snap| snap.meta.for_client(session.focus_client))
            .map(|m| m.hud_archival.clone())
            .unwrap_or_default()
    };
    sim::rebase_hud_archival(&mut archival, rebase_ms);
    if let Some((_, meta)) = out.meta.clients.iter_mut().find(|(id, _)| *id == viewer) {
        meta.killcam_hud = Some(sim::KillcamHud {
            final_kill: session.final_kill,
            time_until_respawn_ms: session.kc_info_tus_ms,
        });
        meta.hud_current = current;
        meta.hud_archival = archival;
    }
}

pub const fn killcam_seconds_to_ms(seconds: f32) -> i32 {
    let ms = seconds * 1000.0;
    let ticks = (ms / (AUTHORITY_MS as f32) + 0.5) as i32;
    ticks.saturating_mul(AUTHORITY_MS)
}
