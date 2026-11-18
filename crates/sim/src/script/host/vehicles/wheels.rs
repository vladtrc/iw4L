use super::*;

const MASK_WHEEL_PHYSICS: u32 = 0x0080_0211;
const MASK_WHEEL_STATIC: u32 = 0x211;

const NAMES: [&str; 6] = [
    "front_left",
    "front_right",
    "back_left",
    "back_right",
    "middle_left",
    "middle_right",
];

pub(super) fn register(registry: &mut NativeRegistry) {
    registry.register(Method, "getwheelsurface", |world, receiver, args| {
        let name = string(args, 0)?;
        let wheel = NAMES.iter().position(|candidate| *candidate == name)
            .ok_or("invalid wheel name: expected front_left, front_right, back_left, back_right, middle_left or middle_right")?;
        let vehicle = heli(world, receiver)?;
        let motion = vehicle.motion.ok_or("vehicle wheel contact inputs are unavailable")?;
        if motion.wheel_count == 0 {
            return Err("GetWheelSurface is not set up for this vehicle type".into());
        }
        if wheel >= usize::from(motion.wheel_count) {
            return Err("vehicle has no middle wheels".into());
        }
        let surface = usize::from(vehicle.wheel_surfaces[wheel]);
        Ok(Value::string(if surface == 0 { "none" } else {
            movement_iw4::SURFACE_TYPE_NAMES.get(surface).copied().ok_or("invalid wheel surface type")?
        }))
    });
}

pub(super) fn update(world: &mut World, id: u64) {
    let Some(motion) = world
        .resource::<RoundScript>()
        .vehicles
        .get(&id)
        .and_then(|vehicle| vehicle.motion)
    else {
        return;
    };
    if motion.wheel_count == 0 {
        return;
    }
    // Wheel tags and contact traces only read this pose. Settle once for the
    // batch instead of rebuilding every script entity's collision per wheel.
    super::super::presence::settle_collision(world);
    let exclude = world
        .resource::<RoundScript>()
        .entities
        .get(&id)
        .and_then(|entity| entity.presence)
        .map(crate::AuthorityModelOwner::ScriptModel);
    let angles = vec_field(&mut world.resource_mut::<RoundScript>(), id, "angles");
    let up = math_iw4::angles_to_axis(angles)[2];
    let mut surfaces = [0; 6];
    for wheel in 0..usize::from(motion.wheel_count) {
        let tag = format!("tag_wheel_{}", NAMES[wheel]);
        let Some((hub, _)) = super::super::presence::tag_world_settled(world, id, &tag) else {
            continue;
        };
        let travel = motion.suspension_travel[usize::from(wheel >= 2)];
        let (above, below) = if motion.physics_enabled {
            if travel >= 1.0 {
                (travel + 20.0, travel)
            } else {
                (20.0, 0.0)
            }
        } else {
            (64.0, 256.0)
        };
        let direction = if motion.physics_enabled {
            up
        } else {
            [0.0, 0.0, 1.0]
        };
        let start = std::array::from_fn(|i| hub[i] + direction[i] * above);
        let end = std::array::from_fn(|i| hub[i] - direction[i] * below);
        let hit = crate::frame::FrameWorld::from_world(world).trace_world_except(
            start,
            end,
            if motion.physics_enabled {
                MASK_WHEEL_PHYSICS
            } else {
                MASK_WHEEL_STATIC
            },
            exclude,
        );
        if hit.fraction < 1.0 || hit.startsolid != 0 {
            surfaces[wheel] = trace_iw4::surface_type_from_flags(hit.surface_flags);
        }
    }
    if let Some(vehicle) = world.resource_mut::<RoundScript>().vehicles.get_mut(&id) {
        vehicle.wheel_surfaces = surfaces;
    }
}
