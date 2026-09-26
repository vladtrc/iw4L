use crate::bullet_collision::EntityCollisionCapabilities;
use crate::gentity::{ScriptMoverGentity, apos_from_entity_state, pos_from_entity_state};
use crate::identities::ScriptModelId;
use entity_iw4::{CG_SCRIPT_MOVER_NODRAW, bg_evaluate_trajectory};
use std::collections::BTreeMap;

pub(crate) fn follow_movers(
    capabilities: &mut [EntityCollisionCapabilities],
    movers: &[ScriptMoverGentity],
    time_ms: i32,
) {
    let by_id: BTreeMap<ScriptModelId, &ScriptMoverGentity> =
        movers.iter().map(|mover| (mover.id, mover)).collect();
    for row in capabilities {
        let Some(mover) = row.owner.script_model().and_then(|id| by_id.get(&id)) else {
            continue;
        };
        row.hidden = mover.state.e_flags & CG_SCRIPT_MOVER_NODRAW != 0;
        row.solid = !mover.nonsolid;
        let pose = (
            bg_evaluate_trajectory(&pos_from_entity_state(&mover.state), time_ms),
            bg_evaluate_trajectory(&apos_from_entity_state(&mover.state), time_ms),
        );
        match row.followed_pose {
            None => row.followed_pose = Some(pose),
            Some(followed) if followed == pose => {}
            Some(_) => {
                row.followed_pose = Some(pose);
                if let Some(dobj) = row.dobj.as_mut() {
                    dobj.set_world_pose(pose.0, pose.1);
                }
                for brush in &mut row.linked_brushes {
                    brush.origin = pose.0;
                    brush.angles = pose.1;
                }
            }
        }
    }
}
