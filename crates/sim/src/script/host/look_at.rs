use super::args::{arg, kind, string};
use super::natives::engine::entity_id;
use crate::bullet_collision::{BulletTraceQuery, ColliderId, TraceOutcome};
use crate::frame::FrameWorld;
use crate::script::{Namespace, NativeRegistry, RoundScript, Value};
use bevy_ecs::prelude::World;

pub(crate) fn register(registry: &mut NativeRegistry) {
    registry.register(
        Namespace::Method,
        "setlookattext",
        |world, receiver, args| {
            let id = entity_id(world, receiver)?;
            if matches!(arg(args, 0)?, Value::LocalizedString(_)) {
                return Err("parameter 1 is a localized string, not a string".into());
            }
            let name = string(args, 0)?;
            world
                .resource_mut::<RoundScript>()
                .entities
                .get_mut(&id)
                .unwrap()
                .look_at_name = Some(name.into());
            let description = match arg(args, 1)? {
                Value::LocalizedString(text) => text.clone(),
                value => {
                    return Err(format!(
                        "parameter 2 is {}, not a localized string",
                        kind(value)
                    ));
                }
            };
            world
                .resource_mut::<RoundScript>()
                .entities
                .get_mut(&id)
                .unwrap()
                .look_at_description = Some(description);
            Ok(Value::Undefined)
        },
    );
}

pub(crate) fn update(world: &mut World) {
    if world.resource::<RoundScript>().profile != asset_core::RuntimeProfile::Campaign {
        return;
    }
    let name_distance = world
        .resource::<crate::script::MatchScript>()
        .dvars
        .get("g_friendlynamedist")
        .and_then(|v| v.parse::<f32>().ok())
        .unwrap_or(15000.0);
    let clients = FrameWorld::from_world(world).client_ids_sorted();
    for client in clients {
        let overlay = select(world, client, name_distance);
        FrameWorld::from_world(world)
            .client_meta_mut(client)
            .actor_overlay = overlay;
    }
}

fn select(
    world: &mut World,
    client: crate::ClientId,
    name_distance: f32,
) -> Option<crate::ActorOverlay> {
    let frame = super::presence::settled(world);
    let ps = frame.player(client)?;
    if ps.health <= 0 {
        return None;
    }
    let eye = [
        ps.origin[0],
        ps.origin[1],
        ps.origin[2] + ps.view_height_current,
    ];
    let (forward, _, _) = math_iw4::angle_vectors(ps.viewangles);
    let end = core::array::from_fn(|i| eye[i] + forward[i] * 15000.0);
    let trace = frame.current_sensor_trace(BulletTraceQuery {
        start: eye,
        end,
        mask: 0x0280_e801,
        ignore: Some(client),
        ignore_hit: None,
        ignore_model: None,
    });
    let collider = match trace {
        TraceOutcome::Hit { collider, .. } => collider,
        TraceOutcome::StartSolid { collider, .. } => collider?,
        _ => return None,
    };
    let presence = match collider {
        ColliderId::EntityBody { owner }
        | ColliderId::EntityDObjBone { owner, .. }
        | ColliderId::EntityLinkedBrush { owner, .. } => owner.script_model()?,
        _ => return None,
    };
    let mut runtime = world.resource_mut::<RoundScript>();
    let (&id, entity) = runtime
        .entities
        .iter()
        .find(|(_, entity)| entity.presence == Some(presence))?;
    let name = entity.look_at_name.as_ref()?.to_string();
    let description = entity
        .look_at_description
        .as_deref()
        .unwrap_or("")
        .to_owned();
    let number = u16::try_from(entity.number).ok()?;
    let Value::Vector(origin) = runtime.object_field(id, "origin") else {
        return None;
    };
    let squared = (0..3).map(|i| (origin[i] - eye[i]).powi(2)).sum::<f32>();
    if squared >= name_distance * name_distance {
        return None;
    }
    Some(crate::ActorOverlay {
        entity: number,
        name,
        description,
        hostile: false,
    })
}
