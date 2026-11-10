use super::*;

pub(super) const FIELDS: [&str; 4] = [
    "upaimlimit",
    "downaimlimit",
    "rightaimlimit",
    "leftaimlimit",
];

fn limits(node: Option<&asset_core::PathNode>) -> Result<[f32; 4], String> {
    let Some(node) = node else {
        return Ok([45.0, -45.0, 45.0, -45.0]);
    };
    let (right, left) = match node.kind {
        2..=5 | 10..=12 => (45.0, -45.0),
        6 if node.spawnflags & 0x200 != 0 => (50.0, -45.0),
        7 if node.spawnflags & 0x200 != 0 => (45.0, -50.0),
        6 => (50.0, -15.0),
        7 => (15.0, -50.0),
        18 => return Err("turret pathnode aim limits require node-to-turret ownership".into()),
        0..=19 => (60.0, -60.0),
        _ => return Err("invalid path node type".into()),
    };
    Ok([45.0, -45.0, right, left])
}

pub(super) fn register(registry: &mut NativeRegistry) {
    registry.register(
        Namespace::Method,
        "setdefaultaimlimits",
        |world, receiver, args| {
            let id = actor_id(world, receiver)?;
            let runtime = world.resource::<RoundScript>();
            let node = args
                .first()
                .map(|value| runtime.path_nodes.node(value))
                .transpose()?;
            let limits = limits(node)?;
            world
                .resource_mut::<RoundScript>()
                .actors
                .get_mut(&id)
                .unwrap()
                .aim_limits = limits;
            Ok(Value::Undefined)
        },
    );
}
