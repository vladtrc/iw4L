use bevy::prelude::*;
use render_frontend::{
    AddWorkerCmd, CellFrustumWorkerCmd, WORKER_CMD_CELL_DYN_BRUSH, WORKER_CMD_CELL_DYN_MODEL,
    WORKER_CMD_CELL_SCENE_ENT, enqueue_cell_frustum_cmds,
};

use crate::prepare::scene::cull::DpvsFrameStats;
use crate::prepare::scene::smodel_geom_cache::FrontendWorkerCmds;

#[must_use]
pub fn admitted_cell_indices(words: &[u32], cell_count: usize) -> Vec<u32> {
    let mut out = Vec::new();
    for (wi, &word) in words.iter().enumerate() {
        let mut w = word;
        while w != 0 {
            let bit = w.trailing_zeros();
            let cell = wi * 32 + bit as usize;
            w &= w.wrapping_sub(1);
            if cell < cell_count {
                out.push(cell as u32);
            }
        }
    }
    out
}

pub fn register_cell_frustum_cmds(app: &mut App) {
    app.add_systems(
        Update,
        enqueue_cell_frustum_cmds_after_vis
            .after(crate::prepare::scene::cull::apply_dpvs_cull)
            .in_set(frame::WorkerCmdSet::CellStatic),
    );
}

fn enqueue_cell_frustum_cmds_after_vis(
    stats: Option<Res<DpvsFrameStats>>,
    mut worker_cmds: ResMut<FrontendWorkerCmds>,
) {
    let _ = worker_cmds.queues.reset_type(WORKER_CMD_CELL_DYN_BRUSH);
    let _ = worker_cmds.queues.reset_type(WORKER_CMD_CELL_DYN_MODEL);
    let _ = worker_cmds.queues.reset_type(WORKER_CMD_CELL_SCENE_ENT);
    let Some(stats) = stats.as_deref() else {
        return;
    };
    let frustum_n = stats.frustum_planes.len();
    let cells = admitted_cell_indices(&stats.cell_vis, stats.cell_vis_count);
    for cell in cells {
        let (plane_count, plane_begin) = stats
            .cell_clips
            .get(cell as usize)
            .filter(|c| c.plane_count > 0)
            .map(|c| (c.plane_count, c.frustum_plane_count))
            .unwrap_or_else(|| {
                let n = u8::try_from(frustum_n).unwrap_or(u8::MAX);
                (n, n)
            });
        let cmd = CellFrustumWorkerCmd {
            cell,
            plane_count,
            plane_begin,
            view: 0,
        };
        match enqueue_cell_frustum_cmds(&mut worker_cmds.queues, cmd) {
            Ok(adds) if adds.iter().any(|a| *a == AddWorkerCmd::OverflowInline) => {
                break;
            }
            Err(_) => break,
            Ok(_) => {}
        }
    }
}
