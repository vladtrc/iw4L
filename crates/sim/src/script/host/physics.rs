use super::args::{arg, float, string, vector};
use crate::frame::FrameWorld;
use crate::script::Namespace::{Function, Method};
use crate::script::{NativeRegistry, RoundScript, Value};
use crate::world::ClientId;
use bevy_ecs::prelude::World;

const USE_RADIUS: f32 = 128.0;
const CAMPAIGN_USE_RADIUS: f32 = 72.0;
const HINTS: [&str; 5] = [
    "HINT_NONE",
    "HINT_NOICON",
    "HINT_ACTIVATE",
    "HINT_HEALTH",
    "HINT_FRIENDLY",
];

fn object_of(world: &World, receiver: &Value) -> Result<u64, String> {
    super::natives::engine::entity_id(world, receiver)
}

fn usable<'a>(
    world: &'a mut World,
    receiver: &Value,
) -> Result<&'a mut super::entities::Usable, String> {
    let object = object_of(world, receiver)?;
    let runtime = world.resource_mut::<RoundScript>().into_inner();
    let entity = runtime.entities.get_mut(&object).unwrap();
    let enabled = matches!(&*entity.classname, "trigger_use" | "trigger_use_touch");
    Ok(entity
        .usable
        .get_or_insert_with(|| super::entities::Usable {
            cursor: 1,
            hint: -1,
            enabled,
            barred: Default::default(),
        }))
}

pub(crate) fn register(registry: &mut NativeRegistry) {
    registry.register(Function, "physicsexplosionsphere", |world, _, args| {
        if args.len() != 4 {
            return Err(
                "PhysicsExplosionSphere requires origin, outer radius, inner radius and magnitude"
                    .into(),
            );
        }
        let center = vector(args, 0)?;
        let outer = float(args, 1)?;
        let inner = float(args, 2)?;
        let magnitude = float(args, 3)?;
        if center.iter().any(|v| !v.is_finite())
            || !outer.is_finite()
            || !inner.is_finite()
            || !magnitude.is_finite()
            || inner < 0.0
            || outer < inner
        {
            return Err(
                "physics sphere requires finite values and 0 <= inner radius <= outer radius"
                    .into(),
            );
        }
        world.resource_scope::<super::mechanics::Mechanics, _>(|world, mut mechanics| {
            mechanics.explode(
                &mut world.resource_mut::<RoundScript>(),
                center,
                outer,
                inner,
                magnitude,
            );
        });
        let tick = world.resource::<crate::step::StepRequest>().tick;
        FrameWorld::from_world(world).push_entity_event(
            tick,
            crate::EventAudience::All,
            entity_iw4::EntityEventKind::PHYS_EXPLOSION_SPHERE,
            crate::EntityEventPayload {
                number: i32::from(trace_iw4::ENTITYNUM_WORLD),
                origin: center,
                origin2: [outer, inner, magnitude],
                ..Default::default()
            },
        );
        Ok(Value::Undefined)
    });
    for name in ["physicslaunchserver", "physicslaunchclient"] {
        registry.register(Method, name, |world, receiver, args| {
            let object = object_of(world, receiver)?;
            vector(args, 0)?;
            let force = vector(args, 1)?;
            world
                .resource_mut::<super::mechanics::Mechanics>()
                .launch(object, force);
            let mut runtime = world.resource_mut::<RoundScript>();
            runtime.entities.get_mut(&object).unwrap().linked_to = None;
            Ok(Value::Undefined)
        });
    }
    registry.register(Method, "makeusable", |world, receiver, _| {
        usable(world, receiver)?.enabled = true;
        Ok(Value::Undefined)
    });
    registry.register(Method, "makeunusable", |world, receiver, _| {
        usable(world, receiver)?.enabled = false;
        Ok(Value::Undefined)
    });
    registry.register(Method, "setcursorhint", |world, receiver, args| {
        let name = string(args, 0)?;
        let cursor = HINTS
            .iter()
            .position(|hint| hint.eq_ignore_ascii_case(&name))
            .ok_or_else(|| format!("{name} is not a valid hint type"))?;
        usable(world, receiver)?.cursor = cursor as i32;
        Ok(Value::Undefined)
    });
    registry.register(Method, "sethintstring", |world, receiver, args| {
        let hint = super::hud::string_index(world, arg(args, 0)?)?;
        usable(world, receiver)?.hint = hint;
        Ok(Value::Undefined)
    });
    registry.register(Method, "disableplayeruse", |world, receiver, args| {
        let client = super::natives::player::player(world, arg(args, 0)?)?;
        usable(world, receiver)?.barred.insert(client);
        Ok(Value::Undefined)
    });
    registry.register(Method, "enableplayeruse", |world, receiver, args| {
        let client = super::natives::player::player(world, arg(args, 0)?)?;
        usable(world, receiver)?.barred.remove(&client);
        Ok(Value::Undefined)
    });
}

