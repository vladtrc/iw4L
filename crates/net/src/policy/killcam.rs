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
            .filter(|s| {
                s.focus_client == focus_client
                    && (s.killcamoffset_ms == seat.archive_ms
                        || s.archivetime_ms == seat.archive_ms)
            })
            .map_or(now_ms, |s| s.started_at_ms);
        if started_at_ms == now_ms {
            diag::info!(
                Net,
                "killcam: script seat viewer={} spectating={} entity={} archive_ms={} length_ms={} (archive holds {}ms, {}B)",
                viewer.0,
                seat.spectator_client,
                seat.kill_cam_entity,
                seat.archive_ms,
                seat.length_ms,
                archive.duration_ms(),
                archive.bytes()
            );
        }
        let focus_life = seats
            .get(*viewer)
            .filter(|s| s.started_at_ms == started_at_ms)
            .and_then(|s| s.focus_life)
            .or_else(|| {
                let snapshot = &archive.newest()?.snapshot;
                snapshot
                    .meta
                    .journal
                    .iter()
                    .rev()
                    .find_map(|record| match record.event {
                        sim::SimEvent::Died {
                            victim,
                            attacker,
                            attacker_life,
                            life_sequence,
                            ..
                        } if victim == *viewer
                            && attacker == Some(focus_client)
                            && snapshot
                                .meta
                                .for_client(*viewer)
                                .is_some_and(|meta| meta.life_sequence == life_sequence) =>
                        {
                            attacker_life
                        }
                        _ => None,
                    })
                    .or_else(|| {
                        snapshot
                            .meta
                            .for_client(focus_client)
                            .map(|m| m.life_sequence)
                    })
            });
        let mut session = KillcamSession {
            archivetime_ms: seat.archive_ms,
            focus_client,
            focus_life,
            focus,
            ends_at_ms: i32::MAX,
            final_kill: seat.final_kill && seat.archive_ms > 0,
            started_at_ms,
            killcamoffset_ms: seat.archive_ms,
            predelay_ms: 0,
            postdelay_ms: 0,
            recalc_pending: false,
            entity_focus: None,
        };
        if seat.archive_ms > 0 {
            follow_archived_focus(archive, &mut session);
        }
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
        if found.is_some_and(|f| {
            f.player_state_exists(session.focus_client)
                && session.focus_life.is_none_or(|life| {
                    f.snapshot
                        .meta
                        .for_client(session.focus_client)
                        .is_some_and(|meta| meta.life_sequence == life)
                })
        }) {
            return;
        }
        if session.archivetime_ms <= 0 {
            session.archivetime_ms = 0;
            return;
        }
        session.archivetime_ms = (session.archivetime_ms - ARCHIVE_TICK_MS).max(0);
    }
}
