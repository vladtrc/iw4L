use gamemode_iw4::destructible::PICKUP_GLASS_PARTS;
use gamemode_iw4::{VehicleBodyState, apply_destructible_part_player_bullet};

use crate::frame::FrameWorld;
use crate::{AuthorityDObjState, AuthorityModelOwner, EventAudience, Tick};

pub(crate) fn install(dobj: &mut AuthorityDObjState) {
    dobj.pickup_glass = Some(PICKUP_GLASS_PARTS.map(|part| VehicleBodyState {
        state_index: 0,
        health: part.health[0],
    }));
    update_hide_parts(dobj);
}

fn update_hide_parts(dobj: &mut AuthorityDObjState) {
    let (Some(cap), Some(states)) = (&dobj.capability, dobj.pickup_glass) else {
        return;
    };
    let mut words = *dobj.semantic_state.hide_part_bits.words();
    for (part, state) in PICKUP_GLASS_PARTS.iter().zip(states) {
        for (tag, hidden) in [
            (part.tag, state.state_index != 0),
            (part.damaged_tag, state.state_index != 1),
        ] {
            if let Some(bone) = cap.pose.bone_names.iter().position(|name| name == tag) {
                let bit = 0x8000_0000 >> (bone % 32);
                if hidden {
                    words[bone / 32] |= bit;
                } else {
                    words[bone / 32] &= !bit;
                }
            }
        }
    }
    let hide = xmodel_runtime::HidePartBits::from_words(words);
    if hide != dobj.semantic_state.hide_part_bits {
        dobj.semantic_state.hide_part_bits = hide;
        dobj.pose_request.hide_part_bits = hide;
        dobj.pose_revision = dobj.pose_revision.wrapping_add(1);
        dobj.semantic_state.pose_revision = dobj.pose_revision;
        dobj.current_collision = None;
        dobj.materialized_pose_revision = None;
    }
}

pub(crate) fn apply_hit(
    world: &mut FrameWorld,
    tick: Tick,
    owner: AuthorityModelOwner,
    bone: u16,
    damage: u32,
) -> bool {
    let Some(dobj) = world
        .entity_collision_capabilities_mut()
        .iter_mut()
        .find(|row| row.owner == owner)
        .and_then(|row| row.dobj.as_mut())
    else {
        return false;
    };
    let (Some(cap), Some(states)) = (&dobj.capability, &mut dobj.pickup_glass) else {
        return false;
    };
    let Some(tag) = cap.pose.bone_names.get(usize::from(bone)) else {
        return false;
    };
    let Some(index) = PICKUP_GLASS_PARTS
        .iter()
        .zip(states.iter())
        .position(|(part, state)| match state.state_index {
            0 => tag == part.tag,
            1 => tag == part.damaged_tag,
            _ => false,
        })
    else {
        return false;
    };
    let part = PICKUP_GLASS_PARTS[index];
    let old = states[index];
    let next = apply_destructible_part_player_bullet(&part.health, 2, old, damage);
    states[index] = next;

    let shatter = (old.state_index < 2 && next.state_index == 2)
        .then(|| {
            let bone = cap
                .pose
                .bone_names
                .iter()
                .position(|name| name == part.fx_tag)?;
            let pose = cap.pose(&dobj.pose_request, dobj.world_from_model).ok()?;
            let matrix = *pose.get(bone)?;
            Some((
                matrix.w_axis.truncate().to_array(),
                matrix.x_axis.truncate().normalize().to_array(),
            ))
        })
        .flatten();
    update_hide_parts(dobj);
    if let Some((origin, direction)) = shatter {
        for (kind, event_parm) in [
            (
                entity_iw4::EntityEventKind::PLAY_FX,
                i32::from(world.effect_name_index(part.fx)),
            ),
            (
                entity_iw4::EntityEventKind::SOUND_ALIAS,
                i32::from(world.sound_alias_index("veh_glass_break_large")),
            ),
        ] {
            world.push_entity_event(
                tick,
                EventAudience::All,
                kind,
                crate::EntityEventPayload {
                    number: i32::from(trace_iw4::ENTITYNUM_WORLD),
                    event_parm,
                    origin,
                    direction,
                    ..Default::default()
                },
            );
        }
    }
    true
}
