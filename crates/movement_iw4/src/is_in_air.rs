use playerstate_iw4::{ENTITYNUM_NONE, PlayerState};

use crate::Pml;

pub fn pm_is_in_air(ps: &PlayerState, pml: &Pml) -> bool {
    if ps.ground_entity_num != ENTITYNUM_NONE {
        return false;
    }
    if ps.pm_type == 1 || ps.pm_type == 7 {
        return false;
    }
    pml.almost_ground_plane == 0
}
