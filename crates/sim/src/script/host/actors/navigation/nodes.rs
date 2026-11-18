use super::*;

fn near(world: &mut World, id: u64, index: u16, require_angle: bool) -> Result<bool, String> {
    let mut runtime = world.resource_mut::<RoundScript>();
    let node = runtime.path_nodes.at(index).ok_or("invalid pathnode")?;
    let target = node.origin;
    // One body diameter is close enough to begin an arrival blend. An authored
    // use radius may allow a larger arrival region.
    let radius = node.radius.max(spatial::ACTOR_BOUNDS.half_size[0] * 2.0);
    let yaw = runtime.path_nodes.actor_angle(index);
    let Value::Vector(origin) = runtime.object_field(id, "origin") else {
        return Err("actor origin is not a vector".into());
    };
    if (origin[0] - target[0]).hypot(origin[1] - target[1]) > radius
        || (origin[2] - target[2]).abs() > spatial::ACTOR_BOUNDS.half_size[2] * 2.0
    {
        return Ok(false);
    }
    if require_angle && let Some(yaw) = yaw {
        let Value::Vector(angles) = runtime.object_field(id, "angles") else {
            return Err("actor angles are not a vector".into());
        };
        // Accept alignment that the orientation controller can finish in one tick.
        let tolerance =
            runtime.actors[&id].orientation.turn_rate.abs() * crate::MATCH_TICK_MS as f32;
        return Ok(math_iw4::angle_subtract(yaw, angles[1]).abs() <= tolerance);
    }
    Ok(true)
}

pub(super) fn register(registry: &mut NativeRegistry) {
    registry.register(Namespace::Method, "nearnode", |world, receiver, args| {
        let id = actor_id(world, receiver)?;
        let index = world
            .resource::<RoundScript>()
            .path_nodes
            .index(arg(args, 0)?)?;
        Ok(Value::Int(i32::from(near(world, id, index, false)?)))
    });
    for (name, require_angle) in [("nearclaimnode", false), ("nearclaimnodeandangle", true)] {
        // NativeRegistry uses function pointers; each entry selects its own policy.
        let native = if require_angle {
            near_claim_angle
        } else {
            near_claim
        };
        registry.register(Namespace::Method, name, native);
    }
    registry.register(
        Namespace::Method,
        "comparenodedirtopathdir",
        |world, receiver, args| {
            let id = actor_id(world, receiver)?;
            let runtime = world.resource::<RoundScript>();
            let node = runtime.path_nodes.node(arg(args, 0)?)?;
            let direction = runtime.actors[&id].navigation.preview.direction;
            let (sin, cos) = node.yaw.to_radians().sin_cos();
            let forward = if node.forward == [0.0; 2] {
                [cos, sin]
            } else {
                node.forward
            };
            Ok(Value::Int(i32::from(
                direction[0] * forward[0] + direction[1] * forward[1] > 0.0,
            )))
        },
    );
}

fn near_claim(world: &mut World, receiver: &Value, _: &[Value]) -> Result<Value, String> {
    near_claim_impl(world, receiver, false)
}
fn near_claim_angle(world: &mut World, receiver: &Value, _: &[Value]) -> Result<Value, String> {
    near_claim_impl(world, receiver, true)
}
fn near_claim_impl(
    world: &mut World,
    receiver: &Value,
    require_angle: bool,
) -> Result<Value, String> {
    let id = actor_id(world, receiver)?;
    let Some(index) = world.resource::<RoundScript>().actors[&id].claimed_node else {
        return Ok(Value::Int(0));
    };
    Ok(Value::Int(i32::from(near(
        world,
        id,
        index,
        require_angle,
    )?)))
}
