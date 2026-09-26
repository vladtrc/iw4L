use bevy::prelude::*;
use playerstate_iw4::SeatFocus;
use sim::ClientId;

use crate::policy::seat::{ActiveKillcams, KillcamSession};
use crate::transport::archive::{ARCHIVE_TICK_MS, FrameArchive};

pub(crate) fn play_script_seats(
    seats: &mut ActiveKillcams,
    archive: &FrameArchive,
    now_ms: i32,
    scripted: &[(ClientId, sim::ScriptSeat)],
) {
    for viewer in seats.viewers() {
        if !scripted.iter().any(|(id, _)| *id == viewer) {
            seats.clear(viewer);
        }
    }
    for (viewer, seat) in scripted {
        let focus_client = ClientId(seat.spectator_client as u32);
        let mut focus = SeatFocus::hitscan(seat.spectator_client);
        if seat.kill_cam_entity >= 0 {
            focus.kill_cam_entity = seat.kill_cam_entity;
        }
        if seat.look_at_entity >= 0 {
            focus.kill_cam_look_at_entity = seat.look_at_entity;
        }
        let started_at_ms = seats
            .get(*viewer)
            .filter(|s| s.focus_client == focus_client && s.killcamoffset_ms == seat.archive_ms)
            .map_or(now_ms, |s| s.started_at_ms);
        if started_at_ms == now_ms {
            diag::info!(
                Net,
                "killcam: script seat viewer={} spectating={} entity={} archive_ms={} length_ms={}",
                viewer.0,
                seat.spectator_client,
                seat.kill_cam_entity,
                seat.archive_ms,
                seat.length_ms
            );
        }
        let mut session = KillcamSession {
            archivetime_ms: seat.archive_ms,
            focus_client,
            focus,
            ends_at_ms: i32::MAX,
            final_kill: false,
            started_at_ms,
            killcamoffset_ms: seat.archive_ms,
            predelay_ms: 0,
            postdelay_ms: 0,
            recalc_pending: false,
            entity_focus: None,
        };
        follow_archived_focus(archive, &mut session);
        seats.arm(*viewer, session);
    }
}

pub(crate) fn follow_archived_focus(archive: &FrameArchive, session: &mut KillcamSession) {
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
