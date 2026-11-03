use bevy::prelude::*;
use render_frontend::{
    AddWorkerCmd, CELL_VISIT_FIRST, CELL_VISIT_FRUSTUM, CellFrustumWorkerCmd,
    WORKER_CMD_CELL_DYN_BRUSH, WORKER_CMD_CELL_DYN_MODEL, WORKER_CMD_CELL_SCENE_ENT,
    enqueue_cell_frustum_cmds,
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
            .in_set(frame::InMatch)
            .after(crate::prepare::scene::cull::apply_dpvs_cull)
            .in_set(frame::WorkerCmdSet::WorldVisibility),
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
    let frustum_n = u8::try_from(stats.frustum_planes.len()).unwrap_or(u8::MAX);
    let mut enqueue =
        |cmd: CellFrustumWorkerCmd| match enqueue_cell_frustum_cmds(&mut worker_cmds.queues, cmd) {
            Ok(adds) if adds.contains(&AddWorkerCmd::OverflowInline) => false,
            Err(_) => false,
            Ok(_) => true,
        };
    let visits = &stats.cell_clip_visits;
    if !visits.is_empty() && visits.len() <= CELL_VISIT_BUDGET {
        for (index, (cell, clip)) in visits.iter().enumerate() {
            let cell_index = *cell as usize;
            let visible = cell_index < stats.cell_vis_count
                && stats
                    .cell_vis
                    .get(cell_index / 32)
                    .is_some_and(|word| word & (1 << (cell_index % 32)) != 0);
            if !visible || clip.plane_count == 0 {
                continue;
            }
            let cmd = CellFrustumWorkerCmd {
                visit: index as u32 + 1,
                cell: *cell,
                plane_count: clip.plane_count,
                plane_begin: clip.frustum_plane_count,
                view: 0,
            };
            if !enqueue(cmd) {
                break;
            }
        }
        return;
    }
    for cell in admitted_cell_indices(&stats.cell_vis, stats.cell_vis_count) {
        let first = stats
            .cell_clips
            .get(cell as usize)
            .filter(|c| c.plane_count > 0 && visits.is_empty());
        let cmd = match first {
            Some(c) => CellFrustumWorkerCmd {
                visit: CELL_VISIT_FIRST,
                cell,
                plane_count: c.plane_count,
                plane_begin: c.frustum_plane_count,
                view: 0,
            },
            None => CellFrustumWorkerCmd {
                visit: CELL_VISIT_FRUSTUM,
                cell,
                plane_count: frustum_n,
                plane_begin: frustum_n,
                view: 0,
            },
        };
        if !enqueue(cmd) {
            break;
        }
    }
}

const CELL_VISIT_BUDGET: usize = 480;
