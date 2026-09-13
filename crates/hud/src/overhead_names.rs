use bevy::prelude::*;
use hud_iw4::{SCREEN_BLEND_FLASHED, cg_is_flashbanged};
use net::{CEntity, CEntityRuntime, CgFrameClock, LocalPresentClient, PresentedSnapshot};

use std::collections::HashMap;

use crate::gaps::{GapCause, HudGap, HudPresentationGaps};

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum OverheadPosedHead {
    ExactWorld([f32; 3]),
    NoDObjOrHead,
}

#[derive(Resource, Debug, Default)]
pub struct OverheadPosedPlayerFrame {
    by_ent: HashMap<u16, OverheadPosedHead>,
}

impl OverheadPosedPlayerFrame {
    pub fn replace(&mut self, rows: impl IntoIterator<Item = (u16, OverheadPosedHead)>) {
        self.by_ent.clear();
        self.by_ent.extend(rows);
    }

    fn get(&self, entnum: u16) -> Option<OverheadPosedHead> {
        self.by_ent.get(&entnum).copied()
    }
}

#[derive(SystemSet, Debug, Clone, PartialEq, Eq, Hash)]
pub struct OverheadPosedPlayerFramePublished;

pub(crate) fn register(app: &mut App) {
    app.init_resource::<OverheadPosedPlayerFrame>().add_systems(
        Update,
        update_overhead_name_readiness
            .after(OverheadPosedPlayerFramePublished)
            .before(crate::gaps::report_hud_gaps)
            .in_set(frame::LifeFrontPublished),
    );
}

fn update_overhead_name_readiness(
    presented: Res<PresentedSnapshot>,
    local: Res<LocalPresentClient>,
    posed: Res<OverheadPosedPlayerFrame>,
    players: Query<(&CEntity, &CEntityRuntime)>,
    cg_clock: Res<CgFrameClock>,
    mut gaps: ResMut<HudPresentationGaps>,
) {
    gaps.clear(HudGap::OverheadNames);
    let Some(snapshot) = presented.snapshot() else {
        return;
    };
    let Some(ps) = presented.player(local.0) else {
        gaps.raise(GapCause::OverheadFlashUnavailable);
        return;
    };
    let local_flashbanged = cg_is_flashbanged(
        cg_clock.time(),
        ps.shellshock_time,
        ps.shellshock_duration,
        SCREEN_BLEND_FLASHED,
    ) != 0;
    for (identity, runtime) in &players {
        if !runtime.in_next_snap()
            || runtime.next_state.e_type != entity_iw4::ET_PLAYER
            || (runtime.next_state.e_flags & 0x20) != 0
        {
            continue;
        }
        let Some(client) = identity.client() else {
            gaps.raise(GapCause::OverheadNoPresentedClient {
                client: u32::from(identity.number()),
            });
            return;
        };
        if client == local.0 {
            continue;
        }
        let Some(meta) = snapshot.meta.for_client(client) else {
            gaps.raise(GapCause::OverheadNoPresentedClient { client: client.0 });
            return;
        };
        if entity_iw4::client_state_name(&meta.name).is_none() {
            gaps.raise(GapCause::OverheadNoPresentedClient { client: client.0 });
            return;
        }
        let entnum = identity.number();
        if posed.get(entnum).is_none() {
            gaps.raise(GapCause::OverheadHeadUnavailable { entnum });
            return;
        }

        if local_flashbanged {
            continue;
        }
        gaps.raise(GapCause::OverheadVisibilityUnavailable);
        return;
    }
}