pub(crate) fn select_usables(world: &mut World) {
    super::triggers::refresh_claims(world);
    let usables: Vec<(
        u64,
        [f32; 3],
        super::entities::Usable,
        super::triggers::TriggerPolicy,
    )> = {
        let mut runtime = world.resource_mut::<RoundScript>();
        let candidates: Vec<(u64, super::entities::Usable, super::triggers::TriggerPolicy)> =
            runtime
                .entities
                .iter()
                .filter_map(|(object, e)| {
                    Some((
                        *object,
                        e.usable
                            .clone()
                            .or_else(|| {
                                // Authored use triggers start enabled even before
                                // a script supplies a custom cursor/hint string.
                                (&*e.classname != "trigger_use_touch"
                                    && (&*e.classname == "trigger_use"
                                        || e.classname.starts_with("trigger_use_")))
                                .then(|| super::entities::Usable {
                                    cursor: 1,
                                    hint: -1,
                                    enabled: true,
                                    barred: Default::default(),
                                })
                            })
                            .filter(|u| u.enabled)?,
                        e.trigger_policy.clone(),
                    ))
                })
                .collect();
        candidates
            .into_iter()
            .filter_map(
                |(object, usable, policy)| match runtime.object_field(object, "origin") {
                    Value::Vector(at) => Some((object, at, usable, policy)),
                    _ => None,
                },
            )
            .collect()
    };
    let clients: Vec<u32> = world
        .resource::<RoundScript>()
        .players
        .keys()
        .copied()
        .collect();
    let radius = if world.resource::<RoundScript>().profile == asset_core::RuntimeProfile::Campaign
    {
        CAMPAIGN_USE_RADIUS
    } else {
        USE_RADIUS
    };
    let runtime = world.resource::<RoundScript>();
    let mounted: std::collections::BTreeMap<_, _> = runtime
        .engine
        .turrets
        .iter()
        .filter_map(|(&object, t)| {
            Some((
                runtime.player_client(t.active_user?)?,
                (object, t.drop_hint),
            ))
        })
        .collect();
    let turret_numbers: std::collections::BTreeMap<_, _> = runtime
        .engine
        .turrets
        .keys()
        .filter_map(|id| Some((*id, runtime.entities.get(id)?.number)))
        .collect();
    let mut selected = std::collections::BTreeMap::new();
    let mut frame = FrameWorld::from_world(world);
    for client in clients {
        let id = ClientId(client);
        if !frame.client_meta(id).is_some_and(|m| {
            m.lifecycle == crate::ClientLifecycle::Alive && !m.controls.usability_disabled
        }) {
            continue;
        }
        let Some(ps) = frame.player(id).copied() else {
            continue;
        };
        if let Some(&(object, hint)) = mounted.get(&client) {
            selected.insert(client, object);
            let ps = frame.player_mut(id).unwrap();
            ps.cursor_hint = 1;
            ps.cursor_hint_ent_index = ps.viewlocked_ent_num;
            ps.cursor_hint_string = hint;
            continue;
        }
        let eye = [
            ps.origin[0],
            ps.origin[1],
            ps.origin[2] + ps.view_height_current,
        ];
        let nearest = usables
            .iter()
            .filter(|(_, _, usable, policy)| {
                !usable.barred.contains(&client) && policy.allows(&frame, client)
            })
            .filter(|(_, at, _, _)| {
                frame.content().atlas().is_none()
                    || crate::world_graph::world_at(*at) == crate::world_graph::world_at(ps.origin)
            })
            .map(|(object, at, usable, _)| {
                let d2: f32 = (0..3).map(|i| (at[i] - eye[i]).powi(2)).sum();
                (d2, *object, usable)
            })
            .filter(|(d2, _, _)| *d2 <= radius * radius)
            .min_by(|a, b| a.0.total_cmp(&b.0));
        let Some((_, object, usable)) = nearest else {
            continue;
        };
        selected.insert(client, object);
        if let Some(ps) = frame.player_mut(id)
            && ps.cursor_hint == 0
        {
            if let Some(number) = turret_numbers.get(&object) {
                ps.cursor_hint_ent_index = *number;
            }
            ps.cursor_hint = usable.cursor;
            ps.cursor_hint_string = usable.hint;
        }
    }
    world.resource_mut::<RoundScript>().use_selected = selected;
}
