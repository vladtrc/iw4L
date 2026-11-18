use super::*;
use crate::EntityBodyBounds;
use crate::frame::FrameWorld;

const PLAYER_FLOOR_MASK: u32 = 0x810011;
const ACTOR_FLOOR_MASK: u32 = 0x820015;

pub(crate) const ACTOR_BOUNDS: EntityBodyBounds = EntityBodyBounds {
    midpoint: [0.0, 0.0, 36.0],
    half_size: [15.0, 15.0, 36.0],
    contents: 0x4000,
};

pub(crate) fn floor_position(
    world: &mut World,
    origin: [f32; 3],
    client: Option<u32>,
) -> Result<Option<[f32; 3]>, String> {
    if !origin.into_iter().all(f32::is_finite) {
        return Err("floor origin must be finite".into());
    }
    let (height, mask) = if let Some(client) = client {
        let frame = FrameWorld::from_world(world);
        let ps = frame
            .player(crate::ClientId(client))
            .ok_or("sentient player has no physical state")?;
        let height = if ps.pm_flags & 1 != 0 {
            30.0
        } else if ps.pm_flags & 2 != 0 {
            50.0
        } else {
            70.0
        };
        (height, PLAYER_FLOOR_MASK)
    } else {
        (72.0, ACTOR_FLOOR_MASK)
    };
    let start = [origin[0], origin[1], origin[2] + height - 30.0];
    let end = [origin[0], origin[1], origin[2] - 1024.0];
    let hit = super::super::natives::engine::trace(
        world,
        start,
        end,
        [-15.0, -15.0, 0.0],
        [15.0, 15.0, 30.0],
        mask,
    );
    Ok((hit.startsolid == 0)
        .then(|| std::array::from_fn(|i| start[i] + (end[i] - start[i]) * hit.fraction)))
}

pub(crate) fn initialize_physics(world: &mut World, id: u64) -> Result<(), String> {
    let (origin, angles) = {
        let mut runtime = world.resource_mut::<RoundScript>();
        let actor = runtime.actors.get(&id).ok_or("entity is not an actor")?;
        if actor.physics_initialized {
            return Err("actor physics already initialized".into());
        }
        if runtime.entities[&id].presence.is_some() {
            return Err("actor already owns model presence".into());
        }
        let Value::Vector(origin) = runtime.object_field(id, "origin") else {
            return Err("actor origin is not a vector".into());
        };
        let Value::Vector(angles) = runtime.object_field(id, "angles") else {
            return Err("actor angles are not a vector".into());
        };
        if !angles.into_iter().all(f32::is_finite) {
            return Err("actor angles must be finite".into());
        }
        (origin, angles)
    };
    let origin = floor_position(world, origin, None)?.unwrap_or(origin);
    let presence = super::super::presence::spawn_presence(world, origin)?;
    let mut frame = FrameWorld::from_world(world);
    let number = frame
        .gentity_number(presence)
        .ok_or("actor presence has no entity number")?;
    let mover = frame
        .script_mover_mut_by_number(number)
        .ok_or("actor presence is missing")?;
    mover.body = Some(ACTOR_BOUNDS);
    mover.box_mid = ACTOR_BOUNDS.midpoint;
    mover.box_half = ACTOR_BOUNDS.half_size;
    mover.state.apos_tr_base = angles;
    frame.insert_collision_owner(crate::entity_body::owner_pose(
        presence,
        ACTOR_BOUNDS,
        origin,
        number as u16,
    ));
    let mut runtime = world.resource_mut::<RoundScript>();
    let entity = runtime.entities.get_mut(&id).unwrap();
    entity.presence = Some(presence);
    entity.number = number;
    entity.contents = ACTOR_BOUNDS.contents as i32;
    entity.solid = true;
    entity.hidden = false;
    entity.can_damage = true;
    entity.can_radius_damage = true;
    runtime.set_object_field(id, "origin", Value::Vector(origin));
    runtime.set_object_field(id, "health", Value::Int(100));
    runtime.set_object_field(id, "maxhealth", Value::Int(100));
    let actor = runtime.actors.get_mut(&id).unwrap();
    actor.goals.initialize(origin);
    actor.orientation.set_angles(angles);
    actor.motion.set_angles(angles);
    actor.alive = true;
    actor.physics_initialized = true;
    Ok(())
}

pub(crate) fn initialize_pending(world: &mut World) -> Result<(), String> {
    let pending: Vec<_> = world
        .resource::<RoundScript>()
        .actors
        .iter()
        .filter(|(_, actor)| !actor.physics_initialized)
        .map(|(&id, _)| id)
        .collect();
    for id in pending {
        initialize_physics(world, id)?;
    }
    Ok(())
}

pub(super) fn register(registry: &mut NativeRegistry) {
    registry.register(
        Namespace::Method,
        "getdroptofloorposition",
        |world, receiver, args| {
            let id = entity_id(world, receiver)?;
            let runtime = world.resource::<RoundScript>();
            if !runtime.sentients.contains_key(&id) {
                return Err("entity is not a sentient".into());
            }
            let client = runtime.player_client(id);
            if client.is_none() && !runtime.actors.contains_key(&id) {
                return Ok(Value::Undefined);
            }
            let origin = if args.is_empty() {
                if let Some(client) = client {
                    FrameWorld::from_world(world)
                        .player(crate::ClientId(client))
                        .ok_or("sentient player has no physical state")?
                        .origin
                } else {
                    let Value::Vector(origin) = world
                        .resource_mut::<RoundScript>()
                        .object_field(id, "origin")
                    else {
                        return Err("sentient origin is not a vector".into());
                    };
                    origin
                }
            } else {
                super::super::args::vector(args, 0)?
            };
            Ok(floor_position(world, origin, client)?.map_or(Value::Undefined, Value::Vector))
        },
    );
}
