use crate::script::{RoundScript, Value};
use bevy_ecs::prelude::World;

type V3 = [f32; 3];

fn candidate(origin: V3, node: &asset_core::PathNode) -> Option<f32> {
    if node.kind == 0 || node.kind == 20 || node.kind >= 32 {
        return None;
    }
    let distance = (origin[0] - node.origin[0]).powi(2) + (origin[1] - node.origin[1]).powi(2);
    (distance <= 192. * 192. && (origin[2] - 120. - node.origin[2]).powi(2) <= 184. * 184.)
        .then_some(distance)
}

fn contains(origin: V3, node: V3) -> bool {
    let bounds = super::super::actors::spatial::ACTOR_BOUNDS;
    (0..3).all(|i| (origin[i] - node[i] - bounds.midpoint[i]).abs() <= bounds.half_size[i] + 1.)
}

fn choose(
    origin: V3,
    candidates: &[(u16, f32, V3, bool)],
    visible: &mut impl FnMut(V3) -> bool,
) -> Option<u16> {
    for linked in [true, false] {
        for &(index, _, end, has_links) in candidates {
            if has_links == linked && ((linked && contains(origin, end)) || visible(end)) {
                return Some(index);
            }
        }
    }
    None
}

fn select_at(world: &mut World, origin: V3) -> (Option<u16>, Option<u16>) {
    let mut candidates = {
        let runtime = world.resource::<RoundScript>();
        runtime
            .path_nodes
            .nodes()
            .iter()
            .enumerate()
            .filter_map(|(index, node)| {
                candidate(origin, node).map(|distance| {
                    (
                        index as u16,
                        distance,
                        node.origin,
                        runtime.path_nodes.links[index]
                            .iter()
                            .any(|link| link.disconnect_count == 0),
                    )
                })
            })
            .collect::<Vec<_>>()
    };
    candidates.sort_by(|a, b| a.1.total_cmp(&b.1));
    let mut bounds = super::super::actors::spatial::ACTOR_BOUNDS;
    bounds.midpoint[2] = 50.;
    bounds.half_size[2] = 27.5;
    let (mins, maxs) = bounds.abs_bounds([0.; 3]);
    let selected = choose(origin, &candidates, &mut |end| {
        let hit = super::super::trace::capsule(
            world,
            movement_iw4::GroundTraceInput {
                start: origin,
                end,
                mins,
                maxs,
                tracemask: 0x820015,
            },
            &[],
        );
        hit.fraction == 1. && hit.startsolid == 0 && hit.allsolid == 0
    });
    (selected, candidates.first().map(|row| row.0))
}

pub(crate) fn query_position(world: &mut World, origin: V3) -> Option<u16> {
    let (selected, fallback) = select_at(world, origin);
    selected.or(fallback)
}

pub(crate) fn query(world: &mut World, id: u64) -> Result<Option<u16>, String> {
    let (origin, previous) = {
        let mut runtime = world.resource_mut::<RoundScript>();
        let sentient = runtime
            .sentients
            .get(&id)
            .ok_or("entity is not a sentient")?;
        if sentient.nearest_node_valid {
            return Ok(sentient.nearest_node);
        }
        let previous = sentient.nearest_node;
        let Value::Vector(origin) = runtime.object_field(id, "origin") else {
            return Err("sentient origin is not a vector".into());
        };
        (origin, previous)
    };
    let (selected, fallback) = select_at(world, origin);
    let sentient = world
        .resource_mut::<RoundScript>()
        .into_inner()
        .sentients
        .get_mut(&id)
        .unwrap();
    sentient.nearest_node_bad = selected.is_none();
    sentient.nearest_node = selected.or(fallback).or(previous);
    sentient.nearest_node_valid = true;
    Ok(sentient.nearest_node)
}
