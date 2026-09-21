use fx_iw4::{fx_existing_elem_sorts_before_new, fx_sort_dist_to_cam_sq};

use crate::elem::{FX_ELEM_HANDLE_NONE, elem_slot_for_handle};
use crate::spark::spark_elem_world_origin;
use crate::system::FxSystemHost;

pub fn sort_new_elems_in_effect(
    host: &mut FxSystemHost,
    effect_slot: usize,
    camera_origin: [f32; 3],
) {
    let (mut pending, stop) = match host.effect_at(effect_slot) {
        Some(e) => (e.first_elem_handle[0], e.first_sorted_elem_handle),
        None => return,
    };
    if pending == stop {
        return;
    }

    // Distances are valid only during this insertion pass: positions, effect
    // frames and camera stay fixed while we change list links.
    host.sort_epoch = host.sort_epoch.wrapping_add(1);
    if host.sort_epoch == 0 {
        host.sort_distances.fill((0, 0.0));
        host.sort_epoch = 1;
    }

    if let Some(effect) = host.effect_at_mut(effect_slot) {
        effect.first_elem_handle[0] = stop;
    }
    if stop != FX_ELEM_HANDLE_NONE {
        if let Some(stop_slot) = elem_slot_for_handle(stop) {
            if let Some(e) = host.elems.get_mut(stop_slot) {
                e.prev_elem_handle = FX_ELEM_HANDLE_NONE;
            }
        }
    }

    while pending != stop {
        let Some(pend_slot) = elem_slot_for_handle(pending) else {
            break;
        };
        let next = host
            .elems
            .get(pend_slot)
            .map(|e| e.next_elem_handle)
            .unwrap_or(FX_ELEM_HANDLE_NONE);

        if let Some(e) = host.elems.get_mut(pend_slot) {
            e.next_elem_handle = FX_ELEM_HANDLE_NONE;
            e.prev_elem_handle = FX_ELEM_HANDLE_NONE;
        }
        sort_sprite_elem_into_effect(host, effect_slot, pending, camera_origin);
        pending = next;
    }

    if let Some(effect) = host.effect_at_mut(effect_slot) {
        effect.first_sorted_elem_handle = effect.first_elem_handle[0];
    }
}

fn sort_sprite_elem_into_effect(
    host: &mut FxSystemHost,
    effect_slot: usize,
    new_handle: u16,
    camera_origin: [f32; 3],
) {
    let Some(new_slot) = elem_slot_for_handle(new_handle) else {
        return;
    };
    let new_sort = match host.elems.get(new_slot) {
        Some(e) if e.occupied => e.sort_order,
        _ => return,
    };
    let Some(new_dist) = sort_distance(host, effect_slot, new_slot, camera_origin) else {
        return;
    };

    let mut prev_handle = FX_ELEM_HANDLE_NONE;
    let mut cursor = match host.effect_at(effect_slot) {
        Some(e) => e.first_elem_handle[0],
        None => return,
    };

    while cursor != FX_ELEM_HANDLE_NONE {
        let Some(cur_slot) = elem_slot_for_handle(cursor) else {
            break;
        };
        let (ex_type, ex_vis, ex_sort, next) = match host.elems.get(cur_slot) {
            Some(e) if e.occupied => (
                e.elem_type,
                e.visual_count,
                e.sort_order,
                e.next_elem_handle,
            ),
            _ => break,
        };
        // Type, visual count and authored order are compared before distance.
        let ex_dist = if ex_type <= 3 && ex_vis != 0 && ex_sort == new_sort {
            let Some(distance) = sort_distance(host, effect_slot, cur_slot, camera_origin) else {
                break;
            };
            distance
        } else {
            0.0
        };
        if !fx_existing_elem_sorts_before_new(ex_type, ex_vis, ex_sort, ex_dist, new_sort, new_dist)
        {
            break;
        }
        prev_handle = cursor;
        cursor = next;
    }

    if let Some(e) = host.elems.get_mut(new_slot) {
        e.next_elem_handle = cursor;
        e.prev_elem_handle = prev_handle;
    }
    if prev_handle == FX_ELEM_HANDLE_NONE {
        if let Some(effect) = host.effect_at_mut(effect_slot) {
            effect.first_elem_handle[0] = new_handle;
        }
    } else if let Some(prev_slot) = elem_slot_for_handle(prev_handle) {
        if let Some(e) = host.elems.get_mut(prev_slot) {
            e.next_elem_handle = new_handle;
        }
    }
    if cursor != FX_ELEM_HANDLE_NONE {
        if let Some(cur_slot) = elem_slot_for_handle(cursor) {
            if let Some(e) = host.elems.get_mut(cur_slot) {
                e.prev_elem_handle = new_handle;
            }
        }
    }
}

fn sort_distance(
    host: &mut FxSystemHost,
    effect_slot: usize,
    elem_slot: usize,
    camera_origin: [f32; 3],
) -> Option<f32> {
    let (epoch, distance) = host.sort_distances[elem_slot];
    if epoch == host.sort_epoch {
        return Some(distance);
    }
    let elem = host.elems.get(elem_slot)?;
    let effect = host.effect_at(effect_slot)?;
    let seed = fx_iw4::fx_elem_random_seed(effect.random_seed, elem.sequence, elem.msec_begin);
    let world = spark_elem_world_origin(
        elem.origin,
        elem.flags,
        &effect.frame_now(),
        &effect.frame_when_played(),
        Some(elem.orient_spawn_params(seed)),
    );
    let distance = fx_sort_dist_to_cam_sq(camera_origin, world);
    host.sort_distances[elem_slot] = (host.sort_epoch, distance);
    Some(distance)
}
