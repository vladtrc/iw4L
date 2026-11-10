use super::*;
use crate::bullet_collision::{PLAYER_MAXS, PLAYER_MINS};

pub(crate) fn initialize(world: &mut World, client: ClientId) -> Result<(), String> {
    let mut runtime = world.resource_mut::<RoundScript>();
    if runtime.profile != asset_core::RuntimeProfile::Campaign || client.0 != 0 {
        return Err("campaign startup requires the single local client0".into());
    }
    if !runtime.players.is_empty() {
        return Err("campaign player already initialized".into());
    }
    let points: Vec<_> = runtime
        .entities
        .iter()
        .filter(|(_, entity)| entity.classname.as_ref() == "info_player_start")
        .map(|(id, _)| *id)
        .collect();
    if points.is_empty() {
        return Err("campaign map has no info_player_start".into());
    }
    let mut starts = Vec::with_capacity(points.len());
    for point in points {
        let (Value::Vector(origin), Value::Vector(angles)) = (
            runtime.object_field(point, "origin"),
            runtime.object_field(point, "angles"),
        ) else {
            return Err("campaign spawn has invalid origin or angles".into());
        };
        if !origin
            .iter()
            .chain(angles.iter())
            .all(|value| value.is_finite())
        {
            return Err("campaign spawn has non-finite origin or angles".into());
        }
        let initial = matches!(runtime.object_field(point, "spawnflags"), Value::Int(flags) if flags & 1 != 0);
        starts.push((point, origin, angles, initial));
    }
    for start in &mut starts {
        start.1 = place_spawn(world, start.1)?;
        world.resource_mut::<RoundScript>().set_object_field(
            start.0,
            "origin",
            Value::Vector(start.1),
        );
    }
    let selected = if let Some(initial) = starts.iter().position(|start| start.3) {
        initial
    } else {
        let nearest = starts
            .iter()
            .enumerate()
            .min_by(|(_, a), (_, b)| {
                super::super::natives::math::distance_sq(a.1, [0.; 3])
                    .total_cmp(&super::super::natives::math::distance_sq(b.1, [0.; 3]))
            })
            .unwrap()
            .0;
        let mut selected = super::super::natives::math::random(world) as usize % starts.len();
        for _ in 0..2 {
            if selected != nearest {
                break;
            }
            selected = super::super::natives::math::random(world) as usize % starts.len();
        }
        selected
    };
    let (_, mut origin, angles, _) = starts[selected];
    origin[2] += 9.0;
    let mut runtime = world.resource_mut::<RoundScript>();
    let object = runtime.create_player(client.0)?;
    runtime.entities.get_mut(&object).unwrap().contents = 0x0200_0000;
    if let Err(error) = super::super::sentients::allocate(&mut runtime, object, 2, false) {
        runtime.delete_entity(object);
        return Err(error);
    }
    let mut slot = PlayerSlot::new(object);
    slot.begun = true;
    slot.sessionstate = "playing".into();
    runtime.players.insert(client.0, slot);
    let tick = world
        .get_resource::<crate::step::StepRequest>()
        .map_or(crate::Tick(0), |request| request.tick);
    let mut frame = FrameWorld::from_world(world);
    {
        let meta = frame.client_meta_mut(client);
        meta.client_state_team = entity_iw4::TEAM_ALLIES;
        meta.max_health = 100;
    }
    crate::script_player::spawn(&mut frame, tick, client, origin, angles, "playing");
    frame.set_phase(crate::MatchPhase::Playing);
    Ok(())
}

const SPAWN_TRACE_MASK: u32 = 0x0281_c011;

fn place_spawn(world: &mut World, origin: [f32; 3]) -> Result<[f32; 3], String> {
    let trace = |world: &mut World, start, end| {
        super::super::natives::engine::trace(
            world,
            start,
            end,
            PLAYER_MINS,
            PLAYER_MAXS,
            SPAWN_TRACE_MASK,
        )
    };
    let up = [origin[0], origin[1], origin[2] + 128.0];
    let hit = trace(world, origin, up);
    if hit.startsolid != 0 || hit.allsolid != 0 {
        return Err("campaign info_player_start is in solid".into());
    }
    let raised: [f32; 3] = std::array::from_fn(|i| origin[i] + (up[i] - origin[i]) * hit.fraction);
    let down = [raised[0], raised[1], raised[2] - 256.0];
    let hit = trace(world, raised, down);
    if hit.startsolid != 0 || hit.allsolid != 0 || hit.fraction >= 1.0 {
        return Err("campaign info_player_start has no clear floor".into());
    }
    let landed: [f32; 3] =
        std::array::from_fn(|i| raised[i] + (down[i] - raised[i]) * hit.fraction);
    let hit = trace(world, landed, landed);
    if hit.startsolid != 0 || hit.allsolid != 0 {
        return Err("campaign info_player_start floor is in solid".into());
    }
    Ok(landed)
}
