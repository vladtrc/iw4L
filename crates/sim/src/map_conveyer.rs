use crate::frame::FrameWorld;
use crate::{ClientId, ClientLifecycle, PLAYER_MAXS, PLAYER_MINS};

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct RadiationConveyer {
    pub origin: [f32; 3],
    pub half: [f32; 3],
    pub vector: [f32; 3],
    pub cmodel: u32,
}

fn touching(origin: [f32; 3], ps_origin: [f32; 3], half: [f32; 3]) -> bool {
    for i in 0..3 {
        let a_min = ps_origin[i] + PLAYER_MINS[i];
        let a_max = ps_origin[i] + PLAYER_MAXS[i];
        let b_min = origin[i] - half[i];
        let b_max = origin[i] + half[i];
        if a_min > b_max || a_max < b_min {
            return false;
        }
    }
    true
}

pub(crate) fn advance(world: &mut FrameWorld) {
    let Some(belt) = world.radiation_conveyer else {
        return;
    };
    let ids: Vec<ClientId> = world.client_ids_sorted();
    for id in ids {
        if !world
            .client_meta(id)
            .is_some_and(|m| m.lifecycle == ClientLifecycle::Alive)
        {
            continue;
        }
        let Some(ps) = world.player(id) else {
            continue;
        };
        if !touching(belt.origin, ps.origin, belt.half) {
            continue;
        }
        let on_ground = ps.ground_entity_num != playerstate_iw4::ENTITYNUM_NONE;
        if !gamemode_iw4::radiation_conveyer_should_push(on_ground) {
            continue;
        }
        let Some(ps) = world.player_mut(id) else {
            continue;
        };
        ps.velocity[0] += belt.vector[0];
        ps.velocity[1] += belt.vector[1];
        ps.velocity[2] += belt.vector[2];
    }
}
