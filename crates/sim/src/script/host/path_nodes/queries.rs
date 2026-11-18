use super::super::args::{float, optional, vector};
use super::*;

fn query(
    world: &mut bevy_ecs::prelude::World,
    args: &[Value],
    sorted: bool,
) -> Result<Value, String> {
    let origin = vector(args, 0)?;
    let radius = float(args, 1)?;
    let minimum = float(args, 2)?;
    // The fourth script parameter is vertical tolerance, not a result count.
    let height = optional(args, 3, float)?.unwrap_or(512.0);
    let kind = optional(args, 4, string)?;
    if !origin
        .into_iter()
        .chain([radius, minimum, height])
        .all(f32::is_finite)
        || radius < 0.0
        || minimum < 0.0
        || height < 0.0
    {
        return Err("node query dimensions must be finite and nonnegative".into());
    }
    let runtime = world.resource::<RoundScript>();
    let exact = kind.as_deref().and_then(|wanted| {
        (0..)
            .map_while(|kind| {
                PathNode {
                    kind,
                    ..Default::default()
                }
                .type_name()
                .map(|name| (kind, name))
            })
            .find(|(_, name)| name.eq_ignore_ascii_case(wanted))
            .map(|(kind, _)| kind)
    });
    let cover = kind
        .as_deref()
        .is_some_and(|kind| kind.eq_ignore_ascii_case("cover"));
    let mut nodes: Vec<_> = runtime
        .path_nodes
        .nodes()
        .iter()
        .enumerate()
        .filter_map(|(index, node)| {
            let name = node.type_name().unwrap_or("");
            let matches_kind = if cover {
                name.starts_with("Cover ")
                    || name.starts_with("Conceal ")
                    || matches!(name, "Exposed" | "Turret")
            } else {
                exact.is_none_or(|kind| node.kind == kind)
            };
            let distance = (node.origin[0] - origin[0]).hypot(node.origin[1] - origin[1]);
            (matches_kind
                && distance <= radius
                && distance >= minimum
                && (node.origin[2] - origin[2]).abs() <= height
                && runtime.path_nodes.links[index]
                    .iter()
                    .any(|link| link.disconnect_count == 0))
            .then_some((distance, index as u16))
        })
        .collect();
    if sorted {
        nodes.sort_by(|a, b| a.0.total_cmp(&b.0).then_with(|| a.1.cmp(&b.1)));
    }
    let values = nodes
        .into_iter()
        .map(|(_, index)| runtime.path_nodes.value(index))
        .collect();
    // Scan the retained graph directly: no copied spatial-tree layout, type
    // bitmask table or original engine's scratch-buffer truncation is needed.
    new_array(world, values)
}

pub(super) fn register(registry: &mut NativeRegistry) {
    registry.register(Namespace::Function, "getnodesinradius", |world, _, args| {
        query(world, args, false)
    });
    registry.register(
        Namespace::Function,
        "getnodesinradiussorted",
        |world, _, args| query(world, args, true),
    );
}
