use crate::bullet_collision::EntityCollisionCapabilities;
use crate::gentity::{ScriptMoverGentity, apos_from_entity_state, pos_from_entity_state};
use crate::identities::ScriptModelId;
use entity_iw4::{CG_SCRIPT_MOVER_NODRAW, evaluate_trajectory};
use std::collections::BTreeMap;

pub(crate) fn follow_movers(
    capabilities: &mut [EntityCollisionCapabilities],
    movers: &[ScriptMoverGentity],
    time_ms: i32,
    content: &crate::SimContent,
) {
    let by_id: BTreeMap<ScriptModelId, &ScriptMoverGentity> =
        movers.iter().map(|mover| (mover.id, mover)).collect();
    for row in capabilities {
        if let Some(mover) = row.owner.script_model().and_then(|id| by_id.get(&id)) {
            follow_mover(row, mover, time_ms, content);
        }
    }
}

pub(crate) fn adopt_movers(
    rows: &mut Vec<EntityCollisionCapabilities>,
    movers: &[ScriptMoverGentity],
    time_ms: i32,
    content: &crate::SimContent,
) {
    let mut by_id: BTreeMap<ScriptModelId, (&ScriptMoverGentity, bool)> = movers
        .iter()
        .map(|mover| (mover.id, (mover, false)))
        .collect();
    rows.retain_mut(|row| {
        if let Some((mover, seen)) = row.owner.script_model().and_then(|id| by_id.get_mut(&id)) {
            *seen = true;
            follow_mover(row, mover, time_ms, content);
            true
        } else if row.dobj.is_some() || !row.linked_brushes.is_empty() {
            row.body = None;
            row.solid = false;
            row.hidden = true;
            row.followed_pose = None;
            true
        } else {
            false
        }
    });
    for (id, (mover, seen)) in by_id {
        if !seen && mover.body.is_some() {
            let mut row = EntityCollisionCapabilities::current_tick(
                crate::AuthorityModelOwner::ScriptModel(id),
                None,
                Vec::new(),
            );
            follow_mover(&mut row, mover, time_ms, content);
            rows.push(row);
        }
    }
    rows.sort_by_key(|row| row.owner);
}

pub(crate) fn follow_mover(
    row: &mut EntityCollisionCapabilities,
    mover: &ScriptMoverGentity,
    time_ms: i32,
    content: &crate::SimContent,
) {
    row.hidden = mover.state.e_flags & CG_SCRIPT_MOVER_NODRAW != 0;
    row.solid = !mover.nonsolid;
    let pose = (
        evaluate_trajectory(&pos_from_entity_state(&mover.state), time_ms),
        evaluate_trajectory(&apos_from_entity_state(&mover.state), time_ms),
    );
    row.body = mover.body.map(|bounds| crate::entity_body::EntityBodyPose {
        bounds,
        origin: pose.0,
        number: mover.state.number as u16,
    });
    let Some(model_pose) = content.simulation_model_transform(pose.0, pose.1) else {
        row.hidden = true;
        row.solid = false;
        row.followed_pose = None;
        return;
    };
    match row.followed_pose {
        Some(followed) if followed == pose => {}
        _ => {
            row.followed_pose = Some(pose);
            if let Some(dobj) = row.dobj.as_mut() {
                dobj.set_world_transform(model_pose);
            }
            for brush in &mut row.linked_brushes {
                brush.origin = pose.0;
                brush.angles = pose.1;
            }
        }
    }
}
