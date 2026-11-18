use super::*;
use bevy_ecs::prelude::World;

pub(super) fn install(runtime: &mut RoundScript) {
    let entities: Vec<_> = runtime
        .entities
        .iter()
        .filter_map(|(&id, entity)| {
            (entity.classname.as_ref() == "script_brushmodel" && entity.brush.is_some())
                .then_some(id)
        })
        .collect();
    for id in entities {
        if matches!(runtime.object_field(id,"spawnflags"),Value::Int(flags) if flags & 1 != 0) {
            runtime.path_nodes.blockers.insert(id, Vec::new());
        }
    }
}

fn validate(world: &World, receiver: &Value, action: &str) -> Result<u64, String> {
    let id = super::super::natives::engine::entity_id(world, receiver)?;
    if !world
        .resource::<RoundScript>()
        .path_nodes
        .blockers
        .contains_key(&id)
    {
        let classname = &world.resource::<RoundScript>().entities[&id].classname;
        return Err(if classname.as_ref() == "script_brushmodel" {
            format!("script_brushmodel must have DYNAMICPATH set to {action} paths")
        } else {
            format!("entity of type '{classname}' cannot {action} paths")
        });
    }
    Ok(id)
}

impl PathNodes {
    pub(crate) fn connect(&mut self, id: u64) {
        let Some(blocker) = self.blockers.get_mut(&id) else {
            return;
        };
        for (from, to) in std::mem::take(blocker) {
            let links = &mut self.links[from];
            if let Some(index) = links
                .iter()
                .position(|link| link.node == to && link.disconnect_count != 0)
            {
                links[index].disconnect_count -= 1;
                if links[index].disconnect_count == 0 {
                    let link = links.remove(index);
                    let at = links
                        .iter()
                        .position(|other| other.disconnect_count != 0 || other.node >= to)
                        .unwrap_or(links.len());
                    links.insert(at, link);
                }
            }
        }
    }

    fn disconnect_link(&mut self, id: u64, from: usize, to: u16) -> Result<(), String> {
        let links = &mut self.links[from];
        let index = links
            .iter()
            .position(|link| link.node == to)
            .ok_or("path link is missing")?;
        links[index].disconnect_count = links[index]
            .disconnect_count
            .checked_add(1)
            .ok_or("path link disconnect count overflow")?;
        if links[index].disconnect_count == 1 {
            let link = links.remove(index);
            let at = links
                .iter()
                .position(|other| other.disconnect_count != 0 && other.node > to)
                .unwrap_or(links.len());
            links.insert(at, link);
        }
        self.blockers.get_mut(&id).unwrap().push((from, to));
        Ok(())
    }

    pub(crate) fn remove_blocker(&mut self, id: u64) {
        self.connect(id);
        self.blockers.remove(&id);
    }
}

pub(crate) fn initialize(world: &mut World) -> Result<(), String> {
    let ids: Vec<_> = world
        .resource::<RoundScript>()
        .path_nodes
        .blockers
        .keys()
        .copied()
        .collect();
    for id in ids {
        disconnect(world, id)?;
    }
    Ok(())
}

fn disconnect(world: &mut World, id: u64) -> Result<(), String> {
    let (brush, model, origin, angles, solid, contents, nodes) = {
        let mut runtime = world.resource_mut::<RoundScript>();
        runtime.path_nodes.connect(id);
        let entity = runtime
            .entities
            .get(&id)
            .ok_or("path blocker entity is missing")?;
        let (brush, solid, contents) = (entity.brush, entity.solid, entity.contents);
        let model = match runtime.object_field(id, "model") {
            Value::String(name) => Some(name),
            _ => None,
        };
        let Value::Vector(origin) = runtime.object_field(id, "origin") else {
            return Err("invalid blocker origin".into());
        };
        let Value::Vector(angles) = runtime.object_field(id, "angles") else {
            return Err("invalid blocker angles".into());
        };
        (
            brush,
            model,
            origin,
            angles,
            solid,
            contents,
            runtime.path_nodes.data.clone(),
        )
    };
    if nodes.is_empty() || !solid || (contents != 0 && contents & 0x2820015 == 0) {
        return Ok(());
    }
    let blocked = {
        let frame = crate::frame::FrameWorld::from_world(world);
        let brush_model = brush
            .map(|brush| {
                frame
                    .clip_cmodels()
                    .models
                    .get(brush as usize)
                    .ok_or("path blocker collision model is missing")
            })
            .transpose()?;
        let capsule = if brush_model.is_none() {
            let name = model
                .as_deref()
                .ok_or("vehicle path blocker has no model")?;
            let capability = frame.model_capability(name).flatten().ok_or_else(|| {
                format!("vehicle path blocker model collision is unavailable: {name}")
            })?;
            let (mid, half) = capability
                .bounds
                .ok_or("vehicle path blocker model bounds are unavailable")?;
            Some((
                std::array::from_fn(|i| mid[i] - half[i]),
                std::array::from_fn(|i| mid[i] + half[i]),
            ))
        } else {
            None
        };
        let mut blocked = Vec::new();
        for (from, node) in nodes.iter().enumerate() {
            for link in node.links.iter().rev() {
                let target = nodes
                    .get(usize::from(link.node))
                    .ok_or("path link target is out of range")?;
                let extents = clipmap_iw4::TraceExtents {
                    shape: trace_iw4::TraceShape::Capsule,
                    start: node.origin,
                    end: target.origin,
                    mins: [-15.0, -15.0, 10.0],
                    maxs: [15.0, 15.0, 72.0],
                    mask: 0x2820015,
                };
                let blocked_by_entity = if let Some(model) = brush_model {
                    clipmap_iw4::transformed_box_sight_trace(
                        model,
                        &frame.clip_bsp().leafbrushes,
                        frame.clip_brushes(),
                        &extents,
                        origin,
                        angles,
                    )
                } else if let Some((mins, maxs)) = capsule {
                    let hit = clipmap_iw4::transformed_temp_capsule_trace(
                        &extents,
                        origin,
                        mins,
                        maxs,
                        contents as u32,
                    );
                    hit.startsolid != 0 || hit.fraction < 1.0
                } else {
                    unreachable!()
                };
                if blocked_by_entity {
                    blocked.push((from, link.node));
                }
            }
        }
        blocked
    };
    let mut runtime = world.resource_mut::<RoundScript>();
    for (from, to) in blocked {
        runtime.path_nodes.disconnect_link(id, from, to)?;
    }
    Ok(())
}

pub(super) fn register(registry: &mut NativeRegistry) {
    registry.register(Namespace::Method, "connectpaths", |world, receiver, _| {
        let id = validate(world, receiver, "connect")?;
        world.resource_mut::<RoundScript>().path_nodes.connect(id);
        Ok(Value::Undefined)
    });
    registry.register(
        Namespace::Method,
        "disconnectpaths",
        |world, receiver, _| {
            let id = validate(world, receiver, "disconnect")?;
            disconnect(world, id)?;
            Ok(Value::Undefined)
        },
    );
}
